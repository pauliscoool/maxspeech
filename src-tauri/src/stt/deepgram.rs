use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone)]
pub struct DeepgramConfig {
    pub api_key: String,
    pub model: String,
    pub language: String,
    pub keywords: Vec<String>,
}

impl Default for DeepgramConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "nova-3".to_string(),
            language: "en".to_string(),
            keywords: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct DgResponse {
    #[serde(rename = "type")]
    _msg_type: Option<String>,
    channel: Option<DgChannel>,
    is_final: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DgChannel {
    alternatives: Vec<DgAlternative>,
}

#[derive(Debug, Deserialize)]
struct DgAlternative {
    transcript: String,
}

#[derive(Debug, Clone)]
pub struct TranscriptChunk {
    pub text: String,
    pub is_final: bool,
}

pub async fn stream_audio(
    config: DeepgramConfig,
    mut audio_rx: mpsc::UnboundedReceiver<Vec<i16>>,
    transcript_tx: mpsc::UnboundedSender<TranscriptChunk>,
    mut stop_rx: mpsc::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut url = format!(
        "wss://api.deepgram.com/v1/listen?model={}&language={}&punctuate=true&interim_results=true&smart_format=true&encoding=linear16&sample_rate=16000&channels=1",
        config.model, config.language
    );

    for kw in &config.keywords {
        url.push_str(&format!("&keywords={}", urlenc(kw)));
    }

    let request = tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(&url)
        .header("Authorization", format!("Token {}", config.api_key))
        .header("Sec-WebSocket-Key", tokio_tungstenite::tungstenite::handshake::client::generate_key())
        .header("Sec-WebSocket-Version", "13")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Host", "api.deepgram.com")
        .body(())
        .unwrap();

    let (ws_stream, _) = tokio_tungstenite::connect_async(request).await?;
    let (mut write, mut read) = ws_stream.split();

    log::info!("Deepgram WebSocket connected");

    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(audio_chunk) = audio_rx.recv() => {
                    let bytes: Vec<u8> = audio_chunk
                        .iter()
                        .flat_map(|&s| s.to_le_bytes())
                        .collect();
                    if write.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
                _ = stop_rx.recv() => {
                    let _ = write.send(Message::Text(r#"{"type":"CloseStream"}"#.into())).await;
                    break;
                }
            }
        }
    });

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(resp) = serde_json::from_str::<DgResponse>(&text) {
                    if let Some(channel) = resp.channel {
                        if let Some(alt) = channel.alternatives.first() {
                            if !alt.transcript.is_empty() {
                                let _ = transcript_tx.send(TranscriptChunk {
                                    text: alt.transcript.clone(),
                                    is_final: resp.is_final.unwrap_or(false),
                                });
                            }
                        }
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                log::error!("Deepgram WS error: {e}");
                break;
            }
            _ => {}
        }
    }

    send_task.abort();
    Ok(())
}

fn urlenc(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}
