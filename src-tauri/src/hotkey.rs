use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::pipeline;
use crate::store::Store;

#[cfg(windows)]
const DEFAULT_HOTKEY: &str = "ctrl+super";
#[cfg(not(windows))]
const DEFAULT_HOTKEY: &str = "ctrl+shift+space";
const DEFAULT_MODE: &str = "hold"; // "hold" | "toggle"

static CURRENT_HOTKEY: Mutex<Option<String>> = Mutex::new(None);
static USING_MODIFIER_HOOK: Mutex<bool> = Mutex::new(false);

pub fn register_hotkeys(app: &AppHandle) {
    let store = app.state::<Store>();
    let mut shortcut_str = store
        .get_setting("hotkey")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_HOTKEY.to_string());
    let mode = store
        .get_setting("hotkey_mode")
        .ok()
        .flatten()
        .unwrap_or_else(|| DEFAULT_MODE.to_string());

    // Migrate blocked / unsupported modifier-only combos so boot always arms a
    // working shortcut instead of a silent no-op (Ctrl+Win on Windows only;
    // ctrl+shift+space elsewhere).
    if let Ok(normalized) = normalize_hotkey(&shortcut_str) {
        #[cfg(windows)]
        let unsupported_mod_only =
            is_modifier_only(&normalized) && !is_supported_modifier_only(&normalized);
        #[cfg(not(windows))]
        let unsupported_mod_only = is_modifier_only(&normalized);
        if is_blocked_combo(&normalized) || unsupported_mod_only {
            log::warn!("Migrating unsupported hotkey '{normalized}' → '{DEFAULT_HOTKEY}'");
            shortcut_str = DEFAULT_HOTKEY.to_string();
            let _ = store.set_setting("hotkey", DEFAULT_HOTKEY);
        }
    }

    if let Err(e) = register_shortcut(app, &shortcut_str, &mode) {
        log::error!("Failed to register hotkey '{shortcut_str}': {e}");
        // Fall back to default
        if shortcut_str != DEFAULT_HOTKEY {
            let _ = register_shortcut(app, DEFAULT_HOTKEY, &mode);
        }
    }
}

