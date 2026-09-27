//! When live transcription returns nothing, retry from the recorded audio and,
//! if that fails too, keep the recording as a "failed" History entry so the
//! dictation is never silently lost.

use std::time::Duration;

use tauri::{Emitter, Manager};

use crate::recording::{self, MAX_REMAKE_RECORDINGS, WAV_SAMPLE_RATE};
use crate::store::Store;
use crate::stt::batch;

const MIN_SECS: f64 = 0.8;
const WINDOW_MS: usize = 100;
/// Ambient room noise sits well under this; speech is far above it.
const VOICE_RMS: f64 = 350.0;
const MIN_VOICE_WINDOWS: usize = 3;
const ATTEMPTS: usize = 2;
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(45);
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

    let mut last = String::from("No speech recognized");
    for attempt in 0..ATTEMPTS {
        let call = batch::transcribe_with_language_and_keyterms(&path_str, language, keyterms);
        match tokio::time::timeout(ATTEMPT_TIMEOUT, call).await {
            Ok(Ok(result)) => {
                let text = result.text.trim().to_string();
                if text.is_empty() {
                    last = "No speech recognized".into();
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
pub fn save_failed(app: &tauri::AppHandle, app_name: &str, reason: &str, pcm: &[i16]) {
    let Some(store) = app.try_state::<Store>() else {
        return;
    };
    let hid = match store.add_failed_history(reason, app_name) {
        Ok(id) => id,
        Err(e) => {
            log::error!("Could not record failed dictation: {e}");
            return;
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
