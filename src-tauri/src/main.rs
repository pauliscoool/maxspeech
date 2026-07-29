#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod context;
mod hotkey;
mod inject;
mod pipeline;
mod secrets;
mod store;
mod stt;

use store::Store;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, WebviewUrl, WebviewWindowBuilder,
};

#[tauri::command]
async fn save_secret(key: String, value: String) -> Result<(), String> {
    secrets::set_secret(&key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
async fn test_microphone(app: tauri::AppHandle) -> Result<(), String> {
    audio::test_microphone(&app).map_err(|e| e.to_string())
}

#[tauri::command]
async fn complete_onboarding(app: tauri::AppHandle) -> Result<(), String> {
    let store = app.state::<Store>();
    store.set_onboarded(true).map_err(|e| e.to_string())?;
    if let Some(w) = app.get_webview_window("onboarding") {
        let _ = w.close();
    }
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.show();
    }
    Ok(())
}

#[tauri::command]
async fn get_history(
    app: tauri::AppHandle,
    search: String,
) -> Result<Vec<store::HistoryEntry>, String> {
    let store = app.state::<Store>();
    store.get_history(&search).map_err(|e| e.to_string())
}

#[tauri::command]
async fn transcribe_file(path: String) -> Result<stt::batch::TranscriptionResult, String> {
    stt::batch::transcribe(&path).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn export_transcription(format: String, text: String) -> Result<(), String> {
    stt::batch::export(&format, &text).map_err(|e| e.to_string())
}

fn main() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("settings") {
                let _ = w.set_focus();
            }
        }))
        .manage(Store::new().expect("Failed to initialize database"))
        .manage(pipeline::PipelineState::default())
        .invoke_handler(tauri::generate_handler![
            save_secret,
            test_microphone,
            complete_onboarding,
            get_history,
            transcribe_file,
            export_transcription,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Build tray menu
            let show_item = MenuItem::with_id(app, "show", "Settings", true, None::<&str>)?;
            let history_item = MenuItem::with_id(app, "history", "History", true, None::<&str>)?;
            let transcriber_item =
                MenuItem::with_id(app, "transcriber", "Transcriber", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[&show_item, &history_item, &transcriber_item, &quit_item],
            )?;

            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("MaxSpeech - Voice to Text")
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => {
                        open_window(app, "settings", "MaxSpeech - Settings", 700, 500);
                    }
                    "history" => {
                        open_window(app, "history", "Dictation History", 600, 500);
                    }
                    "transcriber" => {
                        open_window(app, "transcriber", "Audio Transcriber", 600, 500);
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // Check if onboarding is needed
            let store = handle.state::<Store>();
            if !store.is_onboarded() {
                open_window(&handle, "onboarding", "Welcome to MaxSpeech", 500, 450);
            } else if let Some(w) = handle.get_webview_window("overlay") {
                let _ = w.show();
            }

            // Register global hotkeys
            hotkey::register_hotkeys(&handle);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running MaxSpeech");
}

fn open_window(app: &tauri::AppHandle, label: &str, title: &str, width: u32, height: u32) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, label, WebviewUrl::default())
        .title(title)
        .inner_size(width as f64, height as f64)
        .center()
        .build();
}
