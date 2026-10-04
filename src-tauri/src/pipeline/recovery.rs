//! When live transcription returns nothing, retry from the recorded audio and,
//! if that fails too, keep the recording as a "failed" History entry so the
//! dictation is never silently lost.

use std::time::Duration;

use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::recording::{self, MAX_REMAKE_RECORDINGS, WAV_SAMPLE_RATE};
use crate::store::Store;
use crate::stt::batch;

const MIN_SECS: f64 = 0.8;
const WINDOW_MS: usize = 100;
/// Ambient room noise sits well under this; speech is far above it.
const VOICE_RMS: f64 = 350.0;
const MIN_VOICE_WINDOWS: usize = 3;
/// Soft-spoken speech on a low-gain mic still clears this; a silent room does not.
const QUIET_VOICE_RMS: f64 = 220.0;
const ATTEMPTS: usize = 2;
const AUTO_RETRIES: usize = 3;
const AUTO_RETRY_DELAY: Duration = Duration::from_secs(5);
const RETRY_BACKOFF: Duration = Duration::from_millis(1500);

fn rms(window: &[i16]) -> f64 {
    if window.is_empty() {
        return 0.0;
    }
    let sum: f64 = window.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / window.len() as f64).sqrt()
}

/// True when the recording is long enough and loud enough to have held speech,
/// so an accidental tap or a silent mic doesn't clutter History with failures.
pub fn has_speech(pcm: &[i16]) -> bool {
    let rate = WAV_SAMPLE_RATE as usize;
    let window = rate * WINDOW_MS / 1000;
    if window == 0 || (pcm.len() as f64) < rate as f64 * MIN_SECS {
        return false;
    }
    pcm.chunks(window)
        .filter(|w| rms(w) > VOICE_RMS)
        .count()
        >= MIN_VOICE_WINDOWS
}

/// Quieter, shorter bar than [has_speech]: ~200ms above a low floor. Used to
/// dismiss the overlay instantly when the user said nothing at all.
pub fn heard_voice(pcm: &[i16]) -> bool {
    let window = WAV_SAMPLE_RATE as usize * WINDOW_MS / 1000;
    if window == 0 {
        return false;
    }
    pcm.chunks(window).filter(|w| rms(w) > QUIET_VOICE_RMS).count() >= 2
}

/// [retry_transcribe] error when the audio really has no words (not a network failure).
pub const NO_SPEECH: &str = "No speech recognized";

/// Re-transcribe the session audio with the batch API. Retries network/API
/// errors once; an empty transcript is final (the audio really has no words).
pub async fn retry_transcribe(
    pcm: &[i16],
    language: &str,
    keyterms: &[String],
) -> Result<String, String> {
    let path = recording::recordings_dir().join("retry_tmp.wav");
    recording::write_wav_i16(&path, pcm, WAV_SAMPLE_RATE)
        .map_err(|e| format!("Could not save the recording: {e}"))?;
    let path_str = path.to_string_lossy().into_owned();

    // Outer deadline covers the batch client's own retry: 2 requests plus backoff.
    let attempt_timeout = batch::request_timeout(pcm.len() * 2) * 2 + Duration::from_secs(5);
    let mut last = String::from(NO_SPEECH);
    for attempt in 0..ATTEMPTS {
        let call = batch::transcribe_with_language_and_keyterms(&path_str, language, keyterms);
        match tokio::time::timeout(attempt_timeout, call).await {
            Ok(Ok(result)) => {
                let text = result.text.trim().to_string();
                if text.is_empty() {
                    last = NO_SPEECH.into();
                    break;
                }
                recording::delete_recording_file(&path_str);
                return Ok(text);
            }
            Ok(Err(e)) => last = format!("Transcription failed: {e}"),
            Err(_) => last = "Transcription timed out".into(),
        }
        log::warn!("Dictation retry {} failed: {last}", attempt + 1);
        if attempt + 1 < ATTEMPTS {
            tokio::time::sleep(RETRY_BACKOFF).await;
        }
    }
    recording::delete_recording_file(&path_str);
    Err(last)
}

