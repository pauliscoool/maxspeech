#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod context;
mod hotkey;
mod inject;
mod overlay_win;
mod pipeline;
mod plan;
mod profiles;
mod recording;
mod secrets;
mod sound;
mod store;
mod stt;

use store::Store;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
};

/// Native error dialog — works even when the WebView never starts.
fn show_native_error(title: &str, message: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let to_wide = |s: &str| {
            std::ffi::OsStr::new(s)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect::<Vec<u16>>()
        };
        let t = to_wide(title);
        let m = to_wide(message);
        unsafe {
            let _ = MessageBoxW(
                None,
                PCWSTR(m.as_ptr()),
                PCWSTR(t.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("{title}: {message}");
        let _ = (title, message);
    }
}

fn logs_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(store::data_dir()).join("logs")
}

fn ensure_logs_dir() {
    let _ = std::fs::create_dir_all(logs_dir());
}

fn append_crash_log(text: &str) {
    ensure_logs_dir();
    let path = logs_dir().join("crash.log");
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let line = format!("\n===== {stamp} =====\n{text}\n");
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(line.as_bytes())
        });
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".into());
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".into()
        };
        let msg = format!("MaxSpeech crashed at {loc}\n\n{payload}\n\nA crash log was written to:\n{}", logs_dir().join("crash.log").display());
        append_crash_log(&msg);
        show_native_error("MaxSpeech crashed", &msg);
    }));
}

