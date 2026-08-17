use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Mutex;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::secrets;

/// Last Deepgram key that successfully connected (avoids retrying a bad user key every session).
static LAST_GOOD_KEY: Mutex<Option<String>> = Mutex::new(None);

/// High-value English terms that ASR often mangles; merged with the user dictionary as keyterms.
/// NOTE: deliberately does NOT include "MaxSpeech" — boosting the app's own name
/// biases ASR toward hearing it for near-homophones (e.g. "Maximus Dev" → "MaxSpeech").
const BUILTIN_KEYTERMS: &[&str] = &[
    "Deepgram",
    "Supabase",
    "Tauri",
    "Claude",
    "ChatGPT",
    // Version control — ASR loves "Git" → "get"
    "Git",
    "GitHub",
    "GitLab",
    "gitignore",
    "git push",
    "git pull",
    "git commit",
    "git clone",
    "git merge",
    "git rebase",
    "git status",
    "Vercel",
    "cloud",
    "Cloudflare",
    "OpenAI",
    "API",
    "JSON",
    "TypeScript",
    "JavaScript",
    "PostgreSQL",
    "Postgres",
    "SQLite",
    "GraphQL",
    "Docker",
    "Kubernetes",
    "AWS",
    "React",
    "Next.js",
    "Node.js",
    "Python",
    "Rust",
    "VS Code",
    "Copilot",
    "Anthropic",
    "OAuth",
    "Redis",
    "MongoDB",
    "npm",
    "Vite",
    "Windows",
    "macOS",
    "Linux",
    "Notion",
    "Slack",
    "Discord",
    "Figma",
    "Linear",
    // Gaming / mods — before Cursor so "curse forge" isn't biased to "Cursor".
    "CurseForge",
    "Curse Forge",
    "Forge",
    // Percents — ASR often hears "percent" as "times".
    "percent",
    "percentage",
    "%",
    "Cursor",
];

#[derive(Debug, Clone)]
pub struct DeepgramConfig {
    pub api_key: String,
    pub model: String,
    pub language: String,
    pub keywords: Vec<String>,
}

