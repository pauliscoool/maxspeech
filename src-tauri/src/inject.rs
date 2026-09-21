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
/// Bumped on every inject attempt so deferred clipboard restores from an older
/// paste cannot clobber a newer session's clipboard mid-flight.
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
    let old_clipboard = get_clipboard_text();
    let gen = CLIPBOARD_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    set_clipboard_text(text)?;
    thread::sleep(Duration::from_millis(50));

    if !still_wanted() {
        // Don't leave the cancelled session's text sitting on the clipboard.
        if let Some(old) = old_clipboard {
            let _ = set_clipboard_text(&old);
        }
        return Err("inject cancelled before Ctrl+V (stale session)".into());
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

    let old = old_clipboard;
    thread::spawn(move || {
        // Slow apps (Electron, browsers) often read clipboard asynchronously.
        thread::sleep(Duration::from_millis(900));
        // Skip restore if a newer inject owns the clipboard now.
        if CLIPBOARD_GEN.load(Ordering::SeqCst) != gen {
            return;
        }
        if let Some(old_text) = old {
            let _ = set_clipboard_text(&old_text);
        }
    });
    Ok(())
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
