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
use tauri::{Emitter, Manager};
use tokio::sync::mpsc;

pub struct PipelineState {
    pub active: Mutex<bool>,
    pub last_insertion: Mutex<Option<LastInsertion>>,
    stop_tx: Mutex<Option<mpsc::Sender<()>>>,
    audio_capture: Mutex<AudioCapture>,
}

impl Default for PipelineState {
    fn default() -> Self {
        Self {
            active: Mutex::new(false),
            last_insertion: Mutex::new(None),
            stop_tx: Mutex::new(None),
            audio_capture: Mutex::new(AudioCapture::new()),
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

    let _ = app.emit("dictation-state", "listening");
    log::info!("Dictation started");

    let api_key = match secrets::get_secret("deepgram_api_key") {
        Ok(Some(key)) if !key.is_empty() => key,
        _ => {
            let _ = app.emit("dictation-error", "Deepgram API key not configured");
            let _ = app.emit("dictation-state", "error");
            *state.active.lock().unwrap() = false;
            return;
        }
    };

    let keywords: Vec<String> = app
        .try_state::<Store>()
        .and_then(|store| store.get_dictionary().ok())
        .unwrap_or_default()
        .into_iter()
        .map(|w| w.word)
        .collect();

    let config = DeepgramConfig {
        api_key,
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
            *state.active.lock().unwrap() = false;
            return;
        }
    }

    let app_handle = app.clone();

    let app_for_stt = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = deepgram::stream_audio(config, audio_rx, transcript_tx, stop_rx).await {
            log::error!("Deepgram stream error: {e}");
            let _ = app_for_stt.emit("dictation-error", format!("STT error: {e}"));
        }
    });

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
        let app_name = fg.as_ref().map(|a| a.exe.as_str()).unwrap_or("unknown").to_string();

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
                        let _ = inject::undo_insertion(&LastInsertion { text: old.clone(), char_count: count });
                        if let Ok(rewritten) = tone::rewrite_with_llm(&old, &instruction).await {
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

            let has_llm_key = secrets::get_secret("llm_api_key")
                .ok()
                .and_then(|o| o)
                .is_some();

            let final_output = if has_llm_key {
                let tone_name = fg
                    .as_ref()
                    .and_then(|a| tone::get_tone_for_app(a, &store))
                    .unwrap_or_else(|| "default".to_string());
                tone::apply_tone(&expanded, &tone_name)
                    .await
                    .unwrap_or(expanded)
            } else {
                expanded
            };

            let pipeline_state = app_handle.state::<PipelineState>();
            if let Ok(ins) = inject::inject_text(&final_output) {
                *pipeline_state.last_insertion.lock().unwrap() = Some(ins);
            }

            let _ = store.add_history(&final_output, &app_name);
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

    state.audio_capture.lock().unwrap().stop();

    if let Some(tx) = state.stop_tx.lock().unwrap().take() {
        let _ = tx.try_send(());
    }

    log::info!("Dictation stopped");
}
