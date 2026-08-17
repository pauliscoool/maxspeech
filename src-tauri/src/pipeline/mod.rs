pub mod commands;
pub mod tone;
pub mod vocab;

use crate::audio::AudioCapture;
use crate::context;
use crate::inject::{self, LastInsertion};
use crate::recording::{self, MAX_REMAKE_RECORDINGS, WAV_SAMPLE_RATE};
use crate::secrets;
use crate::store::Store;
use crate::stt::deepgram::{self, DeepgramConfig, TranscriptChunk};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use tokio::sync::mpsc;

/// Hard cap for a single push-to-talk session.
const MAX_RECORDING: Duration = Duration::from_secs(120);

pub struct PipelineState {
    pub active: Mutex<bool>,
    pub last_insertion: Mutex<Option<LastInsertion>>,
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
    paste_epoch: Mutex<u64>,
    /// 16 kHz mono PCM for the active session (Remake cache).
    session_pcm: Mutex<Option<Arc<Mutex<Vec<i16>>>>>,
    /// Foreground app captured at hotkey-down (tone + history must use this, not
    /// whatever is focused after enhance finishes).
    session_fg: Mutex<Option<context::ForegroundApp>>,
}

impl Default for PipelineState {
    fn default() -> Self {
        Self {
            active: Mutex::new(false),
            last_insertion: Mutex::new(None),
            stop_tx: Mutex::new(None),
            audio_capture: Mutex::new(AudioCapture::new()),
            started_at: Mutex::new(None),
            last_session_secs: Mutex::new(0.0),
            session_gen: Mutex::new(0),
            paste_epoch: Mutex::new(0),
            session_pcm: Mutex::new(None),
            session_fg: Mutex::new(None),
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
    // Hotkey path runs on a worker thread — WebView2/DWM clears only stick on the
    // UI thread. Queue there; fall back to inline if scheduling fails.
    let app2 = app.clone();
    if app.run_on_main_thread(move || show_overlay_fast_inner(&app2)).is_err() {
        show_overlay_fast_inner(app);
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
    let current = *app.state::<PipelineState>().paste_epoch.lock().unwrap();
    if current != paste_token {
        return;
    }
    hide_overlay_fast(app);
}

fn show_overlay_fast_inner(app: &tauri::AppHandle) {
    use std::sync::OnceLock;

    /// Physical bottom-center slot, resolved once. Idle parks the same-sized
    /// HWND off-screen, so the hotkey only has to clip (off-screen) and move.
    static SLOT: OnceLock<(i32, i32)> = OnceLock::new();

    // Only create a WebView2 here if startup pre-warm missed. Callers start
    // mic + Deepgram *before* this so a cold overlay cannot steal the first seconds.
    let cold = app.get_webview_window("overlay").is_none();
    crate::ensure_overlay_window(app);
    if let Some(w) = app.get_webview_window("overlay") {
        let (x, y) = *SLOT.get_or_init(|| {
            if let Ok(Some(monitor)) = w.current_monitor() {
                let scale = monitor.scale_factor();
                let size = monitor.size();
                let origin = monitor.position();
                let pill_w = (148.0 * scale).round() as i32;
                let pill_h = (36.0 * scale).round() as i32;
                let margin = (48.0 * scale).round() as i32;
                (
                    origin.x + (size.width as i32 - pill_w) / 2,
                    origin.y + size.height as i32 - pill_h - margin,
                )
            } else {
                (873, 996)
            }
        });
        // Clip while still parked, then one SetWindowPos — never reveal a
        // rectangular HWND for a frame.
        crate::overlay_win::reveal_listening(&w, x, y);
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
        }
    });
}

fn is_signed_in(store: &Store) -> bool {
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

    // Weekly word limit: hard stop when plan quota is exhausted (Mon 00:00 UTC reset).
    let plan_status = app
        .try_state::<Store>()
        .and_then(|store| store.get_plan_status().ok());
    if let Some(ref status) = plan_status {
        if !status.can_dictate {
            let _ = app.emit("dictation-limit", true);
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

    log::info!(
        "Dictation started session={session_id} paste={paste_token} (max {}s wall-clock)",
        MAX_RECORDING.as_secs()
    );

    let keywords: Vec<String> = app
        .try_state::<Store>()
        .and_then(|store| store.get_dictionary().ok())
        .unwrap_or_default()
        .into_iter()
        .map(|w| w.word)
        .collect();

    let plan_tier = plan_status
        .map(|s| s.tier)
        .unwrap_or(crate::plan::PlanTier::Free);
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
            if audio_tx.send(chunk).is_err() {
                break;
            }
        }
    });

