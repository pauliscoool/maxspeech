use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::pipeline;

pub fn register_hotkeys(app: &tauri::AppHandle) {
    let shortcut: Shortcut = "ctrl+space".parse().expect("Invalid shortcut");
    let handle = app.clone();

    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            match event.state {
                ShortcutState::Pressed => {
                    pipeline::start_dictation(&handle);
                }
                ShortcutState::Released => {
                    pipeline::stop_dictation(&handle);
                }
            }
        })
        .expect("Failed to register global shortcut");

    log::info!("Hotkey registered: Ctrl+Space (push-to-talk)");
}