/// Merge user dictionary terms with built-in keyterms (deduped, capped for URL size).
/// User terms come first so personal names win; builtins always get reserved slots
/// so a large dictionary cannot drop Git / TypeScript / CurseForge / etc.
pub fn merge_keyterms(user: Vec<String>) -> Vec<String> {
    const MAX: usize = 80;
    let mut out: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let builtin_n = BUILTIN_KEYTERMS.len().min(MAX);
    let user_cap = MAX.saturating_sub(builtin_n);

    for term in user {
        if out.len() >= user_cap {
            break;
        }
        let t = term.trim();
        if t.is_empty() {
            continue;
        }
        if seen.insert(t.to_lowercase()) {
            out.push(t.to_string());
        }
    }

    for term in BUILTIN_KEYTERMS {
        if out.len() >= MAX {
            break;
        }
        let t = term.trim();
        if t.is_empty() {
            continue;
        }
        if seen.insert(t.to_lowercase()) {
            out.push(t.to_string());
        }
    }
    out
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

/// Languages included in Deepgram Nova-3 `language=multi` code-switching.
const MULTI_CODES: &[&str] = &[
    "en", "es", "fr", "de", "hi", "ru", "pt", "ja", "it", "nl",
];

const ALLOWED_CODES: &[&str] = &[
    "en", "es", "zh", "hi", "ar", "fr", "pt", "ru", "de", "ja", "ko", "it", "tr", "vi",
    "pl", "uk", "nl", "id", "th", "bg",
];

/// Normalize / dedupe selected language codes (max 5).
pub fn normalize_languages(languages: &[String]) -> Vec<String> {
    let mut unique = Vec::new();
    for s in languages {
        let code = s.trim().to_lowercase();
        if ALLOWED_CODES.contains(&code.as_str()) && !unique.contains(&code) {
            unique.push(code);
        }
        if unique.len() >= 5 {
            break;
        }
    }
    if unique.is_empty() {
        unique.push("en".to_string());
    }
    unique
}

/// Languages to actually send to Deepgram for this session.
///
/// When multilingual is off, always one code. If a stale multi-select remains
/// (common after disabling multi / free-tier gating), prefer English when it is
/// in the list — otherwise English speech gets forced through e.g. `language=ru`
/// and comes back as total gibberish.
///
/// When multilingual is on, English is sorted first (UX + heal order only;
/// Deepgram `language=multi` does not take a preference list).
pub fn effective_languages(multilingual: bool, languages: &[String]) -> Vec<String> {
    let mut unique = normalize_languages(languages);
    if multilingual && unique.len() >= 2 {
        if let Some(idx) = unique.iter().position(|c| c == "en") {
            let en = unique.remove(idx);
            unique.insert(0, en);
        }
        return unique;
    }
    if unique.len() == 1 {
        return unique;
    }
    if let Some(en) = unique.iter().find(|c| c.as_str() == "en") {
        return vec![en.clone()];
    }
    vec![unique[0].clone()]
}

/// Resolve Deepgram `language=` from settings (`stt_multilingual`, `stt_languages`).
///
/// When multilingual is on and ≥2 selected languages are in Nova-3's multi set,
/// returns `"multi"` so code-switching (e.g. Russian↔English) works in one stream.
pub fn resolve_language(multilingual: bool, languages: &[String]) -> String {
    let unique = effective_languages(multilingual, languages);

    if !multilingual || unique.len() == 1 {
        return unique[0].clone();
    }

    let multi_langs: Vec<&str> = unique
        .iter()
        .filter(|c| MULTI_CODES.contains(&c.as_str()))
        .map(|c| c.as_str())
        .collect();

    // True code-switching only when ≥2 selected langs are in Deepgram's multi set.
    // Prefer multi even if extra non-multi langs were also picked (those simply
    // aren't covered by the multi model; the multi-capable ones still code-switch).
    if multi_langs.len() >= 2 {
        "multi".to_string()
    } else {
        // Fall back to the first code-switch-capable pick, else the first selected.
        multi_langs
            .first()
            .map(|s| (*s).to_string())
            .unwrap_or_else(|| unique[0].clone())
    }
}

/// Parse `stt_languages` JSON (or comma list) from settings.
pub fn parse_languages_setting(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return vec!["en".to_string()];
    };
    if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(raw) {
        let mut out = Vec::new();
        for v in arr {
            if let Some(s) = v.as_str() {
                let code = s.trim().to_lowercase();
                if ALLOWED_CODES.contains(&code.as_str()) && !out.contains(&code) {
                    out.push(code);
                }
            }
            if out.len() >= 5 {
                break;
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    let mut out = Vec::new();
    for part in raw.split(|c: char| c == ',' || c == '+' || c.is_whitespace()) {
        let code = part.trim().to_lowercase();
        if ALLOWED_CODES.contains(&code.as_str()) && !out.contains(&code) {
            out.push(code);
        }
        if out.len() >= 5 {
            break;
        }
    }
    if out.is_empty() {
        vec!["en".to_string()]
    } else {
        out
    }
}

#[derive(Debug, Deserialize)]
struct DgResponse {
    #[serde(rename = "type")]
    _msg_type: Option<String>,
    channel: Option<DgChannel>,
    is_final: Option<bool>,
    speech_final: Option<bool>,
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

type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

fn build_url(config: &DeepgramConfig) -> String {
    // endpointing: wait for a short silence before speech_final.
    // Keep this moderate — too low chops quiet ends; too high feels laggy.
    // Quiet / soft last syllables need a bit more than 500ms on English.
    let endpointing = if config.language == "multi" { 350 } else { 650 };
    let mut url = format!(
        "wss://api.deepgram.com/v1/listen?model={}&language={}&punctuate=true&interim_results=true&smart_format=true&numerals=true&endpointing={}&encoding=linear16&sample_rate=16000&channels=1",
        config.model, config.language, endpointing
    );
    // Nova-3 rejects legacy `keywords` (HTTP 400). Use `keyterm` instead.
    // Cap to keep the handshake URL reasonable (Nova-3 allows many; URL length is the limit).
    for kw in config.keywords.iter().take(80) {
        let term = kw.trim();
        if !term.is_empty() {
            url.push_str(&format!("&keyterm={}", urlenc(term)));
        }
    }
    log::info!(
        "Deepgram listen URL model={} language={} endpointing={} keyterms={}",
        config.model,
        config.language,
        endpointing,
        config.keywords.len().min(80)
    );
    url
}

/// Cheap TLS/DNS warm so the first real listen handshake is faster.
pub async fn prewarm() {
    let candidates = {
        let mut keys = secrets::deepgram_key_candidates();
        if let Ok(guard) = LAST_GOOD_KEY.lock() {
            if let Some(ref k) = *guard {
                keys.retain(|x| x != k);
                keys.insert(0, k.clone());
            }
        }
        keys
    };
    let config = DeepgramConfig {
        api_key: candidates.first().cloned().unwrap_or_default(),
        keywords: Vec::new(),
        ..Default::default()
    };
    for key in candidates {
        let mut cfg = config.clone();
        cfg.api_key = key;
        match connect_ws(&cfg).await {
            Ok(mut ws) => {
                let _ = LAST_GOOD_KEY.lock().map(|mut g| *g = Some(cfg.api_key.clone()));
                // Close immediately — we only wanted the handshake warm.
                let _ = ws.close(None).await;
                log::info!("Deepgram prewarm OK");
                return;
            }
            Err(e) => log::warn!("Deepgram prewarm failed: {e}"),
        }
    }
}

async fn connect_ws(
    config: &DeepgramConfig,
) -> Result<WsStream, Box<dyn std::error::Error + Send + Sync>> {
    let url = build_url(config);
    let request = tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(&url)
        .header("Authorization", format!("Token {}", config.api_key))
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .header("Sec-WebSocket-Version", "13")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Host", "api.deepgram.com")
        .body(())
        .unwrap();

    let (ws_stream, _) = tokio_tungstenite::connect_async(request).await?;
    Ok(ws_stream)
}

fn pcm_binary(chunk: &[i16]) -> Message {
    let bytes: Vec<u8> = chunk.iter().flat_map(|&s| s.to_le_bytes()).collect();
    Message::Binary(bytes.into())
}

async fn connect_with_fallback(
    config: &mut DeepgramConfig,
    candidates: Vec<String>,
) -> Result<WsStream, Box<dyn std::error::Error + Send + Sync>> {
    let mut last_err: Option<Box<dyn std::error::Error + Send + Sync>> = None;
    for (i, key) in candidates.into_iter().enumerate() {
        config.api_key = key;
        match connect_ws(config).await {
            Ok(ws) => {
                if i > 0 {
                    log::warn!("Deepgram connect failed with primary key; using app fallback");
                } else {
                    log::info!("Deepgram WebSocket connected");
                }
                let _ = LAST_GOOD_KEY
                    .lock()
                    .map(|mut g| *g = Some(config.api_key.clone()));
                return Ok(ws);
            }
            Err(e) => {
                log::warn!("Deepgram connect attempt {} failed: {e}", i + 1);
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| "Deepgram connect failed".into()))
}

pub async fn stream_audio(
    mut config: DeepgramConfig,
    mut audio_rx: mpsc::UnboundedReceiver<Vec<i16>>,
    transcript_tx: mpsc::UnboundedSender<TranscriptChunk>,
    mut stop_rx: mpsc::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut candidates = if config.api_key.is_empty() {
        secrets::deepgram_key_candidates()
    } else {
        let mut keys = vec![config.api_key.clone()];
        for k in secrets::deepgram_key_candidates() {
            if k != config.api_key {
                keys.push(k);
            }
        }
        keys
    };
    // Prefer the last key that worked (skips a failed user-key round-trip).
    if let Ok(guard) = LAST_GOOD_KEY.lock() {
        if let Some(ref k) = *guard {
            if let Some(idx) = candidates.iter().position(|x| x == k) {
                let good = candidates.remove(idx);
                candidates.insert(0, good);
            }
        }
    }

    // Drain mic PCM *during* TLS/WS so the first seconds are not sitting
    // unconsumed (and never dropped) while the handshake runs.
    let mut pending: Vec<Vec<i16>> = Vec::new();
    let connect = connect_with_fallback(&mut config, candidates);
    tokio::pin!(connect);
    let ws_stream = loop {
        tokio::select! {
            biased;
            res = &mut connect => break res?,
            Some(chunk) = audio_rx.recv() => pending.push(chunk),
        }
    };
    if !pending.is_empty() {
        let samples: usize = pending.iter().map(|c| c.len()).sum();
        log::info!(
            "Deepgram handshake complete; flushing {samples} pre-connect PCM samples ({} chunks)",
            pending.len()
        );
    }

    let (mut write, mut read) = ws_stream.split();

    let send_task = tokio::spawn(async move {
        for chunk in pending {
            if write.send(pcm_binary(&chunk)).await.is_err() {
                return;
            }
        }
        loop {
            tokio::select! {
                Some(audio_chunk) = audio_rx.recv() => {
                    if write.send(pcm_binary(&audio_chunk)).await.is_err() {
                        break;
                    }
                }
                _ = stop_rx.recv() => {
                    // Drain trailing PCM (hotkey-release trail + in-flight chunks)
                    // before Finalize so Deepgram still hears word endings.
                    loop {
                        match tokio::time::timeout(
                            std::time::Duration::from_millis(100),
                            audio_rx.recv(),
                        )
                        .await
                        {
                            Ok(Some(audio_chunk)) => {
                                if write.send(pcm_binary(&audio_chunk)).await.is_err() {
                                    break;
                                }
                            }
                            Ok(None) | Err(_) => break,
                        }
                    }
                    // Ask Deepgram to flush finals for the last utterance, wait
                    // briefly for them on the read side, then close.
                    let _ = write
                        .send(Message::Text(r#"{"type":"Finalize"}"#.into()))
                        .await;
                    // Give Deepgram a moment to flush the last syllable after Finalize.
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let _ = write
                        .send(Message::Text(r#"{"type":"CloseStream"}"#.into()))
                        .await;
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
                                let is_final = resp.is_final.unwrap_or(false)
                                    || resp.speech_final.unwrap_or(false);
                                let _ = transcript_tx.send(TranscriptChunk {
                                    text: alt.transcript.clone(),
                                    is_final,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_en_ru_multilingual_is_multi() {
        let langs = vec!["en".into(), "ru".into()];
        assert_eq!(resolve_language(true, &langs), "multi");
    }

    #[test]
    fn resolve_single_language_even_if_toggle_on() {
        let langs = vec!["ru".into()];
        assert_eq!(resolve_language(true, &langs), "ru");
    }

    #[test]
    fn resolve_off_uses_first() {
        let langs = vec!["ru".into(), "de".into()];
        assert_eq!(resolve_language(false, &langs), "ru");
    }

    #[test]
    fn resolve_multi_with_extra_non_multi_still_multi() {
        // uk is not in Nova-3 multi set; en+ru still enable code-switching.
        let langs = vec!["en".into(), "uk".into(), "ru".into()];
        assert_eq!(resolve_language(true, &langs), "multi");
    }

    #[test]
    fn monolingual_heals_stale_ru_en_list_to_english() {
        // Bug: multi off + ["ru","en"] used to truncate to "ru" and destroy English dictation.
        let langs = vec!["ru".into(), "en".into()];
        assert_eq!(resolve_language(false, &langs), "en");
        assert_eq!(effective_languages(false, &langs), vec!["en".to_string()]);
    }

    #[test]
    fn monolingual_keeps_intentional_russian() {
        let langs = vec!["ru".into()];
        assert_eq!(resolve_language(false, &langs), "ru");
    }

    #[test]
    fn merge_keyterms_includes_builtins() {
        let merged = merge_keyterms(vec![]);
        let lower: Vec<String> = merged.iter().map(|s| s.to_lowercase()).collect();
        assert!(lower.iter().any(|s| s == "git"));
        assert!(lower.iter().any(|s| s == "typescript"));
        assert!(lower.iter().any(|s| s == "curseforge"));
        assert!(lower.iter().any(|s| s == "percent"));
        assert!(lower.iter().any(|s| s == "postgres"));
    }

    #[test]
    fn merge_keyterms_keeps_user_names_and_builtins() {
        let user = vec!["Sandra".into(), "Maximus".into()];
        let merged = merge_keyterms(user);
        assert_eq!(merged[0], "Sandra");
        assert_eq!(merged[1], "Maximus");
        assert!(merged.iter().any(|s| s.eq_ignore_ascii_case("GitHub")));
    }

    #[test]
    fn merge_keyterms_large_dictionary_still_keeps_builtins() {
        let user: Vec<String> = (0..100).map(|i| format!("Name{i}")).collect();
        let merged = merge_keyterms(user);
        assert!(merged.len() <= 80);
        let lower: Vec<String> = merged.iter().map(|s| s.to_lowercase()).collect();
        assert!(lower.iter().any(|s| s == "git"));
        assert!(lower.iter().any(|s| s == "curseforge"));
        assert!(lower.iter().any(|s| s == "name0"));
    }

    #[test]
    fn pcm_binary_is_little_endian_i16() {
        let msg = pcm_binary(&[0x1234, -2]);
        match msg {
            Message::Binary(b) => assert_eq!(&b[..], &[0x34, 0x12, 0xFE, 0xFF]),
            other => panic!("expected binary, got {other:?}"),
        }
    }
}
