use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};

/// Serialize all keyboard/clipboard injection so two sessions never interleave.
static INJECT_LOCK: Mutex<()> = Mutex::new(());
/// Bumped on every inject so a cancelled attempt cannot race a newer paste.
static CLIPBOARD_GEN: AtomicU64 = AtomicU64::new(0);

pub struct LastInsertion {
    pub text: String,
    pub char_count: usize,
    /// When this paste landed — used for the 5s re-dictate learn window.
    pub pasted_at: Instant,
}

pub fn inject_text(text: &str) -> Result<LastInsertion, Box<dyn std::error::Error>> {
    inject_text_if(text, || true)
}

/// Paste only while `still_wanted` stays true. Re-checked after the modifier
/// wait and again immediately before Ctrl+V so a newer dictation that started
/// mid-wait cannot get the previous session's text dumped into the new field.
pub fn inject_text_if<F>(
    text: &str,
    still_wanted: F,
) -> Result<LastInsertion, Box<dyn std::error::Error>>
where
    F: Fn() -> bool,
{
    let _guard = INJECT_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let start = Instant::now();

    if !still_wanted() {
        return Err("inject cancelled (stale session)".into());
    }

    // Wait for the push-to-talk modifiers to fully release so paste / focus
    // isn't corrupted (and so our LL hotkey hook doesn't see injected Ctrl
    // while Win is still logically "down").
    wait_for_modifiers_up(Duration::from_millis(450));
    thread::sleep(Duration::from_millis(40));

    if !still_wanted() {
        return Err("inject cancelled after modifier wait (stale session)".into());
    }

    // Prefer a single atomic clipboard paste. Unicode typing can deliver only
    // part of a long string then error — a clipboard fallback after that typed
    // the whole thing again ("half paragraph, then the rest").
    match inject_via_clipboard(text, &still_wanted) {
        Ok(()) => {
            log::info!("Text injected via clipboard paste in {:?}", start.elapsed());
            return Ok(LastInsertion {
                text: text.to_string(),
                char_count: text.chars().count(),
                pasted_at: Instant::now(),
            });
        }
        Err(e) if e.to_string().contains("cancelled") => {
            return Err(e);
        }
        Err(e) => {
            log::warn!("Clipboard paste failed ({e}), trying Unicode inject");
        }
    }

    if !still_wanted() {
        return Err("inject cancelled before unicode (stale session)".into());
    }

    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("{e}"))?;
    enigo.text(text).map_err(|e| format!("Unicode inject failed: {e}"))?;
    log::info!("Text injected via Unicode in {:?}", start.elapsed());

    Ok(LastInsertion {
        text: text.to_string(),
        char_count: text.chars().count(),
        pasted_at: Instant::now(),
    })
}

fn paste_modifier() -> Key {
    #[cfg(target_os = "macos")]
    {
        Key::Meta
    }
    #[cfg(not(target_os = "macos"))]
    {
        Key::Control
    }
}

fn inject_via_clipboard<F>(
    text: &str,
    still_wanted: &F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn() -> bool,
{
    let _gen = CLIPBOARD_GEN.fetch_add(1, Ordering::SeqCst) + 1;

    // Set + verify before Ctrl+V. Windows clipboard can lag or briefly fail to
    // open — without a read-back, Ctrl+V pastes whatever was there before
    // (often a copied URL) instead of the dictation.
    ensure_clipboard_text(text)?;

    if !still_wanted() {
        return Err("inject cancelled before Ctrl+V (stale session)".into());
    }

    // Final read-back immediately before the keystroke.
    let on_clip = get_clipboard_text().unwrap_or_default();
    if on_clip != text {
        return Err(format!(
            "clipboard drifted before paste (got {} chars, want {})",
            on_clip.len(),
            text.len()
        )
        .into());
    }

    let mod_key = paste_modifier();
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("{e}"))?;
    enigo
        .key(mod_key, Direction::Press)
        .map_err(|e| format!("{e}"))?;
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| format!("{e}"))?;
    // Always release even if Click failed mid-way.
    let _ = enigo.key(mod_key, Direction::Release);

    // Do NOT restore the previous clipboard. Slow apps (Electron, browsers,
    // chat clients) often read the clipboard asynchronously after Ctrl+V —
    // restoring a prior URL/link ~1s later made those apps paste the old link
    // instead of (or after) the dictation. Leaving the dictated text on the
    // clipboard is intentional and safe.
    Ok(())
}

/// Write `text` to the clipboard and retry until a read-back matches.
fn ensure_clipboard_text(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_millis(400);
    let mut last_err: Option<String> = None;
    while Instant::now() < deadline {
        match set_clipboard_text(text) {
            Ok(()) => {
                thread::sleep(Duration::from_millis(25));
                if get_clipboard_text().as_deref() == Some(text) {
                    return Ok(());
                }
                last_err = Some("clipboard read-back mismatch".into());
            }
            Err(e) => {
                last_err = Some(e.to_string());
                thread::sleep(Duration::from_millis(30));
            }
        }
    }
    Err(format!(
        "clipboard set failed: {}",
        last_err.unwrap_or_else(|| "unknown".into())
    )
    .into())
}

