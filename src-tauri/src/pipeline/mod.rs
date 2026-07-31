pub mod commands;
pub mod tone;
pub mod vocab;

use crate::audio::AudioCapture;
use crate::context;
use crate::inject::{self, LastInsertion};
use crate::secrets;
use crate::store::Store;
use crate::stt::deepgram::{self, DeepgramConfig, TranscriptChunk};
use std::sync::Mutex;
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
    /// Bumped on each start/stop so orphaned max-length timers cannot kill a newer session.
    session_gen: Mutex<u64>,
}

impl Default for PipelineState {
    fn default() -> Self {
        Self {
            active: Mutex::new(false),
            last_insertion: Mutex::new(None),
            stop_tx: Mutex::new(None),
            audio_capture: Mutex::new(AudioCapture::new()),
            started_at: Mutex::new(None),
            session_gen: Mutex::new(0),
        }
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

    // Weekly word limit: hard stop when plan quota is exhausted (Mon 00:00 UTC reset).
    if let Some(store) = app.try_state::<Store>() {
        if let Ok(status) = store.get_plan_status() {
            if !status.can_dictate {
                let msg = match status.weekly_limit {
                    Some(lim) => format!(
                        "Weekly word limit reached ({lim} words). Upgrade in Settings or wait until Monday."
                    ),
                    None => "Weekly word limit reached. Check usage in Settings.".to_string(),
                };
                let _ = app.emit("dictation-error", msg);
                let _ = app.emit("dictation-state", "error");
                *state.active.lock().unwrap() = false;
                return;
            }
        }
    }

    let session_id = {
        let mut gen = state.session_gen.lock().unwrap();
        *gen = gen.wrapping_add(1);
        *gen
    };
    *state.started_at.lock().unwrap() = Some(Instant::now());

    let _ = app.emit("dictation-state", "listening");
    if let Some(w) = app.get_webview_window("overlay") {
        // Kill WebView2's default white fill before the window becomes visible.
        let _ = w.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));
        let _ = w.set_shadow(false);
        let _ = w.show();
        let _ = w.set_always_on_top(true);
    }
    log::info!(
        "Dictation started session={session_id} (max {}s wall-clock)",
        MAX_RECORDING.as_secs()
    );

    let keywords: Vec<String> = app
        .try_state::<Store>()
        .and_then(|store| store.get_dictionary().ok())
        .unwrap_or_default()
        .into_iter()
        .map(|w| w.word)
        .collect();

    // Empty api_key → stream_audio tries user keyring key, then app fallback.
    let config = DeepgramConfig {
        api_key: String::new(),
        keywords,
        ..Default::default()
    };

    let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<i16>>();
    let (transcript_tx, mut transcript_rx) = mpsc::unbounded_channel::<TranscriptChunk>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>(1);

    *state.stop_tx.lock().unwrap() = Some(stop_tx);

    {
        let mut capture = state.audio_capture.lock().unwrap();
        if let Err(e) = capture.start(audio_tx, app.clone()) {
            log::error!("Failed to start audio capture: {e}");
            let _ = app.emit("dictation-error", format!("Mic error: {e}"));
            let _ = app.emit("dictation-state", "error");
            invalidate_session(&state);
            return;
        }
    }

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

    let app_for_stt = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = deepgram::stream_audio(config, audio_rx, transcript_tx, stop_rx).await {
            log::error!("Deepgram stream error: {e}");
            let _ = app_for_stt.emit("dictation-error", format!("STT error: {e}"));
        }
    });

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut final_text = String::new();
        while let Some(chunk) = transcript_rx.recv().await {
            let _ = app_handle.emit(
                "transcript",
                serde_json::json!({ "text": chunk.text, "is_final": chunk.is_final }),
            );
            if chunk.is_final {
                final_text.push_str(&chunk.text);
                final_text.push(' ');
            }
        }

        let text = final_text.trim().to_string();
        if text.is_empty() {
            let _ = app_handle.emit("dictation-state", "idle");
            return;
        }

        let _ = app_handle.emit("dictation-state", "processing");

        let fg = context::get_foreground_app();
        let app_name = fg
            .as_ref()
            .map(|a| context::friendly_app_name(&a.exe))
            .unwrap_or_else(|| "Unknown".to_string());

        if let Some(cmd_result) = commands::check_command(&text) {
            let pipeline_state = app_handle.state::<PipelineState>();
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
                        let _ = inject::undo_insertion(&LastInsertion {
                            text: old.clone(),
                            char_count: count,
                        });
                        if let Ok(rewritten) =
                            tone::rewrite_with_llm(&old, &instruction).await
                        {
                            if let Ok(new_ins) = inject::inject_text(&rewritten) {
                                *pipeline_state.last_insertion.lock().unwrap() = Some(new_ins);
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

            // Always apply local self-correction (works without an LLM key)
            let corrected = tone::local_self_correct(&expanded);

            let has_llm_key = secrets::has_llm_api_key();
            let ai_enhance = store
                .get_setting("ai_enhance")
                .ok()
                .flatten()
                .map(|v| v != "false")
                .unwrap_or(true);

            let word_count = corrected.split_whitespace().count();
            let mut enhance_ran = false;
            let mut final_output = if has_llm_key && ai_enhance {
                let tone_name = fg
                    .as_ref()
                    .and_then(|a| tone::get_tone_for_app(a, &store))
                    .unwrap_or_else(|| "default".to_string());
                match tone::enhance_dictation(&corrected, &tone_name, word_count >= 40).await {
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
            if let Ok(ins) = inject::inject_text(&final_output) {
                *pipeline_state.last_insertion.lock().unwrap() = Some(ins);
            }

            let history_text = final_output.trim_end().to_string();
            if let Ok(hid) = store.add_history(&history_text, &app_name) {
                let _ = app_handle.emit(
                    "history-added",
                    serde_json::json!({
                        "id": hid,
                        "text": history_text,
                        "app_name": app_name,
                    }),
                );
            }

            // Grammarly-like toast: only when enhance path ran and text actually changed
            let show_enhance_toast = ai_enhance && enhance_ran && text_changed;
            if show_enhance_toast {
                let _ = app_handle.emit(
                    "dictation-enhanced",
                    serde_json::json!({
                        "original": original_for_toast,
                        "enhanced": enhanced_for_toast,
                    }),
                );
            }

            let _ = app_handle.emit("dictation-state", "done");
            let dismiss_ms = if show_enhance_toast { 3200 } else { 2000 };
            tokio::time::sleep(tokio::time::Duration::from_millis(dismiss_ms)).await;
            let _ = app_handle.emit("dictation-state", "idle");
            return;
        }

        let _ = app_handle.emit("dictation-state", "done");
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let _ = app_handle.emit("dictation-state", "idle");
    });
}

pub fn stop_dictation(app: &tauri::AppHandle) {
    let state = app.state::<PipelineState>();
    let mut active = state.active.lock().unwrap();
    if !*active {
        return;
    }
    *active = false;
    drop(active);

    let elapsed = state
        .started_at
        .lock()
        .unwrap()
        .map(|t| t.elapsed().as_secs_f64());
    *state.started_at.lock().unwrap() = None;
    {
        let mut gen = state.session_gen.lock().unwrap();
        *gen = gen.wrapping_add(1);
    }

    state.audio_capture.lock().unwrap().stop();

    if let Some(tx) = state.stop_tx.lock().unwrap().take() {
        let _ = tx.try_send(());
    }

    if let Some(secs) = elapsed {
        log::info!("Dictation stopped after {secs:.1}s");
    } else {
        log::info!("Dictation stopped");
    }
}

fn invalidate_session(state: &PipelineState) {
    *state.active.lock().unwrap() = false;
    *state.started_at.lock().unwrap() = None;
    let mut gen = state.session_gen.lock().unwrap();
    *gen = gen.wrapping_add(1);
}
