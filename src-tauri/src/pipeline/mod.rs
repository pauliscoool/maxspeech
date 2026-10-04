pub mod agent_llm;
pub mod commands;
pub mod learn_substitutions;
pub mod recovery;
pub mod tone;
pub mod vocab;

use crate::audio::AudioCapture;
use crate::context;
use crate::inject::{self, LastInsertion};
use crate::recording::{self, MAX_REMAKE_RECORDINGS, WAV_SAMPLE_RATE};
use crate::secrets;
use crate::store::Store;
use crate::stt::deepgram::{self, DeepgramConfig, TranscriptChunk};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use tokio::sync::mpsc;

/// Hard cap for a single push-to-talk session.
const MAX_RECORDING: Duration = Duration::from_secs(120);

/// Never cut the mic sooner than this after release (capture + WASAPI buffer latency).
const TRAIL_MIN_MS: u64 = 150;
/// Silence after release that means the speaker is finished.
const TRAIL_QUIET_MS: u64 = 200;

pub struct PipelineState {
    pub active: Mutex<bool>,
    pub last_insertion: Mutex<Option<LastInsertion>>,
    /// Previous paste text if the new recording started within the learn window.
    pending_learn_from: Mutex<Option<String>>,
    stop_tx: Mutex<Option<mpsc::Sender<()>>>,
    audio_capture: Mutex<AudioCapture>,
    /// Wall-clock start of the current recording session.
    started_at: Mutex<Option<Instant>>,
    /// Duration of the session that just stopped (for trail / enhance decisions).
    last_session_secs: Mutex<f64>,
    /// Bumped on each start/stop so orphaned max-length timers cannot kill a newer session.
    session_gen: Mutex<u64>,
    /// Bumped only when a *new* recording starts. A finishing enhance/inject from an
    /// older recording skips paste if this no longer matches (prevents half+half).
    /// Re-checked inside inject after the modifier wait so mid-wait starts cancel.
    paste_epoch: Mutex<u64>,
    /// 16 kHz mono PCM for the active session (Remake cache).
    session_pcm: Mutex<Option<Arc<Mutex<Vec<i16>>>>>,
    /// Foreground app captured at hotkey-down (tone + history must use this, not
    /// whatever is focused after enhance finishes).
    session_fg: Mutex<Option<context::ForegroundApp>>,
    /// Window focused at hotkey-down, so overlay ✓/✗ clicks can hand focus back.
    session_hwnd: Mutex<isize>,
    /// Deepgram returned any words for the current session.
    heard_text: AtomicBool,
    /// Paste token of a session the user cancelled or that held no speech; its
    /// pipeline must end silently (no paste, no history, no failure toast).
    discarded_paste: Mutex<u64>,
    /// Hotkey release of the take being processed (release->paste timing log only).
    released_at: Mutex<Option<Instant>>,
}

impl Default for PipelineState {
    fn default() -> Self {
        Self {
            active: Mutex::new(false),
            last_insertion: Mutex::new(None),
            pending_learn_from: Mutex::new(None),
            stop_tx: Mutex::new(None),
            audio_capture: Mutex::new(AudioCapture::new()),
            started_at: Mutex::new(None),
            last_session_secs: Mutex::new(0.0),
            session_gen: Mutex::new(0),
            paste_epoch: Mutex::new(0),
            session_pcm: Mutex::new(None),
            session_fg: Mutex::new(None),
            session_hwnd: Mutex::new(0),
            heard_text: AtomicBool::new(false),
            discarded_paste: Mutex::new(0),
            released_at: Mutex::new(None),
        }
    }
}

fn focus_login_ui(app: &tauri::AppHandle) {
    for label in ["settings", "onboarding", "main"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.show();
            let _ = w.set_focus();
            return;
        }
    }
}

fn show_overlay_fast(app: &tauri::AppHandle) {
    show_overlay_with(app, false);
}

/// `controls`: toggle-mode pill with ✗ / ✓ buttons (wider, clickable).
fn show_overlay_with(app: &tauri::AppHandle, controls: bool) {
    // Hotkey path runs on a worker thread — WebView2/DWM clears only stick on the
    // UI thread. Queue there; fall back to inline if scheduling fails.
    let app2 = app.clone();
    if app
        .run_on_main_thread(move || show_overlay_fast_inner(&app2, controls))
        .is_err()
    {
        show_overlay_fast_inner(app, controls);
    }
}

fn hide_overlay_fast(app: &tauri::AppHandle) {
    let app2 = app.clone();
    if app
        .run_on_main_thread(move || {
            if let Some(w) = app2.get_webview_window("overlay") {
                crate::overlay_win::park_idle(&w);
            }
        })
        .is_err()
    {
        if let Some(w) = app.get_webview_window("overlay") {
            crate::overlay_win::park_idle(&w);
        }
    }
}

fn hide_overlay_if_current(app: &tauri::AppHandle, paste_token: u64) {
    if !paste_still_current(app, paste_token) {
        return;
    }
    hide_overlay_fast(app);
}

/// Bottom-center pill slot on the monitor under the cursor (falls back to
/// primary). Recomputed every hotkey — a OnceLock left Ctrl+Win stuck on the
/// first screen forever, and `current_monitor()` is wrong while parked off-screen.
fn overlay_listening_slot(w: &tauri::WebviewWindow, pill_w: f64) -> (i32, i32) {
    let monitor = w
        .cursor_position()
        .ok()
        .and_then(|p| w.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| w.primary_monitor().ok().flatten())
        .or_else(|| w.current_monitor().ok().flatten());

    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let size = monitor.size();
        let origin = monitor.position();
        let pill_w = (pill_w * scale).round() as i32;
        let pill_h = (crate::overlay_win::PILL_H * scale).round() as i32;
        let margin = (48.0 * scale).round() as i32;
        (
            origin.x + (size.width as i32 - pill_w) / 2,
            origin.y + size.height as i32 - pill_h - margin,
        )
    } else {
        (873, 996)
    }
}

fn show_overlay_fast_inner(app: &tauri::AppHandle, controls: bool) {
    // Only create a WebView2 here if startup pre-warm missed. Callers start
    // mic + Deepgram *before* this so a cold overlay cannot steal the first seconds.
    let cold = app.get_webview_window("overlay").is_none();
    crate::ensure_overlay_window(app);
    if let Some(w) = app.get_webview_window("overlay") {
        let (x, y) = overlay_listening_slot(&w, crate::overlay_win::pill_width(controls));
        // Clip while still parked, then one SetWindowPos — never reveal a
        // rectangular HWND for a frame.
        crate::overlay_win::reveal_listening(&w, x, y, controls);
    }
    if cold {
        replay_listening_for_late_overlay(app);
    }
}