/// Returns WebView2 Evergreen Runtime version, if installed.
fn webview2_runtime_version() -> Option<String> {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::System::Registry::{
            RegCloseKey, RegGetValueW, RegOpenKeyExW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
            KEY_READ, RRF_RT_REG_SZ,
        };

        const GUID: &str = r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
        const GUID_NATIVE: &str =
            r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

        fn read_pv(root: windows::Win32::System::Registry::HKEY, subkey: &str) -> Option<String> {
            unsafe {
                let sub = subkey
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect::<Vec<u16>>();
                let mut hkey = Default::default();
                if RegOpenKeyExW(root, PCWSTR(sub.as_ptr()), Some(0), KEY_READ, &mut hkey).is_err() {
                    return None;
                }
                let name: Vec<u16> = "pv\0".encode_utf16().collect();
                let mut buf = vec![0u16; 64];
                let mut size = (buf.len() * 2) as u32;
                let status = RegGetValueW(
                    hkey,
                    None,
                    PCWSTR(name.as_ptr()),
                    RRF_RT_REG_SZ,
                    None,
                    Some(buf.as_mut_ptr() as *mut _),
                    Some(&mut size),
                );
                let _ = RegCloseKey(hkey);
                if status.is_err() {
                    return None;
                }
                let nul = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                let s = String::from_utf16_lossy(&buf[..nul]).trim().to_string();
                if s.is_empty() || s == "0.0.0.0" {
                    None
                } else {
                    Some(s)
                }
            }
        }

        read_pv(HKEY_LOCAL_MACHINE, GUID)
            .or_else(|| read_pv(HKEY_LOCAL_MACHINE, GUID_NATIVE))
            .or_else(|| read_pv(HKEY_CURRENT_USER, GUID))
            .or_else(|| read_pv(HKEY_CURRENT_USER, GUID_NATIVE))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn preflight_webview2() -> Result<(), String> {
    #[cfg(windows)]
    {
        if webview2_runtime_version().is_some() {
            return Ok(());
        }
        Err(
            "Microsoft Edge WebView2 Runtime is not installed.\n\n\
             MaxSpeech needs WebView2 to show its windows.\n\n\
             1. Install it from:\n\
                https://go.microsoft.com/fwlink/p/?LinkId=2124703\n\
             2. Then run MaxSpeech again.\n\n\
             (The full MaxSpeech installer also embeds WebView2.)"
                .into(),
        )
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

fn write_diagnose_report() -> Result<std::path::PathBuf, String> {
    ensure_logs_dir();
    let path = logs_dir().join("diagnose.txt");
    let data = store::data_dir();
    let db = std::path::Path::new(&data).join("maxspeech.db");
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unknown)".into());
    let wv = webview2_runtime_version().unwrap_or_else(|| "(not found)".into());
    let onboarded = Store::new()
        .ok()
        .map(|s| s.is_onboarded())
        .map(|b| b.to_string())
        .unwrap_or_else(|| "(db unavailable)".into());
    let body = format!(
        "MaxSpeech diagnose\n\
         time: {}\n\
         exe: {exe}\n\
         data_dir: {data}\n\
         db_exists: {}\n\
         db_path: {}\n\
         webview2: {wv}\n\
         onboarded: {onboarded}\n\
         args: {:?}\n\
         os: {}\n\
         arch: {}\n",
        chrono::Local::now().to_rfc3339(),
        db.exists(),
        db.display(),
        std::env::args().collect::<Vec<_>>(),
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    std::fs::write(&path, body).map_err(|e| e.to_string())?;
    Ok(path)
}

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
    // Fresh install: enable launch-at-startup unless the user already opted out.
    ensure_default_autostart(&app);
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

fn stored_account_email(store: &Store) -> String {
    store
        .get_setting("account_email")
        .ok()
        .flatten()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn is_owner_email(email: &str) -> bool {
    crate::plan::is_owner_email(email)
}

fn require_owner(store: &Store) -> Result<(), String> {
    if is_owner_email(&stored_account_email(store)) {
        Ok(())
    } else {
        Err("Only the owner account can edit usage and other people's plans.".into())
    }
}

#[tauri::command]
async fn set_plan_tier(app: tauri::AppHandle, tier: String) -> Result<plan::PlanStatus, String> {
    let store = app.state::<Store>();
    let parsed = plan::PlanTier::parse(&tier);
    let email = stored_account_email(&store);

    match parsed {
        plan::PlanTier::Free => {}
        plan::PlanTier::Starter | plan::PlanTier::Pro | plan::PlanTier::Max => {
            if !is_owner_email(&email) {
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

/// Apply a plan from cloud sync without the checkout gate (already entitled).
#[tauri::command]
async fn sync_plan_tier(app: tauri::AppHandle, tier: String) -> Result<plan::PlanStatus, String> {
    let store = app.state::<Store>();
    let parsed = plan::PlanTier::parse(&tier);
    store.set_plan_tier(parsed).map_err(|e| e.to_string())?;
    store.get_plan_status().map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_usage_bonus(app: tauri::AppHandle, bonus: u64) -> Result<plan::PlanStatus, String> {
    let store = app.state::<Store>();
    require_owner(&store)?;
    store.set_usage_bonus(bonus).map_err(|e| e.to_string())?;
    store.get_plan_status().map_err(|e| e.to_string())
}

#[tauri::command]
async fn adjust_usage(app: tauri::AppHandle, delta: i64) -> Result<plan::PlanStatus, String> {
    let store = app.state::<Store>();
    require_owner(&store)?;
    store.adjust_usage(delta).map_err(|e| e.to_string())?;
    store.get_plan_status().map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_words_used(app: tauri::AppHandle, words: u64) -> Result<plan::PlanStatus, String> {
    let store = app.state::<Store>();
    require_owner(&store)?;
    let current = store.words_this_week().map_err(|e| e.to_string())?;
    let delta = words as i64 - current as i64;
    store.adjust_usage(delta).map_err(|e| e.to_string())?;
    store.get_plan_status().map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_settings_page(app: tauri::AppHandle) -> Result<(), String> {
    open_window(&app, "settings", "MaxSpeech", 935, 612);
    let _ = app.emit("navigate-page", "settings");
    Ok(())
}

#[tauri::command]
async fn open_plans_modal(app: tauri::AppHandle) -> Result<(), String> {
    open_window(&app, "settings", "MaxSpeech", 935, 612);
    let _ = app.emit("open-plans", ());
    Ok(())
}

/// Clip overlay HWND to a capsule, or drop the clip for toast/limit chrome.
#[tauri::command]
fn set_overlay_pill_clip(app: tauri::AppHandle, apply: bool) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") {
        if apply {
            overlay_win::apply_pill_region(&w);
        } else {
            overlay_win::clear_pill_region(&w);
        }
    }
    Ok(())
}

/// Park the overlay off-screen in one main-thread step (no white dismiss flash).
#[tauri::command]
fn park_overlay_idle(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") {
        overlay_win::park_idle(&w);
    }
    Ok(())
}

/// Click-through toggle that never leaves the overlay `WS_EX_LAYERED`.
/// The UI must use this instead of `setIgnoreCursorEvents`, which layers the
/// HWND and makes every transparent pixel composite as opaque white.
#[tauri::command]
fn set_overlay_click_through(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") {
        overlay_win::set_click_through(&w, enabled);
    }
    Ok(())
}

/// Preview the custom dictation cue at the given volume (or saved setting).
#[tauri::command]
async fn preview_sound_cue(app: tauri::AppHandle, volume: Option<String>) -> Result<(), String> {
    let store = app.state::<Store>();
    let label = volume.or_else(|| {
        store
            .get_setting("sound_cue_volume")
            .ok()
            .flatten()
    });
    let vol = sound::volume_from_setting(label.as_deref());
    sound::play_cue(sound::CueKind::Start, vol);
    Ok(())
}

#[cfg(windows)]
fn spawn_detached_nsis_updater(installer: &std::path::Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    // CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS | CREATE_BREAKAWAY_FROM_JOB | CREATE_NO_WINDOW
    const FLAGS: u32 = 0x0000_0200 | 0x0000_0008 | 0x0100_0000 | 0x0800_0000;
    std::process::Command::new(installer)
        .args(["/S", "/UPDATE"])
        .creation_flags(FLAGS)
        .spawn()
        .map_err(|e| format!("Could not launch installer: {e}"))?;
    Ok(())
}

/// Terminate any other maxspeech.exe so NSIS can overwrite. Leaves this PID
/// alone; `process::exit` finishes the job. Overlay lives in this process.
#[cfg(windows)]
fn kill_other_maxspeech_processes() {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let pid = std::process::id();
    let _ = std::process::Command::new("taskkill")
        .args([
            "/F",
            "/IM",
            "maxspeech.exe",
            "/FI",
            &format!("PID ne {pid}"),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
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
        // Brief pause so the UI can show "Restarting…" before we die.
        tokio::time::sleep(std::time::Duration::from_millis(1400)).await;

        // Silent + update mode. Do NOT pass /R: Tauri's RunAsUser waits until
        // the tray app exits and hangs the installer. POSTINSTALL ShellExecute
        // launches the app asynchronously instead. /UPDATE skips uninstall-first.
        //
        // Detach from our job/process group so exiting MaxSpeech cannot take
        // the installer down with it (WebView2 job objects).
        spawn_detached_nsis_updater(&path)?;
        for (_, w) in app.webview_windows() {
            let _ = w.hide();
        }
        kill_other_maxspeech_processes();
        // Hard-quit so NSIS can overwrite the running binary. `app.exit` can
        // race with tray keep-alive; process::exit is definitive. PREINSTALL
        // also KillProcess as a backup; POSTINSTALL starts the new build.
        // Do not return Ok(()) — the IPC completing lets the frontend think
        // install finished while this process is still locking the exe.
        std::process::exit(0);
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
    let dict_terms: Vec<String> = store
        .get_dictionary()
        .unwrap_or_default()
        .into_iter()
        .map(|w| w.word)
        .collect();
    let keyterms = stt::deepgram::merge_keyterms(dict_terms.clone());
    let result =
        stt::batch::transcribe_with_language_and_keyterms(&wav_path, &language, &keyterms)
            .await
            .map_err(|e| e.to_string())?;
    let text = result.text.trim().to_string();
    if text.is_empty() {
        return Err("Could not re-transcribe that recording.".into());
    }

    let expanded = pipeline::vocab::expand_macros(&text, &store);

    let has_llm = secrets::has_llm_api_key();
    let ai_enhance = store
        .get_setting("ai_enhance")
        .ok()
        .flatten()
        .map(|v| v != "false")
        .unwrap_or(true);

    let multilingual = language == "multi" || pipeline::tone::has_non_latin_script(&expanded);
    let corrected = if multilingual {
        expanded.clone()
    } else {
        pipeline::tone::local_self_correct(&expanded)
    };
    if corrected.trim() != expanded.trim() {
        pipeline::vocab::learn_name_corrections(&expanded, &corrected, &store);
        pipeline::learn_substitutions::learn_from_edit(&expanded, &corrected, &store);
    }

    let fg = context::get_foreground_app();
    let tone_name = fg
        .as_ref()
        .and_then(|a| pipeline::tone::get_tone_for_app(a, &store))
        .unwrap_or_else(|| "default".to_string());

    let mut output = if has_llm && ai_enhance {
        let speed = pipeline::tone::EnhanceSpeed::from_store(&store);
        pipeline::tone::enhance_dictation_ex(
            &corrected,
            &tone_name,
            false,
            multilingual,
            &dict_terms,
            speed,
        )
        .await
        .unwrap_or(corrected.clone())
    } else {
        corrected
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

    let fg_is_self = fg
        .as_ref()
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
        let inserted = inject::inject_text(&output).map_err(|e| e.to_string())?;
        let pipeline_state = app.state::<pipeline::PipelineState>();
        *pipeline_state.last_insertion.lock().unwrap() = Some(inserted);
    }
    Ok(saved)
}

/// Edit a history entry and persist any name-like corrections into the dictionary.
#[tauri::command]
async fn update_history_text(
    app: tauri::AppHandle,
    id: i64,
    text: String,
) -> Result<String, String> {
    let store = app.state::<Store>();
    let entry = store
        .get_history_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Dictation not found".to_string())?;
    let cleaned = text.trim().to_string();
    pipeline::vocab::learn_name_corrections(&entry.text, &cleaned, &store);
    pipeline::learn_substitutions::learn_from_edit(&entry.text, &cleaned, &store);
    store
        .update_history_text(id, &cleaned)
        .map_err(|e| e.to_string())?;
    Ok(cleaned)
}

/// Enable launch-at-startup on first run only when the preference was never set.
/// If the user previously chose off (`launch_at_startup=false`), leave it disabled.
fn ensure_default_autostart(app: &tauri::AppHandle) {
    use tauri_plugin_autostart::ManagerExt;
    let store = app.state::<Store>();
    let pref = store
        .get_setting("launch_at_startup")
        .ok()
        .flatten()
        .map(|v| v.trim().to_ascii_lowercase());
    let launcher = app.autolaunch();
    match pref.as_deref() {
        Some("false") | Some("0") | Some("off") | Some("no") => {
            let _ = launcher.disable();
        }
        Some("true") | Some("1") | Some("on") | Some("yes") => {
            let _ = launcher.enable();
        }
        _ => {
            // Unset → default ON for fresh installs.
            if launcher.enable().is_ok() {
                let _ = store.set_setting("launch_at_startup", "true");
            }
        }
    }
}

fn main() {
    install_panic_hook();
    ensure_logs_dir();

    if std::env::args().any(|a| a == "--diagnose") {
        match write_diagnose_report() {
            Ok(path) => {
                // Prefer opening the report over a blocking MessageBox so
                // scripted installs / support sessions don't hang.
                #[cfg(windows)]
                {
                    use std::os::windows::ffi::OsStrExt;
                    use windows::core::PCWSTR;
                    use windows::Win32::UI::WindowsAndMessaging::MessageBoxW;
                    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_OK};
                    let msg = format!(
                        "Wrote diagnostics to:\n{}\n\nClick OK to close.",
                        path.display()
                    );
                    let to_wide = |s: &str| {
                        std::ffi::OsStr::new(s)
                            .encode_wide()
                            .chain(std::iter::once(0))
                            .collect::<Vec<u16>>()
                    };
                    let t = to_wide("MaxSpeech diagnose");
                    let m = to_wide(&msg);
                    unsafe {
                        let _ = MessageBoxW(
                            None,
                            PCWSTR(m.as_ptr()),
                            PCWSTR(t.as_ptr()),
                            MB_OK | MB_ICONINFORMATION,
                        );
                    }
                }
                #[cfg(not(windows))]
                {
                    println!("Wrote diagnostics to {}", path.display());
                }
            }
            Err(e) => show_native_error("MaxSpeech diagnose failed", &e),
        }
        return;
    }

    if let Err(e) = preflight_webview2() {
        append_crash_log(&format!("WebView2 preflight failed: {e}"));
        show_native_error("MaxSpeech needs WebView2", &e);
        return;
    }

    let store = match Store::new() {
        Ok(s) => s,
        Err(e) => {
            append_crash_log(&format!("Store init failed: {e}"));
            show_native_error(
                "MaxSpeech cannot start",
                &format!("{e}\n\nLogs: {}", logs_dir().display()),
            );
            return;
        }
    };

    let log_plugin = tauri_plugin_log::Builder::new()
        .targets([
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Folder {
                path: logs_dir(),
                file_name: Some("maxspeech".into()),
            }),
        ])
        .level(log::LevelFilter::Info)
        .build();

    let app = match tauri::Builder::default()
        .plugin(log_plugin)
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            open_window(app, "settings", "MaxSpeech", 935, 612);
        }))
        .manage(store)
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
            sync_plan_tier,
            set_usage_bonus,
            adjust_usage,
            set_words_used,
            open_settings_page,
            open_plans_modal,
            set_overlay_pill_clip,
            park_overlay_idle,
            set_overlay_click_through,
            preview_sound_cue,
            download_and_run_installer,
            remake_dictation,
            update_history_text,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            log::info!(
                "MaxSpeech starting; data_dir={} webview2={:?}",
                store::data_dir(),
                webview2_runtime_version()
            );

            let show_item = MenuItem::with_id(app, "show", "Open MaxSpeech", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            let icon = match app.default_window_icon().cloned() {
                Some(i) => i,
                None => {
                    log::error!("Missing default window icon — continuing without tray");
                    let store = handle.state::<Store>();
                    if !store.is_onboarded() {
                        open_window(&handle, "onboarding", "Welcome to MaxSpeech", 560, 520);
                    } else {
                        open_window(&handle, "settings", "MaxSpeech", 935, 612);
                    }
                    hotkey::register_hotkeys(&handle);
                    return Ok(());
                }
            };

            match TrayIconBuilder::with_id("maxspeech-tray")
                .icon(icon)
                .menu(&menu)
                .tooltip("MaxSpeech - Voice to Text")
                .show_menu_on_left_click(false)
                .on_menu_event({
                    let handle = handle.clone();
                    move |app, event| match event.id().as_ref() {
                        "show" => {
                            let store = handle.state::<Store>();
                            if !store.is_onboarded() {
                                open_window(app, "onboarding", "Welcome to MaxSpeech", 560, 520);
                            } else {
                                open_window(app, "settings", "MaxSpeech", 935, 612);
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event({
                    let handle = handle.clone();
                    move |tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            let store = handle.state::<Store>();
                            if !store.is_onboarded() {
                                open_window(app, "onboarding", "Welcome to MaxSpeech", 560, 520);
                            } else {
                                open_window(app, "settings", "MaxSpeech", 935, 612);
                            }
                        }
                    }
                })
                .build(app)
            {
                Ok(_tray) => {}
                Err(e) => {
                    log::error!("Tray icon failed (non-fatal): {e}");
                }
            }

            ensure_default_autostart(&handle);

            let store = handle.state::<Store>();
            let from_autostart = std::env::args().any(|a| a == "--autostart");
            if !store.is_onboarded() {
                open_window(&handle, "onboarding", "Welcome to MaxSpeech", 560, 520);
                let already = store
                    .get_setting("tray_tip_shown")
                    .ok()
                    .flatten()
                    .map(|v| v == "true")
                    .unwrap_or(false);
                if !already {
                    let _ = store.set_setting("tray_tip_shown", "true");
                    use tauri_plugin_notification::NotificationExt;
                    let _ = handle
                        .notification()
                        .builder()
                        .title("MaxSpeech is running")
                        .body("Look for the tray icon if the window is closed. Click it to reopen.")
                        .show();
                }
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

            hotkey::register_hotkeys(&handle);

            // Warm Deepgram TLS/DNS and the overlay WebView2 off the hotkey path.
            // Queue overlay on the next UI tick (after this setup returns) so the
            // first press does not create a WebView2 while opening WASAPI.
            tauri::async_runtime::spawn(async move {
                stt::deepgram::prewarm().await;
            });
            let warm_ui = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                ensure_overlay_window(&warm_ui);
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                match window.label() {
                    "onboarding" => {
                        let store = window.app_handle().state::<Store>();
                        if !store.is_onboarded() {
                            api.prevent_close();
                            let _ = window.show();
                            let _ = window.set_focus();
                            return;
                        }
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    "settings" => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    _ => {}
                }
            }
        })
        .build(tauri::generate_context!())
    {
        Ok(app) => app,
        Err(e) => {
            let msg = format!(
                "MaxSpeech failed to start:\n{e}\n\nLogs: {}\n\n\
                 If WebView2 is missing, install it from:\n\
                 https://go.microsoft.com/fwlink/p/?LinkId=2124703",
                logs_dir().display()
            );
            append_crash_log(&msg);
            show_native_error("MaxSpeech failed to start", &msg);
            return;
        }
    };

    app.run(|_app, event| {
        if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}

fn open_window(app: &tauri::AppHandle, label: &str, title: &str, width: u32, height: u32) {
    if let Some(w) = app.get_webview_window(label) {
        if label != "overlay" {
            let _ = w.set_background_color(Some(tauri::window::Color(0, 0, 0, 255)));
        }
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::default())
        .title(title)
        .inner_size(width as f64, height as f64)
        .center();
    if label != "overlay" {
        builder = builder.background_color(tauri::window::Color(0, 0, 0, 255));
    }
    if let Err(e) = builder.build() {
        let msg = format!(
            "Could not open the {label} window:\n{e}\n\n\
             Logs: {}\n\n\
             Try installing/repairing WebView2:\n\
             https://go.microsoft.com/fwlink/p/?LinkId=2124703",
            logs_dir().display()
        );
        log::error!("{msg}");
        append_crash_log(&msg);
        show_native_error("MaxSpeech window error", &msg);
    }
}

/// Create the listening overlay on demand. Startup queues this on the next UI
/// tick so the first hotkey does not have to boot WebView2. If it is still
/// missing, `show_overlay_fast` creates it *after* mic + Deepgram have started.
pub(crate) fn ensure_overlay_window(app: &tauri::AppHandle) {
    if app.get_webview_window("overlay").is_some() {
        return;
    }
    let builder = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("index.html?window=overlay".into()))
        .title("MaxSpeech Overlay")
        .inner_size(148.0, 36.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .shadow(false)
        .background_color(tauri::window::Color(18, 18, 18, 0));
    match builder.build() {
        Ok(w) => {
            overlay_win::park_idle(&w);
            tauri::async_runtime::spawn(async {
                stt::deepgram::prewarm().await;
            });
        }
        Err(e) => log::warn!("Could not create overlay window: {e}"),
    }
}

