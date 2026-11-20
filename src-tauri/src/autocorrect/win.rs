//! Windows runtime for typing autocorrect: a low-level keyboard/mouse hook that only forwards
//! events, a worker thread that decides, and an in-place replacer built on `SendInput`.
//! Typed text lives in memory only and is never logged or persisted.
use std::collections::{HashSet, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThreadId, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::core::Interface;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern, IUIAutomationValuePattern,
    UIA_TextPatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, GetKeyboardLayout, SendInput, ToUnicodeEx, INPUT, INPUT_0, INPUT_KEYBOARD,
    KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_CAPITAL,
    VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_INSERT, VK_LCONTROL, VK_LEFT, VK_LMENU,
    VK_LSHIFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU, VK_RSHIFT,
    VK_RWIN, VK_SHIFT, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
    PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_MBUTTONDOWN,
    WM_QUIT, WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN,
};

use super::engine::Engine;
use super::grammar::GrammarChecker;
use super::rules::ends_sentence;
use super::tracker::{Boundary, Event, Tracker};
use super::{bundled_engine, chip, evaluate, excluded_apps, Status};
use crate::store::Store;

const MAX_RETYPE_AFTER: usize = 6;
const MAX_LIVE_AFTER: usize = 24;
const MAX_ACCEPT_BACKSPACES: usize = 300;
const MIN_GRAMMAR_WORDS: usize = 3;
const USER_WORDS_TTL: Duration = Duration::from_secs(10);

static ENGINE: OnceLock<Arc<Engine>> = OnceLock::new();
static ENGINE_BUILDING: AtomicBool = AtomicBool::new(false);
static GRAMMAR: OnceLock<Mutex<GrammarChecker>> = OnceLock::new();
static GRAMMAR_BUILDING: AtomicBool = AtomicBool::new(false);
static TX: Mutex<Option<Sender<Msg>>> = Mutex::new(None);
static HOOK_THREAD: Mutex<Option<(u32, JoinHandle<()>)>> = Mutex::new(None);
static ENABLED: AtomicBool = AtomicBool::new(false);
static EPOCH: AtomicU64 = AtomicU64::new(0);
static FIXED: AtomicUsize = AtomicUsize::new(0);
static SUGGESTED: AtomicUsize = AtomicUsize::new(0);
static KEYS: AtomicUsize = AtomicUsize::new(0);
static NOTE: Mutex<String> = Mutex::new(String::new());

static SHIFT_DOWN: AtomicBool = AtomicBool::new(false);
static CTRL_DOWN: AtomicBool = AtomicBool::new(false);
static ALT_DOWN: AtomicBool = AtomicBool::new(false);
static WIN_DOWN: AtomicBool = AtomicBool::new(false);

enum Msg {
    Key { event: Event, hwnd: isize },
    Accept(u64),
    Dismiss { id: u64, ignore: bool },
}

fn post(msg: Msg) {
    if let Ok(guard) = TX.lock() {
        if let Some(tx) = guard.as_ref() {
            let _ = tx.send(msg);
        }
    }
}

/// The user clicked the chip's fix button.
pub fn accept(id: u64) {
    post(Msg::Accept(id));
}

/// The user closed the chip, or chose "Never" for a word.
pub fn dismiss(id: u64, ignore: bool) {
    post(Msg::Dismiss { id, ignore });
}

pub fn status() -> Status {
    Status {
        enabled: ENABLED.load(Ordering::SeqCst),
        ready: ENGINE.get().is_some(),
        fixed: FIXED.load(Ordering::SeqCst),
        suggested: SUGGESTED.load(Ordering::SeqCst),
        keys: KEYS.load(Ordering::SeqCst),
        note: NOTE.lock().unwrap().clone(),
    }
}

/// Text was changed by something other than the user's keys (dictation paste, undo):
/// whatever the tracker believes about the field is stale.
pub fn note_external_edit() {
    EPOCH.fetch_add(1, Ordering::SeqCst);
}