/// Fresh overlay WebView2 may miss the first `dictation-state` emit. Replay
/// while this session is still recording so React can enter connecting/live.
fn replay_listening_for_late_overlay(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for ms in [150u64, 400, 900] {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            let state = app.state::<PipelineState>();
            if !*state.active.lock().unwrap() {
                return;
            }
            let _ = app.emit("dictation-state", "listening");
            // Toggle mode's ✗/✓ too, or a cold overlay is wide and clickable with no buttons.
            let _ = app.emit("dictation-controls", crate::hotkey::get_hotkey_mode(&app) == "toggle");
        }
    });
}

pub(crate) fn is_signed_in(store: &Store) -> bool {
    // Must be explicitly unlocked by the UI after a real login / Continue locally.
    // Stale account_email alone must not allow hotkey dictation on the login screen.
    let unlocked = store
        .get_setting("dictation_unlocked")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);
    if !unlocked {
        return false;
    }
    store
        .get_setting("account_email")
        .ok()
        .flatten()
        .map(|e| !e.trim().is_empty())
        .unwrap_or(false)
}

/// Open WASAPI on a worker so overlay WebView2 / UI-thread work cannot delay the mic.
fn spawn_mic_start(
    app: &tauri::AppHandle,
    session_id: u64,
    tee_tx: mpsc::UnboundedSender<Vec<i16>>,
    mic_pref: Option<String>,
) {
    let app_mic = app.clone();
    if let Err(e) = std::thread::Builder::new()
        .name("maxspeech-mic".into())
        .spawn(move || {
            let state = app_mic.state::<PipelineState>();
            if *state.session_gen.lock().unwrap() != session_id || !*state.active.lock().unwrap() {
                return;
            }
            let result = {
                let mut capture = state.audio_capture.lock().unwrap();
                capture.start(tee_tx, app_mic.clone(), mic_pref.as_deref())
            };
            match result {
                Err(e) => {
                    if *state.session_gen.lock().unwrap() != session_id {
                        return;
                    }
                    log::error!("Failed to start audio capture: {e}");
                    let _ = app_mic.emit("dictation-error", format!("Mic error: {e}"));
                    let _ = app_mic.emit("dictation-state", "error");
                    *state.session_pcm.lock().unwrap() = None;
                    invalidate_session(&state);
                }
                Ok(()) => {
                    // stop_dictation won the race — don't leave an orphan stream.
                    if *state.session_gen.lock().unwrap() != session_id
                        || !*state.active.lock().unwrap()
                    {
                        state.audio_capture.lock().unwrap().stop();
                    }
                }
            }
        })
    {
        log::error!("Failed to spawn mic thread: {e}");
    }
}

