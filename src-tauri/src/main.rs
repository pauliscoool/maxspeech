#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod context;
mod hotkey;
mod inject;
mod pipeline;
mod plan;
mod profiles;
mod recording;
mod secrets;
mod store;
mod stt;

use store::Store;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
};

fn stt_language_from_store(store: &Store) -> String {
    let tier = store
        .get_plan_status()
        .map(|s| s.tier)
        .unwrap_or(plan::PlanTier::Free);
    let multi_allowed = tier != plan::PlanTier::Free;
    let multilingual = multi_allowed
        && store
            .get_setting("stt_multilingual")
            .ok()
            .flatten()
            .as_deref()
            == Some("true");
    let languages = stt::deepgram::parse_languages_setting(
        store
            .get_setting("stt_languages")
            .ok()
            .flatten()
            .as_deref(),
    );
    let languages = stt::deepgram::effective_languages(multilingual, &languages);
    stt::deepgram::resolve_language(multilingual, &languages)
}

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
    let store = app.state::<Store>();
    let tier = store
        .get_plan_status()
        .map(|s| s.tier)
        .unwrap_or(plan::PlanTier::Free);
    let free_tier = tier == plan::PlanTier::Free;

    // Free plan: one language only — block multilingual and multi-select.
    if free_tier && key == "stt_multilingual" && value == "true" {
        return Err("Multilingual requires Starter or higher.".into());
    }
    let value = if key == "stt_languages" {
        let langs = stt::deepgram::parse_languages_setting(Some(&value));
        let multi = !free_tier
            && store
                .get_setting("stt_multilingual")
                .ok()
                .flatten()
                .as_deref()
                == Some("true");
        let effective = stt::deepgram::effective_languages(multi && !free_tier, &langs);
        serde_json::to_string(&effective).unwrap_or_else(|_| "[\"en\"]".into())
    } else {
        value
    };

    store.set_setting(&key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_microphones() -> Result<Vec<audio::MicDeviceInfo>, String> {
    // cpal device enumeration must not run on the async runtime thread on Windows.
    tauri::async_runtime::spawn_blocking(|| {
        audio::list_microphones().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_microphone(app: tauri::AppHandle) -> Result<String, String> {
    let store = app.state::<Store>();
    Ok(store
        .get_setting("mic_device")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| audio::MIC_DEVICE_DEFAULT.to_string()))
}

#[tauri::command]
async fn set_microphone(app: tauri::AppHandle, device: String) -> Result<String, String> {
    let store = app.state::<Store>();
    let trimmed = device.trim().to_string();
    let value = if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case(audio::MIC_DEVICE_DEFAULT)
    {
        audio::MIC_DEVICE_DEFAULT.to_string()
    } else {
        let want = trimmed.clone();
        let mics = tauri::async_runtime::spawn_blocking(move || {
            audio::list_microphones().map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        audio::resolve_saved_mic_name(&want, &mics)
            .filter(|v| v != audio::MIC_DEVICE_DEFAULT)
            .ok_or_else(|| format!("Microphone not found: {want}"))?
    };
    store
        .set_setting("mic_device", &value)
        .map_err(|e| e.to_string())?;
    log::info!("Microphone set to {value}");
    Ok(value)
}

#[tauri::command]
async fn test_microphone(app: tauri::AppHandle) -> Result<audio::MicTestResult, String> {
    let pref = app
        .state::<Store>()
        .get_setting("mic_device")
        .map_err(|e| e.to_string())?
        .filter(|s| !s.trim().is_empty());
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        audio::test_microphone(pref.as_deref(), Some(&app2)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<store::HistoryEntry>, String> {
    let store = app.state::<Store>();
    store
        .get_history(&search, limit.unwrap_or(10), offset.unwrap_or(0))
        .map_err(|e| e.to_string())
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
async fn transcribe_file(
    app: tauri::AppHandle,
    path: String,
) -> Result<stt::batch::TranscriptionResult, String> {
    let language = stt_language_from_store(&app.state::<Store>());
    stt::batch::transcribe_with_language(&path, &language)
        .await
        .map_err(|e| e.to_string())
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
    let email = store
        .get_setting("account_email")
        .ok()
        .flatten()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    const OWNER: &str = "pauldimov5@gmail.com";

    match parsed {
        plan::PlanTier::Free => {}
        plan::PlanTier::Max => {
            return Err(
                "Max isn't available as a free plan. Payment checkout is coming soon.".into(),
            );
        }
        plan::PlanTier::Starter | plan::PlanTier::Pro => {
            if email != OWNER {
                return Err(
                    "Payment checkout coming soon for paid plans. Free plan stays available."
                        .into(),
                );
            }
        }
    }

    store.set_plan_tier(parsed).map_err(|e| e.to_string())?;
    store.get_plan_status().map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_settings_page(app: tauri::AppHandle) -> Result<(), String> {
    open_window(&app, "settings", "MaxSpeech", 935, 612);
    let _ = app.emit("navigate-page", "settings");
    Ok(())
}

/// Download the latest installer from `url`, emit progress, then launch it.
/// Used when the signed Tauri updater isn't available yet.
#[tauri::command]
async fn download_and_run_installer(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use futures_util::StreamExt;
    use std::io::Write;

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Download failed: HTTP {}", response.status()));
    }

    let total = response.content_length().unwrap_or(0);
    let filename = url
        .rsplit('/')
        .next()
        .and_then(|s| {
            let clean = s.split('?').next().unwrap_or(s);
            if clean.is_empty() { None } else { Some(clean) }
        })
        .unwrap_or("MaxSpeech-update.bin");

    let path = std::env::temp_dir().join(filename);
    let mut file = std::fs::File::create(&path)
        .map_err(|e| format!("Could not write installer: {e}"))?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    let _ = app.emit("installer-download-progress", 0i32);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download interrupted: {e}"))?;
        file.write_all(&chunk)
            .map_err(|e| format!("Could not write installer: {e}"))?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let pct = ((downloaded * 100) / total).min(99) as i32;
            let _ = app.emit("installer-download-progress", pct);
        }
    }
    file.flush()
        .map_err(|e| format!("Could not finish installer write: {e}"))?;
    let _ = app.emit("installer-download-progress", 100u32);

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new(&path)
            .spawn()
            .map_err(|e| format!("Could not launch installer: {e}"))?;
        // Unlock the running binary so NSIS can replace it.
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        app.exit(0);
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Could not open installer: {e}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        if path.extension().and_then(|e| e.to_str()) == Some("AppImage") {
            let mut perms = std::fs::metadata(&path)
                .map_err(|e| e.to_string())?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).map_err(|e| e.to_string())?;
            std::process::Command::new(&path)
                .spawn()
                .map_err(|e| format!("Could not launch AppImage: {e}"))?;
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            app.exit(0);
        } else {
            std::process::Command::new("xdg-open")
                .arg(&path)
                .spawn()
                .map_err(|e| format!("Could not open installer: {e}"))?;
        }
    }

    Ok(())
}

/// Re-transcribe a recent local recording. Types into the focused app when it
/// isn't MaxSpeech; otherwise copies to the clipboard (injecting into our own
/// WebView scrolls the history list via Space key events).
#[tauri::command]
async fn remake_dictation(app: tauri::AppHandle, id: i64) -> Result<String, String> {
    let store = app.state::<Store>();
    let entry = store
        .get_history_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Dictation not found".to_string())?;

    if !entry.can_remake {
        return Err(
            "Remake is only available for your 10 most recent recordings still saved on this PC."
                .into(),
        );
    }
    let wav_path = entry
        .recording_path
        .clone()
        .ok_or_else(|| "Recording file missing".to_string())?;

    let language = stt_language_from_store(&store);
    let result = stt::batch::transcribe_with_language(&wav_path, &language)
        .await
        .map_err(|e| e.to_string())?;
    let text = result.text.trim().to_string();
    if text.is_empty() {
        return Err("Could not re-transcribe that recording.".into());
    }

    let has_llm = secrets::has_llm_api_key();
    let ai_enhance = store
        .get_setting("ai_enhance")
        .ok()
        .flatten()
        .map(|v| v != "false")
        .unwrap_or(true);

    let multilingual = language == "multi" || pipeline::tone::has_non_latin_script(&text);
    let mut output = if has_llm && ai_enhance {
        pipeline::tone::enhance_dictation_ex(&text, "default", false, multilingual)
            .await
            .unwrap_or(text.clone())
    } else if multilingual {
        text.clone()
    } else {
        pipeline::tone::local_self_correct(&text)
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

    let saved = output.trim_end().to_string();
    let _ = store.update_history_text(id, &saved);

    let fg_is_self = context::get_foreground_app()
        .map(|a| {
            let exe = a.exe.to_lowercase();
            exe.contains("maxspeech")
        })
        .unwrap_or(true);

    if fg_is_self {
        // Clicking Remake focuses MaxSpeech — don't type into the WebView.
        inject::copy_text(&saved).map_err(|e| e.to_string())?;
    } else {
        tokio::time::sleep(std::time::Duration::from_millis(350)).await;
        inject::inject_text(&output).map_err(|e| e.to_string())?;
    }
    Ok(saved)
}

fn main() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            // So login launches can stay tray-only while manual opens still show the UI.
            Some(vec!["--autostart"]),
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
            list_microphones,
            get_microphone,
            set_microphone,
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
            open_settings_page,
            download_and_run_installer,
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

            // Refresh Windows/macOS login item so it includes --autostart.
            {
                use tauri_plugin_autostart::ManagerExt;
                let launcher = handle.autolaunch();
                if launcher.is_enabled().unwrap_or(false) {
                    let _ = launcher.disable();
                    let _ = launcher.enable();
                }
            }

            // Onboarding always shows. Login autostart respects "show window at login".
            // Manual launches (Start menu / tray / second instance) always open the UI.
            let store = handle.state::<Store>();
            let from_autostart = std::env::args().any(|a| a == "--autostart");
            if !store.is_onboarded() {
                open_window(&handle, "onboarding", "Welcome to MaxSpeech", 560, 520);
            } else if from_autostart {
                let show_at_login = store
                    .get_setting("open_window_on_launch")
                    .ok()
                    .flatten()
                    .map(|v| v == "true")
                    .unwrap_or(false);
                if show_at_login {
                    open_window(&handle, "settings", "MaxSpeech", 935, 612);
                }
            } else {
                open_window(&handle, "settings", "MaxSpeech", 935, 612);
            }

            // Register global hotkeys
            hotkey::register_hotkeys(&handle);

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Closing the UI hides to tray — do not kill the process.
                match window.label() {
                    "settings" | "onboarding" => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    _ => {}
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building MaxSpeech")
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                // Closing the last window must not quit; tray Quit still exits (code = Some).
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
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
            width: 174.0,
            height: 36.0,
        }));
        if let Ok(Some(monitor)) = w.current_monitor() {
            let scale = monitor.scale_factor();
            let size = monitor.size();
            let screen_w = size.width as f64 / scale;
            let screen_h = size.height as f64 / scale;
            let x = (screen_w - 174.0) / 2.0;
            let y = screen_h - 36.0 - 48.0;
            let _ = w.set_position(Position::Logical(LogicalPosition { x, y }));
        }
        let _ = w.set_always_on_top(true);
        // Stay shown + click-through so WebView2 stays warm for instant hotkey paint.
        let _ = w.set_ignore_cursor_events(true);
        let _ = w.show();

        // Warm TLS/DNS to Deepgram so the first dictation handshake is faster.
        tauri::async_runtime::spawn(async {
            stt::deepgram::prewarm().await;
        });
    }
}