pub fn set_enabled(app: &AppHandle, on: bool) {
    stop();
    if !on {
        return;
    }
    ensure_engine();
    ensure_grammar();
    let chip_app = app.clone();
    thread::spawn(move || chip::warm_up(&chip_app));
    ENABLED.store(true, Ordering::SeqCst);

    let (tx, rx) = mpsc::channel::<Msg>();
    *TX.lock().unwrap() = Some(tx);

    let worker_app = app.clone();
    thread::spawn(move || run_worker(worker_app, rx));

    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
    let join = thread::spawn(move || run_hook_thread(ready_tx));
    match ready_rx.recv() {
        Ok(Ok(thread_id)) => *HOOK_THREAD.lock().unwrap() = Some((thread_id, join)),
        Ok(Err(e)) => {
            log::error!("Autocorrect hook failed: {e}");
            stop();
        }
        Err(e) => {
            log::error!("Autocorrect hook thread vanished: {e}");
            stop();
        }
    }
}

fn stop() {
    ENABLED.store(false, Ordering::SeqCst);
    *TX.lock().unwrap() = None;
    if let Some((thread_id, join)) = HOOK_THREAD.lock().unwrap().take() {
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        let _ = join.join();
    }
    for flag in [&SHIFT_DOWN, &CTRL_DOWN, &ALT_DOWN, &WIN_DOWN] {
        flag.store(false, Ordering::SeqCst);
    }
}

fn ensure_engine() {
    if ENGINE.get().is_some() || ENGINE_BUILDING.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(|| {
        let started = Instant::now();
        let _ = ENGINE.set(Arc::new(bundled_engine()));
        log::info!("Autocorrect engine ready in {:?}", started.elapsed());
    });
}

fn ensure_grammar() {
    if GRAMMAR.get().is_some() || GRAMMAR_BUILDING.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(|| {
        let started = Instant::now();
        let _ = GRAMMAR.set(Mutex::new(GrammarChecker::new()));
        log::info!("Autocorrect grammar checker ready in {:?}", started.elapsed());
    });
}

// ---- hook thread -------------------------------------------------------------------------

fn run_hook_thread(ready: Sender<Result<u32, String>>) {
    let keyboard = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) };
    let keyboard = match keyboard {
        Ok(h) => h,
        Err(e) => {
            let _ = ready.send(Err(format!("keyboard hook: {e}")));
            return;
        }
    };
    let mouse = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0) };
    let mouse = match mouse {
        Ok(h) => h,
        Err(e) => {
            unsafe {
                let _ = UnhookWindowsHookEx(keyboard);
            }
            let _ = ready.send(Err(format!("mouse hook: {e}")));
            return;
        }
    };
    let thread_id = unsafe { GetCurrentThreadId() };
    if ready.send(Ok(thread_id)).is_err() {
        unsafe {
            let _ = UnhookWindowsHookEx(keyboard);
            let _ = UnhookWindowsHookEx(mouse);
        }
        return;
    }
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(keyboard);
        let _ = UnhookWindowsHookEx(mouse);
    }
}

fn send(event: Event) {
    let hwnd = unsafe { GetForegroundWindow() }.0 as isize;
    if let Ok(guard) = TX.lock() {
        if let Some(tx) = guard.as_ref() {
            let _ = tx.send(Msg::Key { event, hwnd });
        }
    }
}

fn is_modifier(vk: u16) -> bool {
    [VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_CONTROL, VK_LCONTROL, VK_RCONTROL, VK_MENU, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN, VK_CAPITAL]
        .iter()
        .any(|k| k.0 == vk)
}

fn track_modifier(vk: u16, down: bool) {
    let flag = match vk {
        v if v == VK_SHIFT.0 || v == VK_LSHIFT.0 || v == VK_RSHIFT.0 => &SHIFT_DOWN,
        v if v == VK_CONTROL.0 || v == VK_LCONTROL.0 || v == VK_RCONTROL.0 => &CTRL_DOWN,
        v if v == VK_MENU.0 || v == VK_LMENU.0 || v == VK_RMENU.0 => &ALT_DOWN,
        v if v == VK_LWIN.0 || v == VK_RWIN.0 => &WIN_DOWN,
        _ => return,
    };
    flag.store(down, Ordering::SeqCst);
}