    // Deepgram TLS/WS and WASAPI must start immediately — overlapping overlay
    // paint / WebView2 creation. stream_audio drains PCM during the handshake
    // so the first seconds are not dropped.
    let app_for_stt = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = deepgram::stream_audio(config, audio_rx, transcript_tx, stop_rx).await {
            log::error!("Deepgram stream error: {e}");
            let _ = app_for_stt.emit("dictation-error", format!("STT error: {e}"));
            let _ = app_for_stt.emit("dictation-state", "error");
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
    let _ = app.emit("dictation-state", "listening");
    show_overlay_fast(app);

    // Auto-stop after true wall-clock MAX_RECORDING. Bound to session_id so a
    // previous session's sleep cannot kill a newer recording (common in toggle mode).
    let app_timeout = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(MAX_RECORDING).await;
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
        if elapsed + Duration::from_millis(50) < MAX_RECORDING {
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
        let _ = app_timeout.emit("dictation-error", "Max length: 2 minutes — stopping");
        stop_dictation(&app_timeout);
    });

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut final_text = String::new();
        let mut last_interim = String::new();
        while let Some(chunk) = transcript_rx.recv().await {
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
        if text.is_empty() {
            {
                let state = app_handle.state::<PipelineState>();
                *state.session_pcm.lock().unwrap() = None;
            }
            hide_overlay_if_current(&app_handle, paste_token);
            emit_state_if_current(&app_handle, paste_token, "idle");
            return;
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
            match cmd_result {
                commands::CommandResult::ScratchThat => {
                    let last = pipeline_state.last_insertion.lock().unwrap();
                    if let Some(ins) = last.as_ref() {
                        let _ = inject::undo_insertion(ins);
                    }
                }
                commands::CommandResult::Rewrite(instruction) => {
                    let old_text = {
                        let last = pipeline_state.last_insertion.lock().unwrap();
                        last.as_ref().map(|ins| (ins.text.clone(), ins.char_count))
                    };
                    if let Some((old, count)) = old_text {
                        // Rewrite first — never delete the prior paste until we have
                        // something to replace it with (LLM failure used to wipe text).
                        match tone::rewrite_with_llm(&old, &instruction).await {
                            Ok(rewritten) => {
                                let _ = inject::undo_insertion(&LastInsertion {
                                    text: old.clone(),
                                    char_count: count,
                                });
                                match inject::inject_text(&rewritten) {
                                    Ok(new_ins) => {
                                        *pipeline_state.last_insertion.lock().unwrap() =
                                            Some(new_ins);
                                    }
                                    Err(e) => {
                                        log::warn!(
                                            "Rewrite inject failed after undo, restoring original: {e}"
                                        );
                                        if let Ok(restored) = inject::inject_text(&old) {
                                            *pipeline_state.last_insertion.lock().unwrap() =
                                                Some(restored);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("Rewrite LLM failed; leaving original text: {e}");
                            }
                        }
                    }
                }
                commands::CommandResult::InsertText(t) => {
                    if let Ok(ins) = inject::inject_text(&t) {
                        *pipeline_state.last_insertion.lock().unwrap() = Some(ins);
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
            if corrected.trim() != expanded.trim() {
                vocab::learn_name_corrections(&expanded, &corrected, &store);
            }

            let has_llm_key = secrets::has_llm_api_key();
            let ai_enhance = store
                .get_setting("ai_enhance")
                .ok()
                .flatten()
                .map(|v| v != "false")
                .unwrap_or(true);

            // Short hold (<5s): paste fast with local cleanup only. Full LLM
            // enhance is reserved for longer dictations where the lag is worth it.
            let session_secs = *app_handle
                .state::<PipelineState>()
                .last_session_secs
                .lock()
                .unwrap();
            let quick_session = session_secs < 5.0;

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
                    word_count >= 40,
                    multilingual_session,
                    &dict_terms,
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
            if !still_current {
                log::info!(
                    "Skipping paste for stale session paste={paste_token} (newer dictation started)"
                );
                // Do not emit idle — that would hide the newer session's listening pill.
                return;
            }

            if let Ok(ins) = inject::inject_text(&final_output) {
                *pipeline_state.last_insertion.lock().unwrap() = Some(ins);
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
                let pcm = {
                    let state = app_handle.state::<PipelineState>();
                    let taken = state.session_pcm.lock().unwrap().take();
                    match taken {
                        Some(arc) => arc.lock().map(|g| g.clone()).unwrap_or_default(),
                        None => Vec::new(),
                    }
                };
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

/// Only the current paste epoch may drive overlay state — prevents a finishing
/// session from flipping a newer listening UI to idle (hotkey "stuck" feel).
fn emit_state_if_current(app: &tauri::AppHandle, paste_token: u64, state: &str) {
    let pipeline = app.state::<PipelineState>();
    let current = *pipeline.paste_epoch.lock().unwrap();
    if current == paste_token {
        let _ = app.emit("dictation-state", state);
    } else {
        log::debug!("Skip stale dictation-state '{state}' paste={paste_token} current={current}");
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

    // Trail keeps the mic open past Deepgram endpointing so the last syllable
    // finalizes. Short holds stay snappy but need enough tail for soft endings
    // (40ms was dropping the last word ~1/5–1/10 of the time).
    let trail_ms: u64 = if elapsed_secs < 5.0 { 140 } else { 280 };

    let stop_tx = state.stop_tx.lock().unwrap().take();
    let app_trail = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(trail_ms)).await;
        let state = app_trail.state::<PipelineState>();
        // Don't tear down a newer session that started during the trail.
        if !*state.active.lock().unwrap() && *state.session_gen.lock().unwrap() == stop_gen {
            state.audio_capture.lock().unwrap().stop();
        }
        if let Some(tx) = stop_tx {
            let _ = tx.try_send(());
        }
    });

    log::info!("Dictation stopped after {elapsed_secs:.1}s ({trail_ms}ms trail)");
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