pub fn start_dictation(app: &tauri::AppHandle) {
    let state = app.state::<PipelineState>();
    {
        let mut active = state.active.lock().unwrap();
        if *active {
            return;
        }
        *active = true;
    }

    // Auth / plan are cheap SQLite reads. Do them before opening the mic, but
    // do NOT paint the overlay yet — a lively pill while WASAPI/WS are down is
    // how "listening UI but not hearing" happens. Failures still show the pill.
    if !app
        .try_state::<Store>()
        .map(|s| is_signed_in(&s))
        .unwrap_or(false)
    {
        *state.active.lock().unwrap() = false;
        let _ = app.emit("dictation-error", "Sign in to use dictation");
        let _ = app.emit("dictation-state", "error");
        show_overlay_fast(app);
        focus_login_ui(app);
        return;
    }

    // Plan quota: Free is 2 minutes / 24h; paid is weekly words (Mon 00:00 UTC).
    let plan_status = app
        .try_state::<Store>()
        .and_then(|store| store.get_plan_status().ok());
    if let Some(ref status) = plan_status {
        if !status.can_dictate {
            let limit_msg = if status.daily_seconds_limit.is_some() {
                "Daily limit reached"
            } else {
                "Weekly limit reached"
            };
            let _ = app.emit("dictation-limit", limit_msg);
            let _ = app.emit("dictation-state", "limit");
            show_overlay_fast(app);
            let _ = app
                .get_webview_window("overlay")
                .map(|w| crate::overlay_win::set_click_through(&w, false));
            *state.active.lock().unwrap() = false;
            return;
        }
    }

    let session_id = {
        let mut gen = state.session_gen.lock().unwrap();
        *gen = gen.wrapping_add(1);
        *gen
    };
    let paste_token = {
        let mut epoch = state.paste_epoch.lock().unwrap();
        *epoch = epoch.wrapping_add(1);
        *epoch
    };
    *state.started_at.lock().unwrap() = Some(Instant::now());
    // Snapshot focus now — enhance/history must not use a later app switch.
    *state.session_fg.lock().unwrap() = context::get_foreground_app();
    *state.session_hwnd.lock().unwrap() = context::foreground_hwnd();
    state.heard_text.store(false, Ordering::SeqCst);
    {
        let last = state.last_insertion.lock().unwrap();
        let pending = last.as_ref().and_then(|ins| {
            if learn_substitutions::elapsed_within_window(ins.pasted_at.elapsed()) {
                Some(ins.text.clone())
            } else {
                None
            }
        });
        *state.pending_learn_from.lock().unwrap() = pending;
    }

    let remaining_cap = plan_status
        .as_ref()
        .and_then(|s| s.seconds_remaining)
        .map(|s| Duration::from_secs(s.max(1)))
        .unwrap_or(MAX_RECORDING)
        .min(MAX_RECORDING);
    let free_time_cap = plan_status
        .as_ref()
        .and_then(|s| s.daily_seconds_limit)
        .is_some();
    let plan_tier = plan_status
        .as_ref()
        .map(|s| s.tier)
        .unwrap_or(crate::plan::PlanTier::Free);

    log::info!(
        "Dictation started session={session_id} paste={paste_token} (max {}s wall-clock)",
        remaining_cap.as_secs()
    );

    let keywords: Vec<String> = app
        .try_state::<Store>()
        .and_then(|store| store.get_dictionary().ok())
        .unwrap_or_default()
        .into_iter()
        .map(|w| w.word)
        .collect();
    let (multilingual, languages) = app
        .try_state::<Store>()
        .map(|store| {
            // Free tier: single language only (no code-switching).
            let multi_allowed = plan_tier != crate::plan::PlanTier::Free;
            let multi = multi_allowed
                && store
                    .get_setting("stt_multilingual")
                    .ok()
                    .flatten()
                    .as_deref()
                    == Some("true");
            let langs = deepgram::parse_languages_setting(
                store
                    .get_setting("stt_languages")
                    .ok()
                    .flatten()
                    .as_deref(),
            );
            let langs = deepgram::effective_languages(multi, &langs);
            // Heal persisted settings off the hot path (async) if needed.
            if let Ok(Some(raw)) = store.get_setting("stt_languages") {
                let parsed = deepgram::parse_languages_setting(Some(&raw));
                if parsed != langs {
                    let heal_langs = langs.clone();
                    let heal_from = parsed.clone();
                    let heal_multi = multi;
                    let app_heal = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Some(store) = app_heal.try_state::<Store>() {
                            let _ = store.set_setting(
                                "stt_languages",
                                &serde_json::to_string(&heal_langs)
                                    .unwrap_or_else(|_| "[\"en\"]".into()),
                            );
                            if !heal_multi {
                                let _ = store.set_setting("stt_multilingual", "false");
                            }
                            log::warn!(
                                "Healed stt_languages from {heal_from:?} → {heal_langs:?} (multilingual={heal_multi})"
                            );
                        }
                    });
                }
            }
            (multi, langs)
        })
        .unwrap_or_else(|| (false, vec!["en".to_string()]));
    let language = deepgram::resolve_language(multilingual, &languages);
    log::info!("STT language={language} multilingual={multilingual} selected={languages:?}");
    let language_for_pipeline = language.clone();

    // Empty api_key → stream_audio tries cached / user / app fallback keys.
    let config = DeepgramConfig {
        api_key: String::new(),
        keywords: deepgram::merge_keyterms(keywords),
        language,
        ..Default::default()
    };

    let retry_keyterms = config.keywords.clone();
    let stt_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let stream_complete = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let session_pcm: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    *state.session_pcm.lock().unwrap() = Some(session_pcm.clone());

    let (tee_tx, mut tee_rx) = mpsc::unbounded_channel::<Vec<i16>>();
    let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<i16>>();
    let (transcript_tx, mut transcript_rx) = mpsc::unbounded_channel::<TranscriptChunk>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>(1);

    *state.stop_tx.lock().unwrap() = Some(stop_tx);

    // Tee mic chunks into the Remake PCM buffer and the STT stream.
    let pcm_tee = session_pcm.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(chunk) = tee_rx.recv().await {
            if let Ok(mut buf) = pcm_tee.lock() {
                buf.extend_from_slice(&chunk);
            }
            // A dead STT socket must not stop the local recording: Remake and the
            // batch retry need the whole take, not just the part before the drop.
            let _ = audio_tx.send(chunk);
        }
    });

    // Deepgram TLS/WS and WASAPI must start immediately — overlapping overlay
    // paint / WebView2 creation. stream_audio drains PCM during the handshake
    // so the first seconds are not dropped.
    let stt_error_writer = stt_error.clone();
    let stream_complete_writer = stream_complete.clone();
    tauri::async_runtime::spawn(async move {
        match deepgram::stream_audio(config, audio_rx, transcript_tx, stop_rx).await {
            Ok(true) => {}
            Ok(false) => {
                log::warn!("Deepgram stream incomplete — will re-transcribe from the recording");
                stream_complete_writer.store(false, std::sync::atomic::Ordering::SeqCst);
            }
            Err(e) => {
                // No error pill here: the recording continues and the batch retry /
                // failure notification after release report the outcome.
                log::error!("Deepgram stream error: {e}");
                if let Ok(mut slot) = stt_error_writer.lock() {
                    *slot = Some(e.to_string());
                }
                stream_complete_writer.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        }
    });

    let mic_pref = app
        .try_state::<Store>()
        .and_then(|store| store.get_setting("mic_device").ok().flatten())
        .filter(|s| !s.trim().is_empty());
    spawn_mic_start(app, session_id, tee_tx, mic_pref);

    // Cue after mic is spawned so a Bluetooth profile switch cannot precede WASAPI.
    play_sound_cue(app, crate::sound::CueKind::Start);

    // Overlay last: connecting bars until this session's audio-level events.
    // Toggle mode has no key to release, so the pill gets ✗ / ✓ buttons.
    let controls = crate::hotkey::get_hotkey_mode(app) == "toggle";
    let _ = app.emit("dictation-controls", controls);
    let _ = app.emit("dictation-state", "listening");
    show_overlay_with(app, controls);

    // Auto-stop after remaining Free time (or 2 min max). Bound to session_id so a
    // previous session's sleep cannot kill a newer recording (common in toggle mode).
    let app_timeout = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(remaining_cap).await;
        let state = app_timeout.state::<PipelineState>();
        if *state.session_gen.lock().unwrap() != session_id {
            return;
        }
        if !*state.active.lock().unwrap() {
            return;
        }
        let elapsed = state
            .started_at
            .lock()
            .unwrap()
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO);
        // Prefer Instant: only auto-stop once this session has actually hit the cap.
        if elapsed + Duration::from_millis(50) < remaining_cap {
            log::warn!(
                "Ignoring stale max-length timer (session={session_id}, elapsed={:.1}s)",
                elapsed.as_secs_f64()
            );
            return;
        }
        log::info!(
            "Max recording length reached after {:.1}s (session={session_id}) — stopping",
            elapsed.as_secs_f64()
        );
        let msg = if free_time_cap {
            "Free limit: 2 minutes every 24 hours — stopping"
        } else {
            "Max length: 2 minutes — stopping"
        };
        let _ = app_timeout.emit("dictation-error", msg);
        stop_dictation(&app_timeout);
    });

    // This session's own buffer: the shared slot may already belong to a newer session.
    let session_pcm_task = session_pcm.clone();
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut final_text = String::new();
        let mut last_interim = String::new();
        while let Some(chunk) = transcript_rx.recv().await {
            if !chunk.text.trim().is_empty() && paste_still_current(&app_handle, paste_token) {
                app_handle
                    .state::<PipelineState>()
                    .heard_text
                    .store(true, Ordering::SeqCst);
            }
            let _ = app_handle.emit(
                "transcript",
                serde_json::json!({ "text": chunk.text, "is_final": chunk.is_final }),
            );
            apply_transcript_chunk(&mut final_text, &mut last_interim, &chunk.text, chunk.is_final);
        }

        // If CloseStream beat speech_final, keep the last interim so endings aren't lost.
        let mut text = final_text.trim().to_string();
        let interim = last_interim.trim();
        if !interim.is_empty() {
            text = merge_trailing_interim(&text, interim);
        }

        if *app_handle.state::<PipelineState>().discarded_paste.lock().unwrap() == paste_token {
            log::info!("Session paste={paste_token} cancelled — discarding");
            clear_session_pcm_if_current(&app_handle, paste_token);
            return;
        }

        // Bill wall-clock recording time even if this session produced no text.
        let duration_secs = {
            let secs = *app_handle
                .state::<PipelineState>()
                .last_session_secs
                .lock()
                .unwrap();
            if secs <= 0.0 {
                0
            } else {
                secs.ceil() as u64
            }
        };
        if let Some(store) = app_handle.try_state::<Store>() {
            let _ = store.record_duration(duration_secs);
        }

        // Don't flash "Enhancing…" until we know we'll actually call the LLM.
        // Paste happens after enhance — showing Enhancing while only doing local
        // cleanup/paste is confusing (text already looks "done" to the user).

        let fg = {
            let state = app_handle.state::<PipelineState>();
            let snap = state.session_fg.lock().unwrap().clone();
            snap.or_else(context::get_foreground_app)
        };
        let app_name = fg
            .as_ref()
            .map(|a| context::friendly_app_name(&a.exe))
            .unwrap_or_else(|| "Unknown".to_string());

        let mut text = text;
        let stream_incomplete = !stream_complete.load(std::sync::atomic::Ordering::SeqCst);
        if text.is_empty() || stream_incomplete {
            let pcm: Vec<i16> = session_pcm_task
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default();
            if text.is_empty() && !recovery::has_speech(&pcm) {
                // Accidental tap or silent mic — nothing worth keeping.
                clear_session_pcm_if_current(&app_handle, paste_token);
                hide_overlay_if_current(&app_handle, paste_token);
                emit_state_if_current(&app_handle, paste_token, "idle");
                return;
            }

            log::warn!("Live transcript empty or incomplete — retrying from recording");
            let retried =
                recovery::retry_transcribe(&pcm, &language_for_pipeline, &retry_keyterms).await;
            // ✗ pressed while the batch retry ran: no failed entry, toast, or auto-paste.
            if *app_handle.state::<PipelineState>().discarded_paste.lock().unwrap() == paste_token {
                clear_session_pcm_if_current(&app_handle, paste_token);
                return;
            }
            match retried {
                Ok(recovered) => {
                    log::info!("Recovered dictation via batch retry ({} chars)", recovered.len());
                    text = recovered;
                }
                Err(reason) if !text.is_empty() => {
                    log::warn!("Batch retry failed, using partial live transcript: {reason}");
                }
                Err(reason) if reason == recovery::NO_SPEECH => {
                    // Noise (cough, keyboard) with no words: treat as silence, not a failure.
                    log::info!("Batch retry heard no words — dismissing quietly");
                    clear_session_pcm_if_current(&app_handle, paste_token);
                    hide_overlay_if_current(&app_handle, paste_token);
                    emit_state_if_current(&app_handle, paste_token, "idle");
                    return;
                }
                Err(reason) => {
                    let stream_err = stt_error.lock().ok().and_then(|g| g.clone());
                    let detail = stream_err
                        .map(|e| format!("Transcription failed: {e}"))
                        .unwrap_or(reason);
                    let failed_id = recovery::save_failed(&app_handle, &app_name, &detail, &pcm);
                    clear_session_pcm_if_current(&app_handle, paste_token);
                    if paste_still_current(&app_handle, paste_token) {
                        let _ = app_handle.emit(
                            "dictation-error",
                            "Failed to transcribe — retrying in 5 seconds",
                        );
                    }
                    emit_state_if_current(&app_handle, paste_token, "error");
                    recovery::notify(
                        &app_handle,
                        "Failed to transcribe",
                        "Bad connection? Retrying in 5 seconds.",
                    );
                    if let Some(hid) = failed_id {
                        recovery::spawn_auto_retry(
                            app_handle.clone(),
                            hid,
                            fg.as_ref().map(|a| a.exe.clone()),
                        );
                    }
                    return;
                }
            }
        }

        if let Some(cmd_result) = commands::check_command(&text) {
            let pipeline_state = app_handle.state::<PipelineState>();
            let still_current = {
                let epoch = pipeline_state.paste_epoch.lock().unwrap();
                *epoch == paste_token
            };
            if !still_current {
                log::info!("Skipping command inject for stale paste={paste_token}");
                // Do not emit idle — a newer listening session may already own the UI.
                return;
            }
            let epoch_alive = || paste_still_current(&app_handle, paste_token);
            match cmd_result {
                commands::CommandResult::ScratchThat => {
                    let last = pipeline_state.last_insertion.lock().unwrap();
                    match last.as_ref() {
                        Some(ins) if insertion_is_fresh(ins) => {
                            let _ = inject::undo_insertion(ins);
                        }
                        Some(_) => log::info!("Scratch that ignored: last insertion is stale"),
                        None => log::info!("Scratch that ignored: nothing inserted yet"),
                    }
                }
                cmd @ (commands::CommandResult::Rewrite(_) | commands::CommandResult::Local(_)) => {
                    let old_text = {
                        let last = pipeline_state.last_insertion.lock().unwrap();
                        last.as_ref()
                            .filter(|ins| insertion_is_fresh(ins))
                            .map(|ins| (ins.text.clone(), ins.char_count))
                    };
                    if let Some((old, count)) = old_text {
                        let body = old.trim_end();
                        let trailing = &old[body.len()..];
                        // Rewrite first — never delete the prior paste until we have
                        // something to replace it with (LLM failure used to wipe text).
                        let produced: Result<String, String> = match &cmd {
                            commands::CommandResult::Rewrite(request) => {
                                emit_state_if_current(&app_handle, paste_token, "processing");
                                tone::rewrite_with_llm(body, request)
                                    .await
                                    .map_err(|e| e.to_string())
                            }
                            commands::CommandResult::Local(edit) => Ok(edit(body)),
                            _ => unreachable!("outer pattern only admits Rewrite/Local"),
                        };
                        match produced {
                            Ok(rewritten) => {
                                let rewritten = format!("{}{}", rewritten.trim_end(), trailing);
                                if !epoch_alive() {
                                    log::info!(
                                        "Skipping rewrite inject for stale paste={paste_token}"
                                    );
                                    return;
                                }
                                let _ = inject::undo_insertion(&LastInsertion {
                                    text: old.clone(),
                                    char_count: count,
                                    pasted_at: Instant::now(),
                                });
                                match inject::inject_text_if(&rewritten, epoch_alive) {
                                    Ok(new_ins) => {
                                        *pipeline_state.last_insertion.lock().unwrap() =
                                            Some(new_ins);
                                    }
                                    Err(e) if e.to_string().contains("cancelled") => {
                                        log::info!(
                                            "Rewrite inject cancelled (stale paste={paste_token})"
                                        );
                                    }
                                    Err(e) => {
                                        log::warn!(
                                            "Rewrite inject failed after undo, restoring original: {e}"
                                        );
                                        if let Ok(restored) =
                                            inject::inject_text_if(&old, epoch_alive)
                                        {
                                            *pipeline_state.last_insertion.lock().unwrap() =
                                                Some(restored);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("Rewrite LLM failed; leaving original text: {e}");
                                let _ = app_handle.emit(
                                    "dictation-error",
                                    "Couldn't rewrite — original text kept",
                                );
                            }
                        }
                    } else {
                        log::info!("Transform ignored: no recent insertion to edit");
                    }
                }
                commands::CommandResult::InsertText(t) => {
                    match inject::inject_text_if(&t, epoch_alive) {
                        Ok(ins) => {
                            let mut last = pipeline_state.last_insertion.lock().unwrap();
                            // Keep the dictation this extends so "make it formal" / "scratch
                            // that" after "new line" still act on it, not on the bare break.
                            let merged = match last.take() {
                                Some(prev) if insertion_is_fresh(&prev) => LastInsertion {
                                    text: format!("{}{}", prev.text, ins.text),
                                    char_count: prev.char_count + ins.char_count,
                                    pasted_at: ins.pasted_at,
                                },
                                _ => ins,
                            };
                            *last = Some(merged);
                        }
                        Err(e) if e.to_string().contains("cancelled") => {
                            log::info!(
                                "Command inject cancelled (stale paste={paste_token})"
                            );
                        }
                        Err(e) => log::warn!("Command inject failed: {e}"),
                    }
                }
            }
        } else {
            let store = app_handle.state::<Store>();
            let expanded = vocab::expand_macros(&text, &store);

            // Multilingual / non-Latin: skip English-only local heuristics that
            // can chop or mangle code-switched transcripts (e.g. Russian→English).
            let multilingual_session =
                language_for_pipeline == "multi" || tone::has_non_latin_script(&expanded);
            let corrected = if multilingual_session {
                expanded.clone()
            } else {
                tone::local_self_correct(&expanded)
            };

            // Whisper Flow–style: remember name fixes from spoken self-corrections.
            if corrected.trim() != expanded.trim() && tone::spoken_correction_applied(&expanded) {
                vocab::learn_name_corrections(&expanded, &corrected, &store);
                learn_substitutions::learn_from_edit(&expanded, &corrected, &store);
            }

            let has_llm_key = secrets::has_llm_api_key();
            let ai_enhance = store
                .get_setting("ai_enhance")
                .ok()
                .flatten()
                .map(|v| v != "false")
                .unwrap_or(true);
            let enhance_speed = tone::EnhanceSpeed::from_store(&store);

            // Short hold: paste fast with local cleanup only. Full LLM
            // enhance is reserved for longer dictations where the lag is worth it.
            // Fast skips more often; Ultra almost always runs the model.
            let session_secs = *app_handle
                .state::<PipelineState>()
                .last_session_secs
                .lock()
                .unwrap();
            let quick_session = session_secs < enhance_speed.quick_skip_secs();

            let word_count = corrected.split_whitespace().count();
            let mut enhance_ran = false;
            // Multilingual / code-switch: keep Deepgram text as-is. The English
            // Grammarly pass was compounding ASR mistakes into fluent wrong prose
            // ("build function" stayed "blood function" or got rewritten further).
            let will_call_llm = language_for_pipeline != "multi"
                && has_llm_key
                && ai_enhance
                && !quick_session;
            if will_call_llm {
                emit_state_if_current(&app_handle, paste_token, "processing");
            }

            let mut final_output = if language_for_pipeline == "multi" {
                log::info!("Skipping AI enhance for multilingual session");
                corrected
            } else if quick_session {
                log::info!(
                    "Quick session ({session_secs:.1}s) — local cleanup only, skip LLM"
                );
                enhance_ran = corrected.trim() != expanded.trim();
                corrected
            } else if has_llm_key && ai_enhance {
                let tone_name = fg
                    .as_ref()
                    .and_then(|a| tone::get_tone_for_app(a, &store))
                    .unwrap_or_else(|| "default".to_string());
                let dict_terms: Vec<String> = store
                    .get_dictionary()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|w| w.word)
                    .collect();
                match tone::enhance_dictation_ex(
                    &corrected,
                    &tone_name,
                    word_count >= enhance_speed.long_word_threshold(),
                    multilingual_session,
                    &dict_terms,
                    enhance_speed,
                )
                .await
                {
                    Ok(out) => {
                        log::info!("AI enhance ok ({} → {} chars)", corrected.len(), out.len());
                        enhance_ran = true;
                        out
                    }
                    Err(e) => {
                        log::warn!("AI enhance failed, using local correction: {e}");
                        // Local cleanup still counts when enhance is on
                        enhance_ran = corrected.trim() != expanded.trim();
                        corrected
                    }
                }
            } else {
                if !has_llm_key && ai_enhance {
                    log::info!("AI enhance on but no LLM key — local self-correct only");
                    enhance_ran = corrected.trim() != expanded.trim();
                }
                corrected
            };

            // Prefer a real sentence end over ASR's trailing comma/semicolon.
            // Skip multilingual so we don't impose English punctuation habits.
            if language_for_pipeline != "multi" && !multilingual_session {
                let tone_for_punct = fg
                    .as_ref()
                    .and_then(|a| tone::get_tone_for_app(a, &store))
                    .unwrap_or_else(|| "default".to_string());
                final_output =
                    tone::normalize_terminal_punctuation(&final_output, &tone_for_punct);
            }

            let original_for_toast = expanded.trim().to_string();
            let enhanced_for_toast = final_output.trim().to_string();
            let text_changed = original_for_toast != enhanced_for_toast;

            let trailing = store
                .get_setting("trailing_space")
                .ok()
                .flatten()
                .map(|v| v != "false")
                .unwrap_or(true);
            if trailing && !final_output.ends_with(' ') {
                final_output.push(' ');
            }

            let pipeline_state = app_handle.state::<PipelineState>();
            // If the user started a newer recording while we were enhancing, skip
            // paste so two injections don't land as "half, then half".
            let still_current = {
                let epoch = pipeline_state.paste_epoch.lock().unwrap();
                *epoch == paste_token
            };
            // ✗ pressed while the thinking wave was running (toggle mode).
            if *pipeline_state.discarded_paste.lock().unwrap() == paste_token {
                log::info!("Skipping paste for cancelled session paste={paste_token}");
                return;
            }
            if !still_current {
                log::info!(
                    "Skipping paste for stale session paste={paste_token} (newer dictation started)"
                );
                // Do not emit idle — that would hide the newer session's listening pill.
                return;
            }

            let epoch_alive = || paste_still_current(&app_handle, paste_token);
            match inject::inject_text_if(&final_output, epoch_alive) {
                Ok(ins) => {
                    if let Some(t) = pipeline_state.released_at.lock().unwrap().take() {
                        log::info!("Dictation timing: release->paste {} ms", t.elapsed().as_millis());
                    }
                    let prev = pipeline_state.pending_learn_from.lock().unwrap().take();
                    if let Some(prev_text) = prev {
                        learn_substitutions::learn_from_redictate(
                            &prev_text,
                            &final_output,
                            &store,
                        );
                    }
                    *pipeline_state.last_insertion.lock().unwrap() = Some(ins);
                }
                Err(e) if e.to_string().contains("cancelled") => {
                    log::info!(
                        "Paste cancelled mid-inject for stale session paste={paste_token}"
                    );
                    // Newer session owns the field — don't emit idle / toast.
                    return;
                }
                Err(e) => {
                    log::warn!("Paste failed: {e}");
                    // Never lose the text: leave it on the clipboard and say so.
                    let _ = inject::copy_text(&final_output);
                    let _ = app_handle.emit(
                        "dictation-error",
                        "Couldn't paste — text copied, press Ctrl+V",
                    );
                    emit_state_if_current(&app_handle, paste_token, "error");
                }
            }

            // Grammarly-like toast before WAV I/O so it isn't delayed by Remake save.
            let show_enhance_toast = ai_enhance && enhance_ran && text_changed;
            if show_enhance_toast {
                let _ = app_handle.emit(
                    "dictation-enhanced",
                    serde_json::json!({
                        "original": original_for_toast,
                        "enhanced": enhanced_for_toast,
                    }),
                );
            } else {
                // Paste is done — park instantly. Don't linger on "Done" while history saves.
                hide_overlay_if_current(&app_handle, paste_token);
            }
            emit_state_if_current(&app_handle, paste_token, "idle");

            let history_text = final_output.trim_end().to_string();
            if let Ok(hid) = store.add_history(&history_text, &app_name) {
                // Persist local Remake WAV (newest 10 only).
                let pcm = session_pcm_task
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or_default();
                clear_session_pcm_if_current(&app_handle, paste_token);
                if !pcm.is_empty() {
                    let path = recording::wav_path_for_history_id(hid);
                    if let Err(e) = recording::write_wav_i16(&path, &pcm, WAV_SAMPLE_RATE) {
                        log::warn!("Failed to save Remake recording: {e}");
                    } else {
                        let path_str = path.to_string_lossy().into_owned();
                        let _ = store.set_history_recording_path(hid, Some(&path_str));
                        if let Err(e) = store.prune_recordings(MAX_REMAKE_RECORDINGS) {
                            log::warn!("prune_recordings: {e}");
                        }
                    }
                }

                let _ = app_handle.emit(
                    "history-added",
                    serde_json::json!({
                        "id": hid,
                        "text": history_text,
                        "app_name": app_name,
                    }),
                );
            }
            return;
        }

        hide_overlay_if_current(&app_handle, paste_token);
        emit_state_if_current(&app_handle, paste_token, "idle");
    });
}