/// vk + scan -> the character the focused layout would type, or None.
/// Dead keys and multi-char results return Some(Err) so the caller resets.
fn translate(vk: u32, scan: u32) -> Option<Result<char, ()>> {
    let mut state = [0u8; 256];
    if SHIFT_DOWN.load(Ordering::SeqCst) {
        state[VK_SHIFT.0 as usize] = 0x80;
    }
    if CTRL_DOWN.load(Ordering::SeqCst) {
        state[VK_CONTROL.0 as usize] = 0x80;
    }
    if ALT_DOWN.load(Ordering::SeqCst) {
        state[VK_MENU.0 as usize] = 0x80;
    }
    if unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) } & 1 != 0 {
        state[VK_CAPITAL.0 as usize] = 1;
    }
    let mut buf = [0u16; 8];
    let layout = unsafe {
        let mut pid = 0u32;
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
        GetKeyboardLayout(thread)
    };
    // Flag 0x4: do not change the keyboard state (keeps dead keys intact for the real app).
    let n = unsafe { ToUnicodeEx(vk, scan, &state, &mut buf, 0x4, Some(layout)) };
    match n {
        0 => None,
        1 => Some(char::from_u32(u32::from(buf[0])).ok_or(())),
        _ => Some(Err(())),
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let msg = wparam.0 as u32;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
        // Injected input (our own edits, dictation paste, other tools) never reaches the tracker.
        if !info.flags.contains(LLKHF_INJECTED) && (is_down || is_up) {
            let vk = info.vkCode as u16;
            if is_modifier(vk) {
                track_modifier(vk, is_down);
            } else if is_down {
                if let Some(event) = key_event(info.vkCode, info.scanCode) {
                    send(event);
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn key_event(vk: u32, scan: u32) -> Option<Event> {
    let v = vk as u16;
    if v == VK_BACK.0 {
        return Some(if CTRL_DOWN.load(Ordering::SeqCst) { Event::Reset } else { Event::Backspace });
    }
    if v == VK_RETURN.0 {
        return Some(Event::Enter);
    }
    let navigation = [VK_TAB, VK_ESCAPE, VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN, VK_HOME, VK_END, VK_PRIOR, VK_NEXT, VK_DELETE, VK_INSERT];
    if navigation.iter().any(|k| k.0 == v) {
        return Some(Event::Reset);
    }
    let ctrl = CTRL_DOWN.load(Ordering::SeqCst);
    let alt = ALT_DOWN.load(Ordering::SeqCst);
    let win = WIN_DOWN.load(Ordering::SeqCst);
    // Ctrl+Alt together is AltGr on many layouts; any other modifier combo is a shortcut.
    if win || ((ctrl || alt) && !(ctrl && alt)) {
        return Some(Event::Reset);
    }
    match translate(vk, scan) {
        None => None,
        Some(Ok(c)) if !c.is_control() => Some(Event::Char(c)),
        Some(_) => Some(Event::Reset),
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = wparam.0 as u32;
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let on_chip = chip::contains(info.pt.x, info.pt.y);
        if !on_chip
            && (msg == WM_LBUTTONDOWN || msg == WM_RBUTTONDOWN || msg == WM_MBUTTONDOWN || msg == WM_XBUTTONDOWN)
        {
            send(Event::Reset);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

// ---- worker ------------------------------------------------------------------------------

struct LastFix {
    before: String,
    after: String,
    original_lower: String,
}

/// A suggestion currently offered on the chip. It stays valid only while the user keeps typing
/// forward in the same field, because accepting it rewrites everything since `tail` began.
struct Live {
    id: u64,
    kind: &'static str,
    original: String,
    replacement: String,
    /// On-screen text from where the change starts up to (not including) the delimiter.
    tail: String,
    replacement_tail: String,
    delimiter: char,
    typed_after: String,
    hwnd: isize,
}

struct Worker {
    app: AppHandle,
    automation: Option<IUIAutomation>,
    tracker: Tracker,
    hwnd: isize,
    epoch: u64,
    checked: bool,
    suspended: bool,
    /// The field the safety check approved; fixes stop if focus moves off it.
    field: Option<IUIAutomationElement>,
    last_fix: Option<LastFix>,
    session_ignore: HashSet<String>,
    user_words: HashSet<String>,
    user_words_at: Option<Instant>,
    exe_cache: (isize, String),
    live: Option<Live>,
    next_id: u64,
    /// Everything typed since the current sentence began, exactly as it is on screen.
    sentence: String,
    /// The sentence began after Enter or a sentence end, so `sentence` is all of it.
    sentence_complete: bool,
}

fn run_worker(app: AppHandle, rx: Receiver<Msg>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let automation = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok() };
    let mut worker = Worker {
        app,
        automation,
        tracker: Tracker::new(),
        hwnd: 0,
        epoch: EPOCH.load(Ordering::SeqCst),
        checked: false,
        suspended: false,
        field: None,
        last_fix: None,
        session_ignore: HashSet::new(),
        user_words: HashSet::new(),
        user_words_at: None,
        exe_cache: (0, String::new()),
        live: None,
        next_id: 0,
        sentence: String::new(),
        sentence_complete: false,
    };
    let mut pending: VecDeque<Msg> = VecDeque::new();
    loop {
        let msg = match pending.pop_front() {
            Some(m) => m,
            None => match rx.recv() {
                Ok(m) => m,
                Err(_) => break,
            },
        };
        worker.handle(msg, &rx, &mut pending);
    }
    worker.drop_live();
}

fn foreground_hwnd() -> isize {
    unsafe { GetForegroundWindow() }.0 as isize
}

fn key_is_down(vk: VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(i32::from(vk.0)) } as u16) & 0x8000 != 0
}

fn modifiers_held() -> bool {
    [VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN].iter().any(|k| key_is_down(*k))
}

fn process_elevated(process: HANDLE) -> Option<bool> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0u32;
        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        let _ = CloseHandle(token);
        result.ok()?;
        Some(elevation.TokenIsElevated != 0)
    }
}

/// Can we inject into this window? A higher-integrity target silently drops our input.
fn target_reachable(hwnd: isize) -> bool {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(HWND(hwnd as *mut c_void), Some(&mut pid));
        if pid == 0 {
            return false;
        }
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let target = process_elevated(process);
        let _ = CloseHandle(process);
        let me = process_elevated(GetCurrentProcess());
        match (target, me) {
            (Some(false), _) => true,
            (Some(true), Some(true)) => true,
            _ => false,
        }
    }
}

fn key_input(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    }
}

