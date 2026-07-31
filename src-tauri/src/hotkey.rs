use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::pipeline;
use crate::store::Store;

const DEFAULT_HOTKEY: &str = "ctrl+space";
const DEFAULT_MODE: &str = "hold"; // "hold" | "toggle"

static CURRENT_HOTKEY: Mutex<Option<String>> = Mutex::new(None);

pub fn register_hotkeys(app: &AppHandle) {
    let store = app.state::<Store>();
    let shortcut_str = store
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

    if let Err(e) = register_shortcut(app, &shortcut_str, &mode) {
        log::error!("Failed to register hotkey '{shortcut_str}': {e}");
        // Fall back to default
        if shortcut_str != DEFAULT_HOTKEY {
            let _ = register_shortcut(app, DEFAULT_HOTKEY, &mode);
        }
    }
}

fn register_shortcut(app: &AppHandle, shortcut_str: &str, mode: &str) -> Result<(), String> {
    let shortcut: Shortcut = shortcut_str
        .parse()
        .map_err(|e| format!("Invalid shortcut '{shortcut_str}': {e}"))?;

    // Unregister previous if any
    if let Some(prev) = CURRENT_HOTKEY.lock().unwrap().take() {
        if let Ok(prev_sc) = prev.parse::<Shortcut>() {
            let _ = app.global_shortcut().unregister(prev_sc);
        }
    }

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

    *CURRENT_HOTKEY.lock().unwrap() = Some(shortcut_str.to_string());
    log::info!("Hotkey registered: {shortcut_str} (mode={mode})");
    Ok(())
}

pub fn set_hotkey(app: &AppHandle, shortcut_str: &str) -> Result<(), String> {
    let cleaned = shortcut_str.trim().to_lowercase();
    if cleaned.is_empty() {
        return Err("Hotkey cannot be empty".into());
    }
    // Validate parse first
    let _: Shortcut = cleaned
        .parse()
        .map_err(|e| format!("Invalid shortcut: {e}"))?;

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