/// Save the audio as a failed History entry (shows at the top with a Retry button).
pub fn save_failed(
    app: &tauri::AppHandle,
    app_name: &str,
    reason: &str,
    pcm: &[i16],
) -> Option<i64> {
    let store = app.try_state::<Store>()?;
    let hid = match store.add_failed_history(reason, app_name) {
        Ok(id) => id,
        Err(e) => {
            log::error!("Could not record failed dictation: {e}");
            return None;
        }
    };
    let path = recording::wav_path_for_history_id(hid);
    match recording::write_wav_i16(&path, pcm, WAV_SAMPLE_RATE) {
        Ok(()) => {
            let _ = store.set_history_recording_path(hid, Some(&path.to_string_lossy()));
            if let Err(e) = store.prune_recordings(MAX_REMAKE_RECORDINGS) {
                log::warn!("prune_recordings: {e}");
            }
        }
        Err(e) => log::warn!("Failed to save recording for failed dictation: {e}"),
    }
    let _ = app.emit(
        "history-failed",
        serde_json::json!({ "id": hid, "text": reason, "app_name": app_name }),
    );
    Some(hid)
}

/// Native toast so a failure is visible even when the overlay pill is gone.
pub fn notify(app: &tauri::AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        log::warn!("Could not show notification: {e}");
    }
}

/// Re-run a failed dictation from its saved recording every few seconds, telling the
/// user how each attempt went. Gives up after [`AUTO_RETRIES`] and leaves it in History.
pub fn spawn_auto_retry(app: tauri::AppHandle, history_id: i64, target_exe: Option<String>) {
    // If the user dictates again before this lands, copy instead of pasting mid-session.
    let epoch = super::current_paste_epoch(&app);
    tauri::async_runtime::spawn(async move {
        for attempt in 1..=AUTO_RETRIES {
            tokio::time::sleep(AUTO_RETRY_DELAY).await;
            let auto = target_exe.as_deref().map(|exe| (exe, epoch));
            match crate::remake_core(&app, history_id, auto).await {
                Ok((_, pasted)) => {
                    let body = if pasted {
                        "Transcribed and pasted."
                    } else {
                        "Transcribed. Copied to your clipboard, press Ctrl+V."
                    };
                    notify(&app, "Recovered your dictation", body);
                    let _ = app.emit("history-updated", history_id);
                    return;
                }
                Err(e) => {
                    log::warn!("Auto-retry {attempt}/{AUTO_RETRIES} failed: {e}");
                    if attempt < AUTO_RETRIES {
                        notify(&app, "Still couldn't transcribe", "Retrying again in 5 seconds.");
                    }
                }
            }
        }
        notify(
            &app,
            "Couldn't transcribe",
            "Your recording is saved in History. Tap Remake to try again.",
        );
    });
}

#[cfg(test)]
mod tests {
    use super::{has_speech, retry_transcribe};

    /// Live check: re-transcribe one of the user's saved recordings.
    /// cargo test retry_live -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn retry_live_from_saved_recording() {
        let wav = std::fs::read_dir(crate::recording::recordings_dir())
            .expect("no recordings dir")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|x| x == "wav") && !p.ends_with("retry_tmp.wav"))
            .expect("no saved recording to test with");
        let bytes = std::fs::read(&wav).unwrap();
        let pcm: Vec<i16> = bytes[44..]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert!(has_speech(&pcm), "saved recording should count as speech");
        let text = retry_transcribe(&pcm, "en", &[]).await.expect("retry failed");
        println!("RECOVERED {} chars: {}", text.len(), text.chars().take(120).collect::<String>());
        assert!(!text.is_empty());
    }

    #[test]
    fn silence_and_taps_are_not_speech() {
        assert!(!has_speech(&[]));
        assert!(!has_speech(&vec![0i16; 16_000]));
        assert!(!has_speech(&vec![3000i16; 4_000])); // loud but only 0.25s
    }

    #[test]
    fn sustained_loud_audio_is_speech() {
        let pcm: Vec<i16> = (0..24_000)
            .map(|i| ((i as f64 * 0.3).sin() * 4000.0) as i16)
            .collect();
        assert!(has_speech(&pcm));
    }
}