/// Delete `backspaces` characters, then type `text`, as one atomic `SendInput` call.
fn replace_text(backspaces: usize, text: &str) -> bool {
    let mut inputs: Vec<INPUT> = Vec::with_capacity(backspaces * 2 + text.len() * 4);
    for _ in 0..backspaces {
        inputs.push(key_input(VK_BACK.0, 0, KEYBD_EVENT_FLAGS(0)));
        inputs.push(key_input(VK_BACK.0, 0, KEYEVENTF_KEYUP));
    }
    for unit in text.encode_utf16() {
        inputs.push(key_input(0, unit, KEYEVENTF_UNICODE));
        inputs.push(key_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
    }
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    sent as usize == inputs.len()
}

/// Is the focused field's text empty? Unknown (no text interface) counts as not empty.
fn field_is_empty(element: &IUIAutomationElement) -> bool {
    unsafe {
        if let Ok(value) = element
            .GetCurrentPattern(UIA_ValuePatternId)
            .and_then(|p| p.cast::<IUIAutomationValuePattern>())
            .and_then(|p| p.CurrentValue())
        {
            return value.is_empty();
        }
        element
            .GetCurrentPattern(UIA_TextPatternId)
            .and_then(|p| p.cast::<IUIAutomationTextPattern>())
            .and_then(|p| p.DocumentRange())
            .and_then(|r| r.GetText(1))
            .map(|t| t.is_empty())
            .unwrap_or(false)
    }
}

/// Remember (and log once per change) why autocorrect is or isn't acting in the current app.
fn set_note(exe: &str, reason: &str) {
    let note = format!("{exe}: {reason}");
    let mut last = NOTE.lock().unwrap();
    if *last != note {
        log::info!("Autocorrect {note}");
        *last = note;
    }
}

fn pop_chars(s: &mut String, n: usize) {
    for _ in 0..n {
        s.pop();
    }
}

impl Worker {
    fn hard_reset(&mut self) {
        self.tracker.feed(Event::Reset);
        self.checked = false;
        self.suspended = false;
        self.field = None;
        self.last_fix = None;
        self.sentence.clear();
        self.sentence_complete = false;
        self.drop_live();
    }

    fn drop_live(&mut self) {
        if self.live.take().is_some() {
            chip::hide(&self.app);
        }
    }

    fn handle(&mut self, msg: Msg, rx: &Receiver<Msg>, pending: &mut VecDeque<Msg>) {
        let (event, hwnd) = match msg {
            Msg::Key { event, hwnd } => (event, hwnd),
            Msg::Accept(id) => {
                self.accept(id);
                return;
            }
            Msg::Dismiss { id, ignore } => {
                self.dismiss(id, ignore);
                return;
            }
        };
        let epoch = EPOCH.load(Ordering::SeqCst);
        if epoch != self.epoch {
            self.epoch = epoch;
            self.hard_reset();
        }
        if hwnd != self.hwnd {
            self.hwnd = hwnd;
            self.hard_reset();
        }
        if event == Event::Reset {
            self.hard_reset();
            return;
        }
        self.follow_live(event);
        if event == Event::Backspace {
            if let Some(last) = self.last_fix.take() {
                self.undo(last);
                return;
            }
        }
        self.last_fix = None;

        if let Event::Char(_) = event {
            KEYS.fetch_add(1, Ordering::SeqCst);
        }
        if !self.checked {
            if let Event::Char(c) = event {
                if !c.is_whitespace() {
                    self.suspended = !self.safe_to_correct(hwnd);
                    self.checked = true;
                }
            }
        }
        if self.suspended {
            return;
        }
        match event {
            Event::Char(c) => self.sentence.push(c),
            Event::Backspace => {
                self.sentence.pop();
            }
            Event::Enter => {
                self.sentence.clear();
                self.sentence_complete = true;
            }
            Event::Reset => {}
        }
        let Some(boundary) = self.tracker.feed(event) else { return };
        if boundary.partial {
            self.sentence_complete = false;
        }
        self.on_boundary(boundary, hwnd, rx, pending);
    }

    /// Keep an offered suggestion in step with what the user types after it.
    fn follow_live(&mut self, event: Event) {
        let Some(live) = self.live.as_mut() else { return };
        let stale = match event {
            Event::Char(c) => {
                live.typed_after.push(c);
                live.typed_after.chars().count() > MAX_LIVE_AFTER
            }
            Event::Backspace => live.typed_after.pop().is_none(),
            Event::Enter | Event::Reset => true,
        };
        if stale {
            self.drop_live();
        }
    }

    fn on_boundary(&mut self, b: Boundary, hwnd: isize, rx: &Receiver<Msg>, pending: &mut VecDeque<Msg>) {
        if !self.focus_unchanged() {
            // Start over; the next typed character re-runs the full safety check.
            self.hard_reset();
            return;
        }
        if let Some(engine) = ENGINE.get().cloned() {
            self.refresh_user_words();
            let fix = {
                let known = |w: &str| self.user_words.contains(w) || self.session_ignore.contains(w);
                evaluate(&engine, &b, &known)
            };
            if let Some(fix) = fix {
                let tail = format!("{}{}{}", b.leading, b.core, b.trailing);
                let fixed = format!("{}{}{}", b.leading, fix.replacement, b.trailing);
                if fix.auto {
                    // Keys typed while we were deciding are already in the app: include them.
                    pending.extend(rx.try_iter());
                    if self.apply(&b, &fix.replacement, hwnd, pending) {
                        FIXED.fetch_add(1, Ordering::SeqCst);
                        pop_chars(&mut self.sentence, b.chars_to_delete());
                        self.sentence.push_str(&fixed);
                        self.sentence.push(b.delimiter);
                    }
                } else {
                    SUGGESTED.fetch_add(1, Ordering::SeqCst);
                    self.offer(hwnd, "word", &fix.original, &fix.replacement, tail, fixed, b.delimiter);
                }
            }
        }
        if ends_sentence(&b.core, &b.trailing) {
            if self.sentence_complete && self.live.is_none() {
                self.suggest_grammar(hwnd, b.delimiter);
            }
            self.sentence.clear();
            self.sentence_complete = true;
        }
    }

    fn suggest_grammar(&mut self, hwnd: isize, delimiter: char) {
        let store = self.app.state::<Store>();
        if store.get_setting("autocorrect_grammar").ok().flatten().as_deref() == Some("false") {
            return;
        }
        let mut text = self.sentence.clone();
        if text.pop() != Some(delimiter) || text.split_whitespace().count() < MIN_GRAMMAR_WORDS {
            return;
        }
        let Some(checker) = GRAMMAR.get() else { return };
        let fixes = match checker.lock() {
            Ok(mut g) => g.check(&text),
            Err(_) => return,
        };
        let Some(fix) = fixes.into_iter().next() else { return };

        let chars: Vec<char> = text.chars().collect();
        // Deleting a word should take its leading space with it, not leave a double space.
        let start = if fix.replacement.is_empty() && fix.start > 0 && chars[fix.start - 1] == ' ' {
            fix.start - 1
        } else {
            fix.start
        };
        let tail: String = chars[start..].iter().collect();
        let mut replacement_tail: String = chars[start..fix.start].iter().collect();
        replacement_tail.push_str(&fix.replacement);
        replacement_tail.extend(&chars[fix.end..]);
        SUGGESTED.fetch_add(1, Ordering::SeqCst);
        self.offer(hwnd, "grammar", &fix.original, &fix.replacement, tail, replacement_tail, delimiter);
    }

    #[allow(clippy::too_many_arguments)]
    fn offer(
        &mut self,
        hwnd: isize,
        kind: &'static str,
        original: &str,
        replacement: &str,
        tail: String,
        replacement_tail: String,
        delimiter: char,
    ) {
        self.next_id += 1;
        let id = self.next_id;
        self.live = Some(Live {
            id,
            kind,
            original: original.to_string(),
            replacement: replacement.to_string(),
            tail,
            replacement_tail,
            delimiter,
            typed_after: String::new(),
            hwnd,
        });
        chip::show(
            &self.app,
            hwnd,
            serde_json::json!({ "id": id, "kind": kind, "original": original, "replacement": replacement }),
        );
    }

    fn accept(&mut self, id: u64) {
        if self.live.as_ref().map(|l| l.id) != Some(id) {
            return;
        }
        let Some(live) = self.live.take() else { return };
        chip::hide(&self.app);
        if foreground_hwnd() != live.hwnd || modifiers_held() || !self.focus_unchanged() {
            return;
        }
        let backspaces = live.tail.chars().count() + 1 + live.typed_after.chars().count();
        if backspaces > MAX_ACCEPT_BACKSPACES {
            return;
        }
        let Ok(_inject_guard) = crate::inject::INJECT_LOCK.try_lock() else { return };
        let typed = format!("{}{}{}", live.replacement_tail, live.delimiter, live.typed_after);
        if replace_text(backspaces, &typed) {
            FIXED.fetch_add(1, Ordering::SeqCst);
            // The text we were tracking has changed shape: start over from here.
            self.last_fix = None;
            self.sentence.clear();
            self.sentence_complete = false;
        }
    }

    fn dismiss(&mut self, id: u64, ignore: bool) {
        if self.live.as_ref().map(|l| l.id) != Some(id) {
            return;
        }
        let Some(live) = self.live.take() else { return };
        chip::hide(&self.app);
        if ignore && live.kind == "word" {
            let lower = live.original.to_lowercase();
            let _ = self.app.state::<Store>().add_autocorrect_ignore(&lower);
            self.session_ignore.insert(lower);
        }
    }

    fn apply(&mut self, b: &Boundary, replacement: &str, hwnd: isize, pending: &VecDeque<Msg>) -> bool {
        let mut after = String::new();
        for m in pending {
            match m {
                Msg::Key { event: Event::Char(c), hwnd: h } if *h == hwnd && !c.is_control() => after.push(*c),
                _ => return false,
            }
        }
        if after.chars().count() > MAX_RETYPE_AFTER || foreground_hwnd() != hwnd || modifiers_held() {
            return false;
        }
        let Ok(_inject_guard) = crate::inject::INJECT_LOCK.try_lock() else { return false };
        // Last check right before typing: focus can move while we were deciding.
        if !self.focus_unchanged() {
            return false;
        }

        let before = format!("{}{}{}", b.leading, b.core, b.trailing);
        let fixed = format!("{}{}{}", b.leading, replacement, b.trailing);
        let backspaces = b.chars_to_delete() + after.chars().count();
        let typed = format!("{fixed}{}{after}", b.delimiter);
        if !replace_text(backspaces, &typed) {
            return false;
        }
        // A Backspace right after the fix undoes it, but only if nothing was typed after it.
        if after.is_empty() {
            self.last_fix = Some(LastFix { before, after: fixed, original_lower: b.core.to_lowercase() });
        }
        true
    }

    /// The user pressed Backspace straight after a fix (the app already removed the space):
    /// put their original word back and stop correcting it this session.
    fn undo(&mut self, last: LastFix) {
        if foreground_hwnd() != self.hwnd || !self.focus_unchanged() {
            return;
        }
        let Ok(_inject_guard) = crate::inject::INJECT_LOCK.try_lock() else { return };
        if replace_text(last.after.chars().count(), &last.before) {
            self.tracker.restore_token(&last.before);
            // The screen now ends with the original token and no delimiter.
            pop_chars(&mut self.sentence, last.after.chars().count() + 1);
            self.sentence.push_str(&last.before);
            self.session_ignore.insert(last.original_lower);
        }
    }

    fn refresh_user_words(&mut self) {
        if self.user_words_at.is_some_and(|t| t.elapsed() < USER_WORDS_TTL) {
            return;
        }
        let store = self.app.state::<Store>();
        let mut words = HashSet::new();
        for entry in store.get_dictionary().unwrap_or_default() {
            words.extend(entry.word.split_whitespace().map(str::to_lowercase));
        }
        for w in store.get_autocorrect_ignore().unwrap_or_default() {
            words.insert(w.to_lowercase());
        }
        self.user_words = words;
        self.user_words_at = Some(Instant::now());
    }

    fn exe_for(&mut self, hwnd: isize) -> String {
        if self.exe_cache.0 != hwnd {
            let exe = crate::context::get_foreground_app().map(|a| a.exe).unwrap_or_default();
            self.exe_cache = (hwnd, exe);
        }
        self.exe_cache.1.clone()
    }

    /// Decide whether it is safe to edit the focused field, and say why not when it isn't.
    /// Also reports whether the field is empty, which makes the next word a sentence start.
    fn check_field(&mut self, hwnd: isize) -> Result<bool, &'static str> {
        let exe = self.exe_for(hwnd);
        if exe.is_empty() {
            return Err("couldn't identify the app");
        }
        if excluded_apps(&self.app.state::<Store>()).contains(&exe) {
            return Err("app is on the turned-off list");
        }
        if !target_reachable(hwnd) {
            return Err("app runs with higher privileges");
        }
        // Fail closed: a corrupted password is worse than a missed correction.
        let Some(automation) = &self.automation else { return Err("UI Automation unavailable") };
        let element = unsafe { automation.GetFocusedElement() }.map_err(|_| "couldn't inspect the focused field")?;
        match unsafe { element.CurrentIsPassword() } {
            Ok(b) if !b.as_bool() => {}
            Ok(_) => return Err("password field"),
            Err(_) => return Err("couldn't tell if this is a password field"),
        }
        let empty = field_is_empty(&element);
        self.field = Some(element);
        Ok(empty)
    }

    /// Apps can move focus without a key or click we see (auto-advance to a password box),
    /// so re-check before every fix: same approved field, still not a password field.
    fn focus_unchanged(&self) -> bool {
        let (Some(automation), Some(field)) = (&self.automation, &self.field) else { return false };
        let Ok(current) = (unsafe { automation.GetFocusedElement() }) else { return false };
        if !matches!(unsafe { current.CurrentIsPassword() }, Ok(b) if !b.as_bool()) {
            return false;
        }
        unsafe { automation.CompareElements(field, &current) }.is_ok_and(|same| same.as_bool())
    }

    fn safe_to_correct(&mut self, hwnd: isize) -> bool {
        let exe = self.exe_cache.1.clone();
        match self.check_field(hwnd) {
            Ok(empty) => {
                if empty {
                    // Nothing before the caret: the first word starts a sentence.
                    self.tracker.feed(Event::Enter);
                    self.sentence_complete = true;
                }
                set_note(&exe, if empty { "active (empty field)" } else { "active" });
                true
            }
            Err(reason) => {
                set_note(&exe, reason);
                false
            }
        }
    }
}
