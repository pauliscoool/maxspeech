use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::secrets;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranscriptionResult {
    pub text: String,
    pub speakers: Vec<SpeakerSegment>,
    pub duration_secs: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SpeakerSegment {
    pub speaker: String,
    pub text: String,
}

pub async fn transcribe(
    file_path: &str,
) -> Result<TranscriptionResult, Box<dyn std::error::Error + Send + Sync>> {
    let data = tokio::fs::read(file_path).await?;
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("wav");

    let mime = match ext {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "mp4" | "mkv" | "webm" => "video/mp4",
        _ => "audio/wav",
    };

    let client = reqwest::Client::new();
    let mut last_err: Option<String> = None;
    let mut body: Option<serde_json::Value> = None;

    for (i, api_key) in secrets::deepgram_key_candidates().into_iter().enumerate() {
        let resp = client
            .post("https://api.deepgram.com/v1/listen?model=nova-3&punctuate=true&diarize=true&smart_format=true")
            .header("Authorization", format!("Token {}", api_key))
            .header("Content-Type", mime)
            .body(data.clone())
            .send()
            .await;

        match resp {
            Ok(resp) => {
                let status = resp.status();
                let json: serde_json::Value = resp.json().await.unwrap_or(serde_json::json!({}));
                if status.is_success() {
                    if i > 0 {
                        log::warn!("Deepgram batch auth failed with primary key; using app fallback");
                    }
                    body = Some(json);
                    break;
                }
                let err = json["err_msg"]
                    .as_str()
                    .or_else(|| json["error"].as_str())
                    .unwrap_or("Deepgram request failed")
                    .to_string();
                log::warn!("Deepgram batch attempt {} failed ({status}): {err}", i + 1);
                last_err = Some(err);
                // Retry on auth / forbidden; otherwise stop.
                if status.as_u16() != 401 && status.as_u16() != 403 {
                    break;
                }
            }
            Err(e) => {
                log::warn!("Deepgram batch attempt {} network error: {e}", i + 1);
                last_err = Some(e.to_string());
            }
        }
    }

    let body = body.ok_or_else(|| last_err.unwrap_or_else(|| "Deepgram API key not set".into()))?;

    let text = body["results"]["channels"][0]["alternatives"][0]["transcript"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let duration_secs = body["metadata"]["duration"].as_f64().unwrap_or(0.0);

    // Extract speaker segments if diarization is present
    let mut speakers = Vec::new();
    if let Some(words) = body["results"]["channels"][0]["alternatives"][0]["words"].as_array() {
        let mut current_speaker = String::new();
        let mut current_text = String::new();
        for word in words {
            let speaker = format!("Speaker {}", word["speaker"].as_i64().unwrap_or(0));
            let w = word["punctuated_word"]
                .as_str()
                .or(word["word"].as_str())
                .unwrap_or("");
            if speaker != current_speaker {
                if !current_text.is_empty() {
                    speakers.push(SpeakerSegment {
                        speaker: current_speaker.clone(),
                        text: current_text.trim().to_string(),
                    });
                }
                current_speaker = speaker;
                current_text = String::new();
            }
            current_text.push_str(w);
            current_text.push(' ');
        }
        if !current_text.is_empty() {
            speakers.push(SpeakerSegment {
                speaker: current_speaker,
                text: current_text.trim().to_string(),
            });
        }
    }

    Ok(TranscriptionResult {
        text,
        speakers,
        duration_secs,
    })
}

pub fn export(format: &str, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let desktop = if let Some(d) = std::env::var_os("USERPROFILE") {
        std::path::Path::new(&d).join("Desktop")
    } else {
        std::path::PathBuf::from(".")
    };

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = match format {
        "srt" => format!("maxspeech_{timestamp}.srt"),
        "md" => format!("maxspeech_{timestamp}.md"),
        _ => format!("maxspeech_{timestamp}.txt"),
    };

    let path = desktop.join(&filename);
    let content = match format {
        "md" => format!("# MaxSpeech Transcription\n\n{text}\n"),
        "srt" => {
            // Simple single-block SRT
            format!("1\n00:00:00,000 --> 99:59:59,999\n{text}\n")
        }
        _ => text.to_string(),
    };
    std::fs::write(&path, content)?;
    log::info!("Exported transcription to {}", path.display());
    Ok(())
}
