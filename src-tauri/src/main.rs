#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod context;
mod hotkey;
mod inject;
mod pipeline;
mod plan;
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
async fn has_secret(key: String) -> Result<bool, String> {
    Ok(secrets::get_secret(&key)
        .map_err(|e| e.to_string())?
        .map(|v| !v.is_empty())
        .unwrap_or(false))
}

#[tauri::command]
async fn clear_secret(key: String) -> Result<(), String> {
    secrets::delete_secret(&key).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_setting(app: tauri::AppHandle, key: String) -> Result<String, String> {
    Ok(app
        .state::<Store>()
        .get_setting(&key)
        .map_err(|e| e.to_string())?
        .unwrap_or_default())
}

#[tauri::command]
async fn set_setting(app: tauri::AppHandle, key: String, value: String) -> Result<(), String> {
    app.state::<Store>()
        .set_setting(&key, &value)
        .map_err(|e| e.to_string())
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
    open_window(&app, "settings", "MaxSpeech", 935, 612);
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
async fn delete_history(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    app.state::<Store>()
        .delete_history(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_history_many(app: tauri::AppHandle, ids: Vec<i64>) -> Result<usize, String> {
    app.state::<Store>()
        .delete_history_many(&ids)
        .map_err(|e| e.to_string())
}

/// Clear keyring secrets, session meta, and history; reopen onboarding.
#[tauri::command]
async fn clear_session(app: tauri::AppHandle) -> Result<(), String> {
    for key in ["deepgram_api_key", "llm_api_key", "api_key", "openai_api_key"] {
        let _ = secrets::delete_secret(key);
    }

    let store = app.state::<Store>();
    store.clear_all_history().map_err(|e| e.to_string())?;
    store.clear_session_meta().map_err(|e| e.to_string())?;

    // Close main shell window(s) and show onboarding fresh.
    for label in ["settings", "main"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.close();
        }
    }
    if let Some(w) = app.get_webview_window("onboarding") {
        let _ = w.close();
    }
    open_window(&app, "onboarding", "Welcome to MaxSpeech", 560, 520);
    Ok(())
}

#[tauri::command]
async fn get_stats(app: tauri::AppHandle) -> Result<store::Stats, String> {
    app.state::<Store>().get_stats().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_dictionary(app: tauri::AppHandle) -> Result<Vec<store::DictWord>, String> {
    app.state::<Store>()
        .get_dictionary()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_dict_word(app: tauri::AppHandle, word: String) -> Result<(), String> {
    app.state::<Store>()
        .add_dict_word(&word, 1.0)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_dict_word(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    app.state::<Store>()
        .delete_dict_word(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_macros(app: tauri::AppHandle) -> Result<Vec<store::Macro>, String> {
    app.state::<Store>().get_macros().map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_macro(app: tauri::AppHandle, trigger: String, expansion: String) -> Result<(), String> {
    app.state::<Store>()
        .add_macro(&trigger, &expansion)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_macro(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    app.state::<Store>()
        .delete_macro(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_app_profiles(app: tauri::AppHandle) -> Result<Vec<store::AppProfile>, String> {
    app.state::<Store>()
        .get_app_profiles()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_app_profile(
    app: tauri::AppHandle,
    id: i64,
    tone: Option<String>,
    enabled: Option<bool>,
) -> Result<(), String> {
    app.state::<Store>()
        .update_app_profile(id, tone.as_deref(), enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_user_name() -> Result<String, String> {
    Ok(std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "there".into()))
}

#[tauri::command]
async fn transcribe_file(path: String) -> Result<stt::batch::TranscriptionResult, String> {
    stt::batch::transcribe(&path).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn export_transcription(format: String, text: String) -> Result<(), String> {
    stt::batch::export(&format, &text).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_hotkey(app: tauri::AppHandle) -> Result<String, String> {
    Ok(hotkey::get_hotkey(&app))
}

#[tauri::command]
async fn get_hotkey_mode(app: tauri::AppHandle) -> Result<String, String> {
    Ok(hotkey::get_hotkey_mode(&app))
}

#[tauri::command]
async fn set_hotkey(app: tauri::AppHandle, shortcut: String) -> Result<(), String> {
    hotkey::set_hotkey(&app, &shortcut)
}

#[tauri::command]
async fn set_hotkey_mode(app: tauri::AppHandle, mode: String) -> Result<(), String> {
    hotkey::set_hotkey_mode(&app, &mode)
}

#[tauri::command]
async fn get_plan_status(app: tauri::AppHandle) -> Result<plan::PlanStatus, String> {
    app.state::<Store>()
        .get_plan_status()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_plan_tier(app: tauri::AppHandle, tier: String) -> Result<plan::PlanStatus, String> {
    let store = app.state::<Store>();
    let parsed = plan::PlanTier::parse(&tier);
    store.set_plan_tier(parsed).map_err(|e| e.to_string())?;
    store.get_plan_status().map_err(|e| e.to_string())
}

/// Re-type a past dictation into the currently focused app.
#[tauri::command]
async fn remake_dictation(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let store = app.state::<Store>();
    let entry = store
        .get_history_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Dictation not found".to_string())?;

    let text = entry.text;
    let has_llm = secrets::has_llm_api_key();
    let ai_enhance = store
        .get_setting("ai_enhance")
        .ok()
        .flatten()
        .map(|v| v != "false")
        .unwrap_or(true);

    let mut output = if has_llm && ai_enhance {
        pipeline::tone::apply_tone(&text, "default")
            .await
            .unwrap_or(text)
    } else {
        text
    };

    let trailing = store
        .get_setting("trailing_space")
        .ok()
        .flatten()
        .map(|v| v != "false")
        .unwrap_or(true);
    if trailing && !output.ends_with(' ') {
        output.push(' ');
    }

    // Brief delay so the user can click back into their target app
    tokio::time::sleep(std::time::Duration::from_millis(350)).await;
    inject::inject_text(&output).map_err(|e| e.to_string())?;
    Ok(())
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            open_window(app, "settings", "MaxSpeech", 935, 612);
        }))
        .manage(Store::new().expect("Failed to initialize database"))
        .manage(pipeline::PipelineState::default())
        .invoke_handler(tauri::generate_handler![
            save_secret,
            has_secret,
            clear_secret,
            get_setting,
            set_setting,
            test_microphone,
            complete_onboarding,
            get_history,
            delete_history,
            delete_history_many,
            clear_session,
            get_stats,
            get_dictionary,
            add_dict_word,
            delete_dict_word,
            get_macros,
            add_macro,
            delete_macro,
            get_app_profiles,
            update_app_profile,
            get_user_name,
            transcribe_file,
            export_transcription,
            get_hotkey,
            get_hotkey_mode,
            set_hotkey,
            set_hotkey_mode,
            get_plan_status,
            set_plan_tier,
            remake_dictation,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            let show_item = MenuItem::with_id(app, "show", "Open MaxSpeech", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            // Single tray icon only (do not also set trayIcon in tauri.conf.json)
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("Missing default window icon")?;
            let _tray = TrayIconBuilder::with_id("maxspeech-tray")
                .icon(icon)
                .menu(&menu)
                .tooltip("MaxSpeech - Voice to Text")
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => {
                        open_window(app, "settings", "MaxSpeech", 935, 612);
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // Position floating dictation bar at bottom-center
            position_overlay(&handle);

            // Check if onboarding is needed
            let store = handle.state::<Store>();
            if !store.is_onboarded() {
                open_window(&handle, "onboarding", "Welcome to MaxSpeech", 560, 520);
            } else {
                open_window(&handle, "settings", "MaxSpeech", 935, 612);
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

fn clear_overlay_background(w: &tauri::WebviewWindow) {
    // WebView2 defaults to opaque white — force alpha 0 on both layers.
    let clear = tauri::window::Color(0, 0, 0, 0);
    let _ = w.set_background_color(Some(clear));
}

fn position_overlay(app: &tauri::AppHandle) {
    use tauri::{LogicalPosition, LogicalSize, Position, Size};
    if let Some(w) = app.get_webview_window("overlay") {
        clear_overlay_background(&w);
        let _ = w.set_shadow(false);
        let _ = w.set_size(Size::Logical(LogicalSize {
            width: 218.0,
            height: 38.0,
        }));
        if let Ok(Some(monitor)) = w.current_monitor() {
            let scale = monitor.scale_factor();
            let size = monitor.size();
            let screen_w = size.width as f64 / scale;
            let screen_h = size.height as f64 / scale;
            let x = (screen_w - 218.0) / 2.0;
            let y = screen_h - 38.0 - 48.0;
            let _ = w.set_position(Position::Logical(LogicalPosition { x, y }));
        }
        let _ = w.set_always_on_top(true);
        let _ = w.set_ignore_cursor_events(false);
    }
}