/// Backspace-based undo/rewrite is only safe while the cursor is still right after our paste;
/// after this long the user has likely clicked or typed elsewhere.
const LAST_INSERTION_TTL: std::time::Duration = std::time::Duration::from_secs(300);

fn insertion_is_fresh(ins: &LastInsertion) -> bool {
    ins.pasted_at.elapsed() < LAST_INSERTION_TTL
}

/// True while this paste token is still the newest recording start (even if ✗ discarded it).
fn epoch_current(app: &tauri::AppHandle, paste_token: u64) -> bool {
    *app.state::<PipelineState>().paste_epoch.lock().unwrap() == paste_token
}

/// True while this session may still paste or drive the overlay: newest, and not ✗-cancelled.
fn paste_still_current(app: &tauri::AppHandle, paste_token: u64) -> bool {
    epoch_current(app, paste_token)
        && *app.state::<PipelineState>().discarded_paste.lock().unwrap() != paste_token
}

/// Drop the shared Remake buffer only if a newer session hasn't replaced it.
fn clear_session_pcm_if_current(app: &tauri::AppHandle, paste_token: u64) {
    if epoch_current(app, paste_token) {
        *app.state::<PipelineState>().session_pcm.lock().unwrap() = None;
    }
}

/// Paste epoch now, so a later auto-retry can tell whether the user dictated again.
pub(crate) fn current_paste_epoch(app: &tauri::AppHandle) -> u64 {
    *app.state::<PipelineState>().paste_epoch.lock().unwrap()
}