fn register_shortcut(app: &AppHandle, shortcut_str: &str, mode: &str) -> Result<(), String> {
    let cleaned = normalize_hotkey(shortcut_str)?;

    // Clear previous registration (plugin shortcut and/or modifier hook).
    unregister_current(app);

    if is_modifier_only(&cleaned) {
        if !is_supported_modifier_only(&cleaned) {
            return Err(format!("Unsupported modifier-only hotkey '{cleaned}'"));
        }
        #[cfg(windows)]
        {
            win_mod_hook::install(app, &cleaned, mode)?;
            *USING_MODIFIER_HOOK.lock().unwrap() = true;
            *CURRENT_HOTKEY.lock().unwrap() = Some(cleaned.clone());
            log::info!("Hotkey registered via LL hook: {cleaned} (mode={mode})");
            return Ok(());
        }
        #[cfg(not(windows))]
        {
            return Err(format!(
                "Modifier-only hotkey '{cleaned}' is only supported on Windows"
            ));
        }
    }

    let shortcut: Shortcut = cleaned
        .parse()
        .map_err(|e| format!("Invalid shortcut '{cleaned}': {e}"))?;

    let handle = app.clone();
    let mode_owned = mode.to_string();

    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            let mode_now = handle
                .try_state::<Store>()
                .and_then(|s| s.get_setting("hotkey_mode").ok().flatten())
                .unwrap_or_else(|| mode_owned.clone());

            match event.state {
                ShortcutState::Pressed => {
                    if mode_now == "toggle" {
                        let active = *handle.state::<pipeline::PipelineState>().active.lock().unwrap();
                        if active {
                            pipeline::stop_dictation(&handle);
                        } else {
                            pipeline::start_dictation(&handle);
                        }
                    } else {
                        pipeline::start_dictation(&handle);
                    }
                }
                ShortcutState::Released => {
                    if mode_now != "toggle" {
                        pipeline::stop_dictation(&handle);
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;

    *USING_MODIFIER_HOOK.lock().unwrap() = false;
    *CURRENT_HOTKEY.lock().unwrap() = Some(cleaned.clone());
    log::info!("Hotkey registered: {cleaned} (mode={mode})");
    Ok(())
}

fn unregister_current(app: &AppHandle) {
    if let Some(prev) = CURRENT_HOTKEY.lock().unwrap().take() {
        if let Ok(prev_sc) = prev.parse::<Shortcut>() {
            let _ = app.global_shortcut().unregister(prev_sc);
        }
    }
    if *USING_MODIFIER_HOOK.lock().unwrap() {
        #[cfg(windows)]
        win_mod_hook::uninstall();
        *USING_MODIFIER_HOOK.lock().unwrap() = false;
    }
}

pub fn set_hotkey(app: &AppHandle, shortcut_str: &str) -> Result<(), String> {
    let cleaned = normalize_hotkey(shortcut_str)?;
    if cleaned.is_empty() {
        return Err("Hotkey cannot be empty".into());
    }
    if is_blocked_combo(&cleaned) {
        return Err("That hotkey combo is not supported. Try Ctrl+Win or Ctrl+Shift+Z.".into());
    }
    // Validate: modifier-only (Windows hook) or standard Shortcut parse
    if is_modifier_only(&cleaned) {
        #[cfg(not(windows))]
        {
            return Err(format!(
                "modifier-only hotkey '{cleaned}' is only supported on Windows"
            ));
        }
        if !is_supported_modifier_only(&cleaned) {
            return Err(format!("Unsupported modifier-only hotkey '{cleaned}'"));
        }
    } else {
        let _: Shortcut = cleaned
            .parse()
            .map_err(|e| format!("Invalid shortcut: {e}"))?;
    }

    let store = app.state::<Store>();
    store
        .set_setting("hotkey", &cleaned)
        .map_err(|e| e.to_string())?;

    let mode = store
        .get_setting("hotkey_mode")
        .ok()
        .flatten()
        .unwrap_or_else(|| DEFAULT_MODE.to_string());

    register_shortcut(app, &cleaned, &mode)
}

pub fn set_hotkey_mode(app: &AppHandle, mode: &str) -> Result<(), String> {
    let mode = match mode {
        "toggle" => "toggle",
        _ => "hold",
    };
    let store = app.state::<Store>();
    store
        .set_setting("hotkey_mode", mode)
        .map_err(|e| e.to_string())?;

    let shortcut = store
        .get_setting("hotkey")
        .ok()
        .flatten()
        .unwrap_or_else(|| DEFAULT_HOTKEY.to_string());

    register_shortcut(app, &shortcut, mode)
}

pub fn get_hotkey(app: &AppHandle) -> String {
    app.state::<Store>()
        .get_setting("hotkey")
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_HOTKEY.to_string())
}

pub fn get_hotkey_mode(app: &AppHandle) -> String {
    app.state::<Store>()
        .get_setting("hotkey_mode")
        .ok()
        .flatten()
        .unwrap_or_else(|| DEFAULT_MODE.to_string())
}

fn normalize_hotkey(raw: &str) -> Result<String, String> {
    let mut mods = Vec::new();
    let mut key: Option<String> = None;

    for raw_tok in raw.split('+') {
        let t = raw_tok.trim().to_lowercase();
        if t.is_empty() {
            continue;
        }
        match t.as_str() {
            "control" | "ctrl" => {
                if !mods.iter().any(|m: &String| m == "ctrl") {
                    mods.push("ctrl".into());
                }
            }
            "alt" | "option" => {
                if !mods.iter().any(|m: &String| m == "alt") {
                    mods.push("alt".into());
                }
            }
            "shift" => {
                if !mods.iter().any(|m: &String| m == "shift") {
                    mods.push("shift".into());
                }
            }
            "super" | "meta" | "cmd" | "command" | "win" | "windows" => {
                if !mods.iter().any(|m: &String| m == "super") {
                    mods.push("super".into());
                }
            }
            other => {
                if key.is_some() {
                    return Err(format!("Invalid hotkey '{raw}': multiple keys"));
                }
                key = Some(other.to_string());
            }
        }
    }

    // Stable modifier order for comparisons / storage
    let order = ["ctrl", "alt", "shift", "super"];
    mods.sort_by_key(|m| order.iter().position(|o| o == m).unwrap_or(99));

    let mut parts = mods;
    if let Some(k) = key {
        parts.push(k);
    }
    if parts.is_empty() {
        return Err("Hotkey cannot be empty".into());
    }
    Ok(parts.join("+"))
}

fn is_modifier_only(s: &str) -> bool {
    !s.split('+').any(|t| {
        let t = t.trim().to_lowercase();
        !matches!(
            t.as_str(),
            "ctrl"
                | "control"
                | "alt"
                | "option"
                | "shift"
                | "super"
                | "meta"
                | "cmd"
                | "command"
                | "win"
                | "windows"
        )
    }) && s.contains('+')
}

fn is_supported_modifier_only(s: &str) -> bool {
    // Ctrl+Win is the only modifier-only combo we hook.
    s == "ctrl+super"
}

fn is_blocked_combo(s: &str) -> bool {
    // Ctrl+Alt alone is removed as an option (conflicts / unused).
    s == "ctrl+alt" || s == "alt+ctrl"
}

/// Windows low-level keyboard hook for modifier-only combos (Ctrl+Win).
/// `RegisterHotKey` / tauri global-shortcut require a non-modifier main key, so
/// Ctrl+Win cannot be registered that way.
#[cfg(windows)]
mod win_mod_hook {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Mutex;
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use tauri::{AppHandle, Manager};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
        KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LWIN, VK_RCONTROL, VK_RWIN,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL,
        WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    use crate::pipeline;
    use crate::store::Store;

    /// Dummy VK used to cancel the pending Start-menu action when Win was pressed
    /// before Ctrl (we must not eat Win-up unless we ate Win-down).
    const VK_DISARM: u16 = 0xE8;

    struct HookThread {
        thread_id: u32,
        join: Option<JoinHandle<()>>,
    }

    static HOOK_THREAD: Mutex<Option<HookThread>> = Mutex::new(None);
    static APP: Mutex<Option<AppHandle>> = Mutex::new(None);
    static MODE: Mutex<String> = Mutex::new(String::new());
    static COMBO_ACTIVE: AtomicBool = AtomicBool::new(false);
    /// True only if we swallowed the matching Win KEYDOWN — then we must eat KEYUP.
    static ATE_WIN_DOWN: AtomicBool = AtomicBool::new(false);
    static CTRL_DOWN: AtomicBool = AtomicBool::new(false);
    static WIN_DOWN: AtomicBool = AtomicBool::new(false);
    /// Bumped on every edge so stale debounce workers bail out.
    static EDGE_GEN: AtomicU64 = AtomicU64::new(0);

    pub fn install(app: &AppHandle, _combo: &str, mode: &str) -> Result<(), String> {
        uninstall();
        *APP.lock().unwrap() = Some(app.clone());
        *MODE.lock().unwrap() = mode.to_string();
        COMBO_ACTIVE.store(false, Ordering::SeqCst);
        ATE_WIN_DOWN.store(false, Ordering::SeqCst);
        EDGE_GEN.fetch_add(1, Ordering::SeqCst);
        CTRL_DOWN.store(ctrl_physically_down(), Ordering::SeqCst);
        WIN_DOWN.store(win_physically_down(), Ordering::SeqCst);

        let (tx, rx) = std::sync::mpsc::channel::<Result<u32, String>>();
        let join = thread::spawn(move || {
            let hook = match unsafe {
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_proc), None, 0)
            } {
                Ok(h) => h,
                Err(e) => {
                    let _ = tx.send(Err(format!("SetWindowsHookExW failed: {e}")));
                    return;
                }
            };

            let thread_id = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
            if tx.send(Ok(thread_id)).is_err() {
                unsafe {
                    let _ = UnhookWindowsHookEx(hook);
                }
                return;
            }

            let mut msg = MSG::default();
            unsafe {
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let _ = UnhookWindowsHookEx(hook);
            }
        });

        let thread_id = rx
            .recv()
            .map_err(|e| format!("hook thread channel: {e}"))??;

        *HOOK_THREAD.lock().unwrap() = Some(HookThread {
            thread_id,
            join: Some(join),
        });
        Ok(())
    }

    pub fn uninstall() {
        EDGE_GEN.fetch_add(1, Ordering::SeqCst);
        COMBO_ACTIVE.store(false, Ordering::SeqCst);
        // If we ate Win-down, synthesize Win-up so the OS isn't left with Win stuck.
        if ATE_WIN_DOWN.swap(false, Ordering::SeqCst) {
            synthesize_win_up();
        }
        CTRL_DOWN.store(false, Ordering::SeqCst);
        WIN_DOWN.store(false, Ordering::SeqCst);
        if let Some(mut ht) = HOOK_THREAD.lock().unwrap().take() {
            unsafe {
                let _ = PostThreadMessageW(ht.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            if let Some(join) = ht.join.take() {
                let _ = join.join();
            }
        }
        *APP.lock().unwrap() = None;
    }

    fn ctrl_physically_down() -> bool {
        unsafe { GetAsyncKeyState(VK_CONTROL.0 as i32) < 0 }
    }

    fn win_physically_down() -> bool {
        unsafe {
            GetAsyncKeyState(VK_LWIN.0 as i32) < 0 || GetAsyncKeyState(VK_RWIN.0 as i32) < 0
        }
    }

    fn is_ctrl_vk(vk: u32) -> bool {
        vk == VK_CONTROL.0 as u32
            || vk == VK_LCONTROL.0 as u32
            || vk == VK_RCONTROL.0 as u32
    }

    fn is_win_vk(vk: u32) -> bool {
        vk == VK_LWIN.0 as u32 || vk == VK_RWIN.0 as u32
    }

    /// Cancel the "Win is held → open Start on release" latch without eating Win-up.
    fn disarm_win_start_menu() {
        unsafe {
            let mut inputs = [
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(VK_DISARM),
                            wScan: 0,
                            dwFlags: Default::default(),
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(VK_DISARM),
                            wScan: 0,
                            dwFlags: KEYEVENTF_KEYUP,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                },
            ];
            let _ = SendInput(&mut inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }

    fn synthesize_win_up() {
        unsafe {
            let mut inputs = [
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_LWIN,
                            wScan: 0,
                            dwFlags: KEYEVENTF_KEYUP,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_RWIN,
                            wScan: 0,
                            dwFlags: KEYEVENTF_KEYUP,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                },
            ];
            let _ = SendInput(&mut inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }

    fn fire_pressed() {
        let gen = EDGE_GEN.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        let Some(app) = APP.lock().unwrap().clone() else {
            return;
        };
        let mode = MODE.lock().unwrap().clone();
        thread::spawn(move || {
            // Debounce brief Ctrl/Win flicker so we don't start then instantly stop.
            thread::sleep(Duration::from_millis(40));
            if EDGE_GEN.load(Ordering::SeqCst) != gen {
                return;
            }
            if !(CTRL_DOWN.load(Ordering::SeqCst) && WIN_DOWN.load(Ordering::SeqCst)) {
                return;
            }
            // Heal sticky atomics against the real keyboard.
            if !ctrl_physically_down() || !win_physically_down() {
                CTRL_DOWN.store(ctrl_physically_down(), Ordering::SeqCst);
                WIN_DOWN.store(win_physically_down(), Ordering::SeqCst);
                COMBO_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }

            let mode_now = app
                .try_state::<Store>()
                .and_then(|s| s.get_setting("hotkey_mode").ok().flatten())
                .unwrap_or(mode);

            if mode_now == "toggle" {
                let active = *app.state::<pipeline::PipelineState>().active.lock().unwrap();
                if active {
                    pipeline::stop_dictation(&app);
                } else {
                    pipeline::start_dictation(&app);
                }
            } else {
                pipeline::start_dictation(&app);
            }
        });
    }

    fn fire_released() {
        let gen = EDGE_GEN.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        let Some(app) = APP.lock().unwrap().clone() else {
            return;
        };
        let mode = MODE.lock().unwrap().clone();
        thread::spawn(move || {
            // Debounce: if the combo comes back quickly, don't stop.
            thread::sleep(Duration::from_millis(70));
            if EDGE_GEN.load(Ordering::SeqCst) != gen {
                return;
            }
            if CTRL_DOWN.load(Ordering::SeqCst) && WIN_DOWN.load(Ordering::SeqCst) {
                return;
            }

            let mode_now = app
                .try_state::<Store>()
                .and_then(|s| s.get_setting("hotkey_mode").ok().flatten())
                .unwrap_or(mode);
            if mode_now != "toggle" {
                pipeline::stop_dictation(&app);
            }
        });
    }

    fn update_combo_state() {
        let want = CTRL_DOWN.load(Ordering::SeqCst) && WIN_DOWN.load(Ordering::SeqCst);
        let was = COMBO_ACTIVE.swap(want, Ordering::SeqCst);
        if want && !was {
            fire_pressed();
        } else if !want && was {
            fire_released();
        }
    }

    /// Soft-heal: if we think a modifier is down but the OS says it isn't, clear it.
    fn reconcile_modifiers() {
        let ctrl_os = ctrl_physically_down();
        let win_os = win_physically_down();
        let mut changed = false;
        if CTRL_DOWN.load(Ordering::SeqCst) && !ctrl_os {
            CTRL_DOWN.store(false, Ordering::SeqCst);
            changed = true;
        }
        if WIN_DOWN.load(Ordering::SeqCst) && !win_os {
            WIN_DOWN.store(false, Ordering::SeqCst);
            // OS already saw the up — don't leave ATE_WIN_DOWN latch armed.
            ATE_WIN_DOWN.store(false, Ordering::SeqCst);
            changed = true;
        }
        if changed {
            update_combo_state();
        }
    }

    unsafe extern "system" fn low_level_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code < 0 {
            return unsafe { CallNextHookEx(None, code, wparam, lparam) };
        }

        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let vk = info.vkCode;
        let msg = wparam.0 as u32;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        // Ignore injected input (clipboard Ctrl+V, our disarm key, synthetic Win-up).
        if info.flags.contains(LLKHF_INJECTED) {
            return unsafe { CallNextHookEx(None, code, wparam, lparam) };
        }

        let mut eat = false;

        if is_ctrl_vk(vk) {
            if is_down {
                CTRL_DOWN.store(true, Ordering::SeqCst);
                // Win already held (Win-then-Ctrl): do NOT eat the upcoming Win-up
                // (that stuck the Win key in the OS). Disarm Start menu instead.
                if WIN_DOWN.load(Ordering::SeqCst) && !ATE_WIN_DOWN.load(Ordering::SeqCst) {
                    disarm_win_start_menu();
                }
            } else if is_up {
                CTRL_DOWN.store(false, Ordering::SeqCst);
            }
            update_combo_state();
            reconcile_modifiers();
        } else if is_win_vk(vk) {
            if is_down {
                WIN_DOWN.store(true, Ordering::SeqCst);
                // Swallow Win only when Ctrl is already held (Ctrl-then-Win).
                // Matching KEYUP must also be eaten — never eat UP without DOWN.
                if CTRL_DOWN.load(Ordering::SeqCst) {
                    ATE_WIN_DOWN.store(true, Ordering::SeqCst);
                    eat = true;
                }
                update_combo_state();
            } else if is_up {
                WIN_DOWN.store(false, Ordering::SeqCst);
                if ATE_WIN_DOWN.swap(false, Ordering::SeqCst) {
                    eat = true;
                }
                update_combo_state();
            }
            reconcile_modifiers();
        } else if is_down {
            // Any other key while we think modifiers are stuck — resync from OS.
            reconcile_modifiers();
        }

        if eat {
            LRESULT(1)
        } else {
            unsafe { CallNextHookEx(None, code, wparam, lparam) }
        }
    }
}
