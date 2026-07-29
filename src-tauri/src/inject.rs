use enigo::{Enigo, Keyboard, Settings, Key, Direction};
use std::thread;
use std::time::Duration;

#[cfg(windows)]
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
#[cfg(windows)]
use windows::Win32::Foundation::{HANDLE, HGLOBAL};
#[cfg(windows)]
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

pub struct LastInsertion {
    pub text: String,
    pub char_count: usize,
}

pub fn inject_text(text: &str) -> Result<LastInsertion, Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    let old_clipboard = get_clipboard_text();
    set_clipboard_text(text)?;

    thread::sleep(Duration::from_millis(30));
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Control, Direction::Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Unicode('v'), Direction::Click).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Control, Direction::Release).map_err(|e| format!("{e}"))?;

    let old = old_clipboard;
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(150));
        if let Some(old_text) = old {
            let _ = set_clipboard_text(&old_text);
        }
    });

    log::info!("Text injected in {:?}", start.elapsed());

    Ok(LastInsertion {
        text: text.to_string(),
        char_count: text.chars().count(),
    })
}

pub fn undo_insertion(insertion: &LastInsertion) -> Result<(), Box<dyn std::error::Error>> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("{e}"))?;
    for _ in 0..insertion.char_count {
        enigo.key(Key::Backspace, Direction::Click).map_err(|e| format!("{e}"))?;
        thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

#[cfg(windows)]
fn get_clipboard_text() -> Option<String> {
    unsafe {
        if OpenClipboard(None).is_ok() {
            let result = if let Ok(h) = GetClipboardData(13) {
                let hmem = HGLOBAL(h.0 as *mut _);
                let ptr = GlobalLock(hmem) as *const u16;
                if !ptr.is_null() {
                    let mut len = 0;
                    while *ptr.add(len) != 0 {
                        len += 1;
                    }
                    let slice = std::slice::from_raw_parts(ptr, len);
                    let s = String::from_utf16_lossy(slice);
                    let _ = GlobalUnlock(hmem);
                    Some(s)
                } else {
                    None
                }
            } else {
                None
            };
            let _ = CloseClipboard();
            result
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
fn get_clipboard_text() -> Option<String> {
    None
}

#[cfg(windows)]
fn set_clipboard_text(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let byte_len = wide.len() * 2;
    unsafe {
        OpenClipboard(None)?;
        EmptyClipboard()?;
        let hmem = GlobalAlloc(GMEM_MOVEABLE, byte_len)?;
        let ptr = GlobalLock(hmem) as *mut u16;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        let _ = GlobalUnlock(hmem);
        SetClipboardData(13, Some(HANDLE(hmem.0 as *mut _)))?;
        CloseClipboard()?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_clipboard_text(_text: &str) -> Result<(), Box<dyn std::error::Error>> {
    Err("Clipboard not supported on this platform".into())
}