/// A dictation is recording, or one started after `epoch`: an auto-retry must not paste into it.
pub(crate) fn dictation_moved_on(app: &tauri::AppHandle, epoch: u64) -> bool {
    let state = app.state::<PipelineState>();
    let active = *state.active.lock().unwrap();
    active || *state.paste_epoch.lock().unwrap() != epoch
}

/// Only the current paste epoch may drive overlay state — prevents a finishing
/// session from flipping a newer listening UI to idle (hotkey "stuck" feel).
fn emit_state_if_current(app: &tauri::AppHandle, paste_token: u64, state: &str) {
    if paste_still_current(app, paste_token) {
        let _ = app.emit("dictation-state", state);
    } else {
        log::debug!("Skip stale or cancelled dictation-state '{state}' paste={paste_token}");
    }
}

fn play_sound_cue(app: &tauri::AppHandle, kind: crate::sound::CueKind) {
    let Some(store) = app.try_state::<Store>() else {
        return;
    };
    let enabled = store
        .get_setting("sound_cue")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);
    if !enabled {
        return;
    }
    let vol = crate::sound::volume_from_setting(
        store
            .get_setting("sound_cue_volume")
            .ok()
            .flatten()
            .as_deref(),
    );
    crate::sound::play_cue(kind, vol);
}

