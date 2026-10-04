use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::secrets;

/// Last Deepgram key that successfully connected (avoids retrying a bad user key every session).
static LAST_GOOD_KEY: Mutex<Option<String>> = Mutex::new(None);

/// High-value English terms that ASR often mangles; merged with the user dictionary as keyterms.
/// Only distinctive names belong here. Boosting everyday words ("get"→Git, "cloud",
/// "react", "rust", "percent") makes Deepgram hear them when the speaker never said them.
/// NOTE: deliberately does NOT include "MaxSpeech" — boosting the app's own name
/// biases ASR toward hearing it for near-homophones (e.g. "Maximus Dev" → "MaxSpeech").
const BUILTIN_KEYTERMS: &[&str] = &[
    "Deepgram",
    "Supabase",
    "Tauri",
    "Claude",
    "ChatGPT",
    // Version control — ASR loves "Git" → "get"
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
    "Next.js",
    "Node.js",
    "Python",
    "VS Code",
    "Copilot",
    "Anthropic",
    "OAuth",
    "Redis",
    "MongoDB",
    "npm",
    "Vite",
    "macOS",
    "Linux",
    "Notion",
    "Discord",
    "Figma",
    // Gaming / mods.
    "CurseForge",
    // Product — ASR hears "Covenant Core" as Court / Corner.
    "Covenant Core",
    // Product — ASR hears "Tailscale" as "tail scale" / "tale scale".
    "Tailscale",
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
/// so a large dictionary cannot drop GitHub / TypeScript / CurseForge / etc.
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
    from_finalize: Option<bool>,
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

/// Longest wait for the last results after release before falling back to the recording.
const STOP_DRAIN_CAP_MS: u64 = 3_500;

type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

fn build_url(config: &DeepgramConfig) -> String {
    // endpointing: silence before speech_final. Too low chops the last words
    // on a brief pause; hold-to-talk can wait a bit for a complete phrase.
    let endpointing = if config.language == "multi" { 700 } else { 1100 };
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

/// Keep consuming mic audio until the user stops. When the connection is down the
/// session must still last until release, otherwise the pipeline would finish
/// (and paste a partial transcript) while the user is still talking.
async fn drain_until_stop(
    audio_rx: &mut mpsc::UnboundedReceiver<Vec<i16>>,
    stop_rx: &mut mpsc::Receiver<()>,
) {
    loop {
        tokio::select! {
            chunk = audio_rx.recv() => {
                if chunk.is_none() {
                    break;
                }
            }
            _ = stop_rx.recv() => break,
        }
    }
}

/// Streams mic audio to Deepgram. Returns `Ok(true)` only when the live transcript
/// is known complete; `Ok(false)` means the connection dropped or the tail never
/// arrived and the caller should re-transcribe the recording.
pub async fn stream_audio(
    mut config: DeepgramConfig,
    mut audio_rx: mpsc::UnboundedReceiver<Vec<i16>>,
    transcript_tx: mpsc::UnboundedSender<TranscriptChunk>,
    mut stop_rx: mpsc::Receiver<()>,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
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
    let started = Instant::now();
    let mut pending: Vec<Vec<i16>> = Vec::new();
    let connect = tokio::time::timeout(
        Duration::from_secs(8),
        connect_with_fallback(&mut config, candidates),
    );
    tokio::pin!(connect);
    let ws_stream = loop {
        tokio::select! {
            biased;
            res = &mut connect => {
                match res {
                    Ok(Ok(ws)) => break ws,
                    Ok(Err(e)) => {
                        drain_until_stop(&mut audio_rx, &mut stop_rx).await;
                        return Err(e);
                    }
                    Err(_) => {
                        drain_until_stop(&mut audio_rx, &mut stop_rx).await;
                        return Err("Deepgram connection timed out".into());
                    }
                }
            }
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

    let connect_ms = started.elapsed().as_millis() as u64;
    let (mut write, mut read) = ws_stream.split();
    // Milliseconds since `started`; 0 = not reached. Only feeds the timing log line.
    let stop_at = Arc::new(AtomicU64::new(0));

    let stop_requested = Arc::new(AtomicBool::new(false));
    let degraded = Arc::new(AtomicBool::new(false));
    let finalize_ack = Arc::new(AtomicBool::new(false));
    let interim_pending = Arc::new(AtomicBool::new(false));

    let send_task = {
        let stop_requested = stop_requested.clone();
        let degraded = degraded.clone();
        let stop_at = stop_at.clone();
        tokio::spawn(async move {
        let mut offline = false;
        for chunk in pending {
            if write.send(pcm_binary(&chunk)).await.is_err() {
                offline = true;
                degraded.store(true, Ordering::SeqCst);
                break;
            }
        }
        loop {
            tokio::select! {
                Some(audio_chunk) = audio_rx.recv() => {
                    if !offline && write.send(pcm_binary(&audio_chunk)).await.is_err() {
                        offline = true;
                        degraded.store(true, Ordering::SeqCst);
                    }
                }
                _ = stop_rx.recv() => {
                    stop_requested.store(true, Ordering::SeqCst);
                    stop_at.store((started.elapsed().as_millis() as u64).max(1), Ordering::SeqCst);
                    if offline {
                        break;
                    }
                    // Drain trailing PCM (hotkey-release trail + in-flight chunks)
                    // before Finalize so Deepgram still hears word endings.
                    loop {
                        match tokio::time::timeout(
                            std::time::Duration::from_millis(280),
                            audio_rx.recv(),
                        )
                        .await
                        {
                            Ok(Some(audio_chunk)) => {
                                if write.send(pcm_binary(&audio_chunk)).await.is_err() {
                                    degraded.store(true, Ordering::SeqCst);
                                    break;
                                }
                            }
                            Ok(None) | Err(_) => break,
                        }
                    }
                    // CloseStream alone flushes every remaining result before Deepgram closes
                    // the socket. Measured ~110ms vs ~270ms for Finalize + waiting on its ack,
                    // and it still returns the whole transcript behind an audio backlog.
                    let _ = write
                        .send(Message::Text(r#"{"type":"CloseStream"}"#.into()))
                        .await;
                    break;
                }
            }
        }
        })
    };

    loop {
        let next = if stop_requested.load(Ordering::SeqCst) {
            let since_stop = (started.elapsed().as_millis() as u64)
                .saturating_sub(stop_at.load(Ordering::SeqCst));
            let left = STOP_DRAIN_CAP_MS.saturating_sub(since_stop);
            match tokio::time::timeout(Duration::from_millis(left), read.next()).await {
                Ok(next) => next,
                Err(_) => {
                    // A dead link must not hold the paste forever; the caller re-transcribes.
                    log::warn!("Deepgram did not finish within {STOP_DRAIN_CAP_MS}ms of stop");
                    degraded.store(true, Ordering::SeqCst);
                    break;
                }
            }
        } else {
            read.next().await
        };
        let Some(msg) = next else { break };
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(resp) = serde_json::from_str::<DgResponse>(&text) {
                    if resp.from_finalize == Some(true) {
                        finalize_ack.store(true, Ordering::SeqCst);
                    }
                    if let Some(channel) = resp.channel {
                        if let Some(alt) = channel.alternatives.first() {
                            if !alt.transcript.is_empty() {
                                let is_final = resp.is_final.unwrap_or(false)
                                    || resp.speech_final.unwrap_or(false);
                                interim_pending.store(!is_final, Ordering::SeqCst);
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
                degraded.store(true, Ordering::SeqCst);
                break;
            }
            _ => {}
        }
    }

    // The socket ended before the user stopped: keep the session (and the local
    // recording) alive until release so the caller can re-transcribe everything.
    if !stop_requested.load(Ordering::SeqCst) {
        degraded.store(true, Ordering::SeqCst);
        let _ = send_task.await;
    } else {
        send_task.abort();
    }
    let stop_ms = stop_at.load(Ordering::SeqCst);
    if stop_ms > 0 {
        let drain_ms = (started.elapsed().as_millis() as u64).saturating_sub(stop_ms);
        #[cfg(test)]
        eprintln!("TIMING connect={connect_ms}ms stop->done={drain_ms}ms");
        log::info!("Deepgram timing: connect={connect_ms}ms stop->done={drain_ms}ms");
    }
    let complete = stop_requested.load(Ordering::SeqCst)
        && !degraded.load(Ordering::SeqCst)
        && (finalize_ack.load(Ordering::SeqCst) || !interim_pending.load(Ordering::SeqCst));
    Ok(complete)
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
        assert!(lower.iter().any(|s| s == "github"));
        assert!(lower.iter().any(|s| s == "typescript"));
        assert!(lower.iter().any(|s| s == "curseforge"));
        // Everyday words must not be boosted — they get "heard" when never said.
        for common in ["git", "percent", "forge", "react", "rust", "cursor", "windows"] {
            assert!(!lower.iter().any(|s| s == common), "{common} should not be a keyterm");
        }
        assert!(lower.iter().any(|s| s == "postgres"));
        assert!(lower.iter().any(|s| s == "covenant core"));
        assert!(lower.iter().any(|s| s == "tailscale"));
        assert!(!lower.iter().any(|s| s.contains("corner")));
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
        assert!(lower.iter().any(|s| s == "github"));
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

    /// Streams saved 16 kHz mono recordings in real time to Nova-3 (v1) and Flux (v2)
    /// and reports last-audio -> last-transcript latency. Real API calls.
    #[tokio::test]
    #[ignore]
    async fn model_probe() {
        let dir = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("MaxSpeech/recordings");
        let wavs = std::env::var("PROBE_WAVS").unwrap_or_else(|_| "2413.wav,2414.wav,2416.wav".into());
        let key = secrets::deepgram_key_candidates().into_iter().last().unwrap();
        let targets = [
            ("nova-3", "wss://api.deepgram.com/v1/listen?model=nova-3&language=en&punctuate=true&interim_results=true&smart_format=true&numerals=true&endpointing=1100&encoding=linear16&sample_rate=16000&channels=1"),
            ("flux-en", "wss://api.deepgram.com/v2/listen?model=flux-general-en&encoding=linear16&sample_rate=16000"),
        ];
        for wav in wavs.split(',') {
            let bytes = std::fs::read(dir.join(wav.trim())).unwrap();
            let pcm = &bytes[44..];
            for (name, url) in targets {
                let req = tokio_tungstenite::tungstenite::http::Request::builder()
                    .uri(url)
                    .header("Authorization", format!("Token {key}"))
                    .header("Sec-WebSocket-Key", tokio_tungstenite::tungstenite::handshake::client::generate_key())
                    .header("Sec-WebSocket-Version", "13")
                    .header("Connection", "Upgrade")
                    .header("Upgrade", "websocket")
                    .header("Host", "api.deepgram.com")
                    .body(())
                    .unwrap();
                let t_conn = std::time::Instant::now();
                let (ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
                let connect_ms = t_conn.elapsed().as_millis();
                let (mut w, mut r) = ws.split();
                let reader = tokio::spawn(async move {
                    let mut finals: Vec<String> = Vec::new();
                    let mut last = std::time::Instant::now();
                    let mut types = std::collections::BTreeMap::<String, u32>::new();
                    while let Some(Ok(m)) = r.next().await {
                        if let Message::Text(t) = m {
                            let v: serde_json::Value = serde_json::from_str(&t).unwrap_or_default();
                            let ty = v["type"].as_str().unwrap_or("?").to_string();
                            let ev = v["event"].as_str().unwrap_or("").to_string();
                            *types.entry(format!("{ty}/{ev}")).or_default() += 1;
                            if v["is_final"].as_bool() == Some(true) {
                                finals.push(v["channel"]["alternatives"][0]["transcript"].as_str().unwrap_or("").to_string());
                                last = std::time::Instant::now();
                            } else if ev == "EndOfTurn" {
                                finals.push(v["transcript"].as_str().unwrap_or("").to_string());
                                last = std::time::Instant::now();
                            }
                        } else if let Message::Close(_) = m {
                            break;
                        }
                    }
                    (finals, last, types)
                });
                for chunk in pcm.chunks(2560) {
                    w.send(Message::Binary(chunk.to_vec().into())).await.unwrap();
                    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                }
                let t_end = std::time::Instant::now();
                w.send(Message::Text(r#"{"type":"CloseStream"}"#.into())).await.unwrap();
                let (finals, last, types) = tokio::time::timeout(std::time::Duration::from_secs(8), reader)
                    .await
                    .map(|r| r.unwrap())
                    .unwrap_or_default_probe();
                let text = finals.join(" ");
                println!(
                    "{wav} {name}: connect={connect_ms}ms stop->lastfinal={}ms words={} msgs={types:?}\n   {}",
                    last.saturating_duration_since(t_end).as_millis(),
                    text.split_whitespace().count(),
                    text.chars().take(160).collect::<String>()
                );
            }
        }
    }

    trait ProbeDefault {
        fn unwrap_or_default_probe(self) -> (Vec<String>, std::time::Instant, std::collections::BTreeMap<String, u32>);
    }
    impl ProbeDefault for Result<(Vec<String>, std::time::Instant, std::collections::BTreeMap<String, u32>), tokio::time::error::Elapsed> {
        fn unwrap_or_default_probe(self) -> (Vec<String>, std::time::Instant, std::collections::BTreeMap<String, u32>) {
            self.unwrap_or_else(|_| (vec!["<TIMEOUT 8s>".into()], std::time::Instant::now(), Default::default()))
        }
    }
}

#[cfg(test)]
mod latency_probe {
    use super::*;

    fn read_wav_pcm(path: &std::path::Path) -> Vec<i16> {
        let bytes = std::fs::read(path).expect("read wav");
        bytes[44..]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect()
    }

    fn word_edit_distance(a: &[String], b: &[String]) -> usize {
        let mut prev: Vec<usize> = (0..=b.len()).collect();
        for i in 1..=a.len() {
            let mut cur = vec![i; b.len() + 1];
            for j in 1..=b.len() {
                let cost = usize::from(a[i - 1] != b[j - 1]);
                cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            }
            prev = cur;
        }
        prev[b.len()]
    }

    /// Same recording through the live stream and through batch (prerecorded) Nova-3.
    /// PROBE_WAVS=2225.wav cargo test --release --bin maxspeech compare_stream_vs_batch -- --ignored --nocapture
    #[test]
    #[ignore]
    fn compare_stream_vs_batch() {
        let names = std::env::var("PROBE_WAVS").unwrap_or_else(|_| "2225.wav".into());
        let rt = tokio::runtime::Runtime::new().unwrap();
        for name in names.split(',') {
            let path = crate::recording::recordings_dir().join(name.trim());
            let pcm = read_wav_pcm(&path);
            let keyterms = merge_keyterms(Vec::new());
            let (stream_text, batch_text, batch_ms) = rt.block_on(async {
                let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<i16>>();
                let (transcript_tx, mut transcript_rx) = mpsc::unbounded_channel::<TranscriptChunk>();
                let (stop_tx, stop_rx) = mpsc::channel::<()>(1);
                let config = DeepgramConfig { keywords: keyterms.clone(), ..Default::default() };
                let handle = tokio::spawn(stream_audio(config, audio_rx, transcript_tx, stop_rx));
                // Faster than real time: the transcript, not the pacing, is under test here.
                for chunk in pcm.chunks(800) {
                    let _ = audio_tx.send(chunk.to_vec());
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                drop(audio_tx);
                let _ = stop_tx.send(()).await;
                let _ = handle.await;
                let mut stream_text = String::new();
                while let Ok(c) = transcript_rx.try_recv() {
                    if c.is_final {
                        stream_text.push_str(c.text.trim());
                        stream_text.push(' ');
                    }
                }
                let t = Instant::now();
                let batch = crate::stt::batch::transcribe_with_language_and_keyterms(
                    path.to_str().unwrap(),
                    "en",
                    &keyterms,
                )
                .await
                .expect("batch");
                (stream_text.trim().to_string(), batch.text, t.elapsed().as_millis())
            });
            let words = |t: &str| -> Vec<String> {
                t.split_whitespace()
                    .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
                    .filter(|w| !w.is_empty())
                    .collect()
            };
            let (a, b) = (words(&stream_text), words(&batch_text));
            println!(
                "CMP {name}: {:.1}s audio | stream {} words | batch {} words | word edit distance {} | batch took {batch_ms} ms",
                pcm.len() as f64 / 16000.0,
                a.len(),
                b.len(),
                word_edit_distance(&a, &b)
            );
            println!("CMP_STREAM {name}: {stream_text}");
            println!("CMP_BATCH  {name}: {batch_text}");
        }
    }

    /// Streams saved recordings in real time, releases, and times the drain.
    /// Live network + the app's own key lookup; ignored by default.
    /// PROBE_WAVS=2214.wav,2217.wav cargo test --release --bin maxspeech latency_probe -- --ignored --nocapture
    #[test]
    #[ignore]
    fn stream_recording_and_time_finalize() {
        let names = std::env::var("PROBE_WAVS").unwrap_or_else(|_| "2214.wav".into());
        let rt = tokio::runtime::Runtime::new().unwrap();
        for name in names.split(',') {
            let path = crate::recording::recordings_dir().join(name.trim());
            let pcm = read_wav_pcm(&path);
            rt.block_on(async {
                let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<i16>>();
                let (transcript_tx, mut transcript_rx) = mpsc::unbounded_channel::<TranscriptChunk>();
                let (stop_tx, stop_rx) = mpsc::channel::<()>(1);
                let config = DeepgramConfig {
                    keywords: merge_keyterms(Vec::new()),
                    ..Default::default()
                };
                let handle = tokio::spawn(stream_audio(config, audio_rx, transcript_tx, stop_rx));

                let mut tick = tokio::time::interval(Duration::from_millis(50));
                for chunk in pcm.chunks(800) {
                    tick.tick().await;
                    let _ = audio_tx.send(chunk.to_vec());
                }
                // Same order as the app: capture stops (senders drop), then stop signal.
                drop(audio_tx);
                let released = Instant::now();
                let _ = stop_tx.send(()).await;
                let complete = handle.await.unwrap().unwrap_or(false);
                let drain = released.elapsed();

                let mut text = String::new();
                while let Ok(c) = transcript_rx.try_recv() {
                    if c.is_final {
                        text.push_str(c.text.trim());
                        text.push(' ');
                    }
                }
                println!(
                    "PROBE {name}: {:.1}s audio | release->transcript-complete {} ms | complete={complete} | {} chars",
                    pcm.len() as f64 / 16000.0,
                    drain.as_millis(),
                    text.trim().len()
                );
                println!("PROBE_TEXT {name}: {}", text.trim());
            });
        }
    }
}