fn wait_for_modifiers_up(timeout: Duration) {
    #[cfg(windows)]
    {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if !any_modifier_down() {
                // Require a brief clean window so we don't race a flicker.
                thread::sleep(Duration::from_millis(25));
                if !any_modifier_down() {
                    return;
                }
            }
            thread::sleep(Duration::from_millis(15));
        }
        log::warn!("Modifiers still down after {:?}; injecting anyway", timeout);
    }
    #[cfg(not(windows))]
    {
        // No reliable cross-desktop modifier poll; brief delay after hotkey release.
        let _ = timeout;
        thread::sleep(Duration::from_millis(80));
    }
}

#[cfg(windows)]
fn any_modifier_down() -> bool {
    unsafe {
        GetAsyncKeyState(VK_CONTROL.0 as i32) < 0
            || GetAsyncKeyState(VK_SHIFT.0 as i32) < 0
            || GetAsyncKeyState(VK_MENU.0 as i32) < 0
            || GetAsyncKeyState(VK_LWIN.0 as i32) < 0
            || GetAsyncKeyState(VK_RWIN.0 as i32) < 0
    }
}

fn press_ctrl_combo(letter: char) -> Result<(), Box<dyn std::error::Error>> {
    let mod_key = paste_modifier();
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("{e}"))?;
    enigo
        .key(mod_key, Direction::Press)
        .map_err(|e| format!("{e}"))?;
    let click = enigo.key(Key::Unicode(letter), Direction::Click);
    let _ = enigo.key(mod_key, Direction::Release);
    click.map_err(|e| format!("{e}"))?;
    Ok(())
}

/// Poll until the clipboard differs from `marker` (the copy landed).
fn wait_for_copy(marker: &str, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        thread::sleep(Duration::from_millis(30));
        match get_clipboard_text() {
            Some(t) if t != marker && !t.is_empty() => return Some(t),
            _ => {}
        }
    }
    None
}

/// Copy the focused app's selection; if nothing is selected, select all first.
/// A sentinel on the clipboard tells "copy landed" apart from "nothing selected".
pub fn capture_selection() -> Result<String, Box<dyn std::error::Error>> {
    let _guard = INJECT_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    // The hotkey's own Ctrl+Shift must be up or the copy becomes Ctrl+Shift+C.
    wait_for_modifiers_up(Duration::from_millis(600));
    thread::sleep(Duration::from_millis(40));

    let previous = get_clipboard_text();
    let marker = format!(
        "\u{200B}maxspeech-sel-{}",
        CLIPBOARD_GEN.fetch_add(1, Ordering::SeqCst)
    );
    ensure_clipboard_text(&marker)?;

    press_ctrl_combo('c')?;
    let mut copied = wait_for_copy(&marker, Duration::from_millis(350));
    if copied.is_none() {
        press_ctrl_combo('a')?;
        thread::sleep(Duration::from_millis(60));
        press_ctrl_combo('c')?;
        copied = wait_for_copy(&marker, Duration::from_millis(500));
    }

    match &previous {
        Some(prev) => {
            let _ = set_clipboard_text(prev);
        }
        None => {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                let _ = cb.clear();
            }
        }
    }

    match copied {
        Some(t) if !t.trim().is_empty() => Ok(t),
        _ => Err("No text to enhance".into()),
    }
}

/// Bring `hwnd` back to the foreground before pasting over its selection.
#[cfg(windows)]
pub fn focus_window(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};
    if hwnd == 0 {
        return;
    }
    let target = HWND(hwnd as *mut _);
    unsafe {
        if GetForegroundWindow() != target {
            let _ = SetForegroundWindow(target);
            thread::sleep(Duration::from_millis(80));
        }
    }
}

#[cfg(not(windows))]
pub fn focus_window(_hwnd: isize) {}

pub fn undo_insertion(insertion: &LastInsertion) -> Result<(), Box<dyn std::error::Error>> {
    let _guard = INJECT_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    wait_for_modifiers_up(Duration::from_millis(300));
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("{e}"))?;
    for _ in 0..insertion.char_count {
        enigo
            .key(Key::Backspace, Direction::Click)
            .map_err(|e| format!("{e}"))?;
        thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

fn get_clipboard_text() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok())
}

pub fn copy_text(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    set_clipboard_text(text)
}

fn set_clipboard_text(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("clipboard open: {e}"))?;
    cb.set_text(text.to_string())
        .map_err(|e| format!("clipboard set: {e}"))?;
    Ok(())
}