pub fn stop_dictation(app: &tauri::AppHandle) {
    let state = app.state::<PipelineState>();
    let mut active = state.active.lock().unwrap();
    if !*active {
        return;
    }
    *active = false;
    drop(active);
    play_sound_cue(app, crate::sound::CueKind::Stop);

    let elapsed_secs = state
        .started_at
        .lock()
        .unwrap()
        .map(|t| t.elapsed().as_secs_f64())
        .unwrap_or(0.0);
    *state.started_at.lock().unwrap() = None;
    *state.last_session_secs.lock().unwrap() = elapsed_secs;
    let stop_gen = {
        let mut gen = state.session_gen.lock().unwrap();
        *gen = gen.wrapping_add(1);
        *gen
    };

    // Keep capturing after hotkey-up: people release during the last syllable. A fixed
    // wait either chops endings or makes every take pay for the worst case, so stop as
    // soon as the mic has been quiet for TRAIL_QUIET_MS (after TRAIL_MIN_MS), capped.
    let trail_max_ms: u64 = if elapsed_secs < 5.0 { 450 } else { 550 };
    *state.released_at.lock().unwrap() = Some(Instant::now());

    let stop_tx = state.stop_tx.lock().unwrap().take();
    let app_trail = app.clone();
    tauri::async_runtime::spawn(async move {
        let released = Instant::now();
        loop {
            let waited = released.elapsed().as_millis() as u64;
            if waited >= trail_max_ms
                || (waited >= TRAIL_MIN_MS && crate::audio::silent_for(TRAIL_QUIET_MS))
            {
                log::info!("Trail ended {waited}ms after release");
                break;
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
        let state = app_trail.state::<PipelineState>();
        // Don't tear down a newer session that started during the trail.
        if !*state.active.lock().unwrap() && *state.session_gen.lock().unwrap() == stop_gen {
            state.audio_capture.lock().unwrap().stop();
        }
        if let Some(tx) = stop_tx {
            let _ = tx.try_send(());
        }
    });

    log::info!("Dictation stopped after {elapsed_secs:.1}s (trail up to {trail_max_ms}ms)");
    let paste_token = *state.paste_epoch.lock().unwrap();
    // Nothing said: no voice on the mic and no words from Deepgram. Vanish now
    // instead of spinning the thinking wave while the stream drains.
    let heard_voice = state
        .session_pcm
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|a| a.lock().ok().map(|g| recovery::heard_voice(&g)))
        .unwrap_or(false);
    if !heard_voice && !state.heard_text.load(Ordering::SeqCst) {
        // Hide now, but let the stream decide: a soft word Deepgram does catch still pastes.
        log::info!("No speech detected — dismissing overlay");
        hide_overlay_if_current(app, paste_token);
        emit_state_if_current(app, paste_token, "idle");
        return;
    }
    // Switch to the thinking wave while Deepgram drains + enhance runs — frozen
    // mic bars after release look stuck when transcription takes a moment.
    emit_state_if_current(app, paste_token, "processing");
}

/// Overlay ✗ (toggle mode): stop recording and throw the take away.
pub fn cancel_dictation(app: &tauri::AppHandle) {
    let paste_token = *app.state::<PipelineState>().paste_epoch.lock().unwrap();
    stop_dictation(app);
    discard_session(app, paste_token);
}

/// Hand focus back to the window the user was dictating into (overlay clicks
/// can take it), so the paste lands in the right place.
pub fn refocus_session_window(app: &tauri::AppHandle) {
    let hwnd = *app.state::<PipelineState>().session_hwnd.lock().unwrap();
    inject::focus_window(hwnd);
}

fn discard_session(app: &tauri::AppHandle, paste_token: u64) {
    let state = app.state::<PipelineState>();
    if *state.paste_epoch.lock().unwrap() != paste_token {
        return;
    }
    *state.discarded_paste.lock().unwrap() = paste_token;
    hide_overlay_fast(app);
    let _ = app.emit("dictation-state", "idle");
}

fn invalidate_session(state: &PipelineState) {
    *state.active.lock().unwrap() = false;
    *state.started_at.lock().unwrap() = None;
    let mut gen = state.session_gen.lock().unwrap();
    *gen = gen.wrapping_add(1);
}

/// Fold one Deepgram Result into the session transcript.
///
/// Nova-3 emits multiple `is_final` segments during a long hold (and a new
/// utterance after each `speech_final` pause). Each final is a *delta* for that
/// audio window — append them. Interims replace the in-progress window.
///
/// When Deepgram finalizes a *prefix* of the current interim (common ~3s
/// windows), keep the leftover tail so a dropped follow-up message cannot
/// wipe the rest of the utterance.
fn apply_transcript_chunk(
    final_text: &mut String,
    last_interim: &mut String,
    chunk_text: &str,
    is_final: bool,
) {
    let t = chunk_text.trim();
    if t.is_empty() {
        return;
    }
    if !is_final {
        *last_interim = t.to_string();
        return;
    }

    let accumulated = final_text.trim().to_string();
    // Some results repeat everything so far instead of a delta — replace, don't duplicate.
    if is_text_prefix(t, &accumulated) && t.len() >= accumulated.len() {
        *final_text = format!("{t} ");
        last_interim.clear();
        return;
    }

    let interim = last_interim.trim().to_string();
    if is_text_prefix(&interim, t) {
        let rest = if interim.len() > t.len() {
            interim[t.len()..].trim_start().to_string()
        } else {
            String::new()
        };
        if !final_text.is_empty() && !final_text.ends_with(' ') {
            final_text.push(' ');
        }
        final_text.push_str(t);
        final_text.push(' ');
        *last_interim = rest;
        return;
    }

    if !final_text.is_empty() && !final_text.ends_with(' ') {
        final_text.push(' ');
    }
    final_text.push_str(t);
    final_text.push(' ');
    last_interim.clear();
}

/// True when `full` is `prefix`, or `prefix` plus a non-alphanumeric break
/// (space / punctuation). Avoids "the" matching "there".
fn is_text_prefix(full: &str, prefix: &str) -> bool {
    if prefix.is_empty() || !full.starts_with(prefix) {
        return false;
    }
    if full.len() == prefix.len() {
        return true;
    }
    full[prefix.len()..].starts_with(|c: char| !c.is_alphanumeric())
}

/// Merge a trailing interim onto finalized text when CloseStream races speech_final.
/// Prefer the longer superseding interim, or overlap-aware append so we don't
/// duplicate ("Daniel walked" + "walked out" → "Daniel walked out").
/// Never replace a longer accumulated transcript with a shorter last-interim.
fn merge_trailing_interim(final_text: &str, interim: &str) -> String {
    let text = final_text.trim();
    let interim = interim.trim();
    if interim.is_empty() {
        return text.to_string();
    }
    if text.is_empty() {
        return interim.to_string();
    }
    if text == interim || text.ends_with(interim) {
        return text.to_string();
    }
    // Interim is a longer version of the whole utterance so far.
    if is_text_prefix(interim, text) {
        return interim.to_string();
    }

    let t_words: Vec<&str> = text.split_whitespace().collect();
    let i_words: Vec<&str> = interim.split_whitespace().collect();
    let max_overlap = t_words.len().min(i_words.len());
    for overlap in (1..=max_overlap).rev() {
        if t_words[t_words.len() - overlap..] == i_words[..overlap] {
            let mut out = t_words[..t_words.len() - overlap].to_vec();
            out.extend_from_slice(&i_words);
            return out.join(" ");
        }
    }

    format!("{text} {interim}")
}

#[cfg(test)]
mod interim_merge_tests {
    use super::{apply_transcript_chunk, merge_trailing_interim};

    fn fold(chunks: &[(&str, bool)]) -> String {
        let mut final_text = String::new();
        let mut last_interim = String::new();
        for (text, is_final) in chunks {
            apply_transcript_chunk(&mut final_text, &mut last_interim, text, *is_final);
        }
        let mut text = final_text.trim().to_string();
        let interim = last_interim.trim();
        if !interim.is_empty() {
            text = merge_trailing_interim(&text, interim);
        }
        text
    }

    #[test]
    fn keeps_text_when_interim_already_included() {
        assert_eq!(
            merge_trailing_interim("Daniel walked out", "out"),
            "Daniel walked out"
        );
    }

    #[test]
    fn appends_new_interim_tail() {
        assert_eq!(
            merge_trailing_interim("Daniel walked", "out"),
            "Daniel walked out"
        );
    }

    #[test]
    fn overlaps_shared_words() {
        assert_eq!(
            merge_trailing_interim("Daniel walked", "walked out"),
            "Daniel walked out"
        );
    }

    #[test]
    fn prefers_longer_superseding_interim() {
        assert_eq!(
            merge_trailing_interim("Daniel walked", "Daniel walked out"),
            "Daniel walked out"
        );
    }

    #[test]
    fn does_not_drop_first_utterance_for_new_interim() {
        let merged = merge_trailing_interim(
            "The first ten seconds were about the budget and hiring plan.",
            "And then I kept talking about the timeline for another ten seconds",
        );
        assert!(merged.contains("first ten seconds"));
        assert!(merged.contains("another ten seconds"));
    }

    #[test]
    fn contains_does_not_drop_a_new_utterance() {
        // "the budget" appears in the first utterance; that must not discard the rest.
        let merged = merge_trailing_interim(
            "We should discuss the budget today",
            "the budget for Q3 looks tight",
        );
        assert!(merged.contains("discuss the budget"));
        assert!(merged.contains("Q3 looks tight"));
    }

    #[test]
    fn accumulates_multiple_finals_across_a_pause() {
        let text = fold(&[
            ("yeah so my credit card number is two two two two three", false),
            ("yeah so my credit card number is two two", true),
            ("two two three three three three", false),
            ("two two three three three three", true),
            ("and then I kept talking after a pause", false),
            ("and then I kept talking after a pause about the launch date", true),
        ]);
        assert!(text.contains("credit card number"));
        assert!(text.contains("three three three three"));
        assert!(text.contains("launch date"));
    }

    #[test]
    fn keeps_interim_tail_when_final_is_a_prefix() {
        let mut final_text = String::new();
        let mut last_interim = String::new();
        apply_transcript_chunk(
            &mut final_text,
            &mut last_interim,
            "hello this is a long first utterance that continues",
            false,
        );
        apply_transcript_chunk(
            &mut final_text,
            &mut last_interim,
            "hello this is a long first utterance",
            true,
        );
        assert!(final_text.contains("long first utterance"));
        assert!(
            last_interim.contains("that continues"),
            "prefix final discarded the interim tail: {last_interim:?}"
        );
    }

    #[test]
    fn cumulative_final_does_not_duplicate() {
        let text = fold(&[
            ("hello there", true),
            ("hello there how are you today", true),
        ]);
        let hello = text.matches("hello there").count();
        assert_eq!(hello, 1, "duplicated cumulative finals: {text}");
        assert!(text.contains("how are you today"));
    }

    #[test]
    fn trailing_interim_after_speech_final_appends_next_utterance() {
        // First utterance finalized; second still interim when CloseStream hits.
        let text = fold(&[
            ("The first ten seconds were about the budget.", true),
            ("And then I kept talking about hiring after the pause", false),
        ]);
        assert!(text.contains("first ten seconds"));
        assert!(text.contains("after the pause"));
    }
}

#[cfg(test)]
mod local_step_bench {
    use super::*;

    /// Times the synchronous post-transcript steps against the real local DB (read-only calls).
    /// cargo test --release --bin maxspeech local_step_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn time_local_steps() {
        let store = Store::new().expect("store");
        let text = "So I was thinking that we should probably move the meeting to Thursday, um, actually no wait, Friday, because Sarah is out and the numbers are not ready yet. We can also look at the quarterly report and I mean the budget forecast for the next three months, and then send a summary to the whole team by end of day.";
        let app = context::ForegroundApp { exe: "chrome.exe".into(), title: "Gmail - Inbox - Google Chrome".into() };
        let n = 50u32;
        let time = |label: &str, f: &dyn Fn()| {
            f(); // warm
            let t = Instant::now();
            for _ in 0..n {
                f();
            }
            println!("STEP {label}: {:.3} ms/call", t.elapsed().as_secs_f64() * 1000.0 / n as f64);
        };
        println!("STEP app_profiles rows: {}", store.get_app_profiles().unwrap().len());
        let dict = store.get_dictionary().unwrap();
        println!("STEP dictionary rows: {}", dict.len());
        println!("STEP dictionary: {:?}", dict.iter().map(|d| d.word.clone()).collect::<Vec<_>>());
        println!("STEP macros rows: {}", store.get_macros().unwrap().len());
        let subs = store.get_substitutions().unwrap();
        println!("STEP substitutions rows: {}", subs.len());
        println!("STEP substitutions: {:?}", subs);
        for sample in [
            "so i think if we go then it is right and also do it because what you said is sure not the next thing",
            "there are people in the rooms and it failed after seconds so look at the side of the desktop",
        ] {
            println!("STEP EXPAND in : {sample}");
            println!("STEP EXPAND out: {}", vocab::expand_macros(sample, &store));
        }
        // The SQL-filtered lookup must pick exactly what the old load-everything loop picked.
        let reference = |app: &context::ForegroundApp| -> Option<String> {
            let exe = app.exe.to_lowercase();
            let title = app.title.to_lowercase();
            let mut best: Option<(usize, String)> = None;
            for p in store.get_app_profiles().unwrap() {
                if !p.enabled { continue; }
                let pe = p.exe_pattern.to_lowercase();
                if pe.is_empty() || !exe.contains(&pe) { continue; }
                let pt = p.title_pattern.to_lowercase();
                if !(pt.is_empty() || title.contains(&pt)) { continue; }
                let score = if pt.is_empty() { 0 } else { 1_000 + pt.len() };
                match best {
                    Some((b, _)) if score <= b => {}
                    _ => best = Some((score, p.tone.clone())),
                }
            }
            best.map(|(_, t)| t)
        };
        for (exe, title) in [
            ("chrome.exe", "Gmail - Inbox - Google Chrome"),
            ("chrome.exe", "ChatGPT - Google Chrome"),
            ("Code.exe", "main.rs - Visual Studio Code"),
            ("slack.exe", "general - Slack"),
            ("WINWORD.EXE", "Document1 - Word"),
            ("notepad.exe", "Untitled - Notepad"),
            ("claude.exe", "Claude"),
            ("unknownapp.exe", "x"),
        ] {
            let app = context::ForegroundApp { exe: exe.into(), title: title.into() };
            assert_eq!(tone::get_tone_for_app(&app, &store), reference(&app), "{exe} / {title}");
        }
        println!("STEP tone lookup matches reference for all sample apps");
        time("expand_macros", &|| {
            let _ = vocab::expand_macros(text, &store);
        });
        time("local_self_correct", &|| {
            let _ = tone::local_self_correct(text);
        });
        time("get_tone_for_app", &|| {
            let _ = tone::get_tone_for_app(&app, &store);
        });
        time("normalize_terminal_punctuation", &|| {
            let _ = tone::normalize_terminal_punctuation(text, "default");
        });
        time("get_plan_status", &|| {
            let _ = store.get_plan_status();
        });
        time("get_setting x4", &|| {
            for k in ["ai_enhance", "trailing_space", "enhance_speed", "sound_cue_volume"] {
                let _ = store.get_setting(k);
            }
        });
    }
}
