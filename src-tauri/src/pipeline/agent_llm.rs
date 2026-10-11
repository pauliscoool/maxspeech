//! Text rewriting through Deepgram's Voice Agent "think" step.
//!
//! MaxSpeech only ships a Deepgram key. Deepgram hosts managed LLMs (OpenAI,
//! Anthropic, ...) behind the Voice Agent socket, so we open a session and inject
//! the text as a user message. The agent normally releases its reply one spoken
//! sentence at a time (seconds of pacing), so the model is told to hand the result
//! to a client-side function instead: the whole text arrives in one
//! FunctionCallRequest as soon as the LLM finishes (~0.6 s). No audio is ever sent
//! (only silence to keep the session alive); the agent's TTS audio is ignored.

use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::Message;

use crate::secrets;

const AGENT_HOST: &str = "agent.deepgram.com";
const AGENT_URL: &str = "wss://agent.deepgram.com/v1/agent/converse";
const THINK_PROVIDER: &str = "open_ai";
/// Fastest faithful model in the 2026-10 probe (~0.65 s, fixes misheard words).
const THINK_MODEL: &str = "gpt-5.4-mini";
const FUNCTION_NAME: &str = "deliver_text";
const SESSION_TIMEOUT: Duration = Duration::from_secs(60);
/// A prewarmed session lives as long as the longest dictation, then closes itself so a
/// forgotten one can never stream silence (and bill) forever.
const PREWARM_TIMEOUT: Duration = Duration::from_secs(150);
/// 120 ms of 16 kHz mono linear16 silence, one per poll tick.
const SILENCE_FRAME_BYTES: usize = 3840;
const DELIVER_INSTRUCTION: &str = "ALWAYS deliver the final text by calling the function deliver_text exactly once, with the complete text as its \"text\" argument. Never speak or write anything else.";
/// Fallback when the model speaks instead of calling the function: this long after the last sentence.
const REPLY_IDLE: Duration = Duration::from_millis(8000);
/// Whole-passage replies arrive at speaking pace; wait longer before assuming the model is done.
const REPLY_IDLE_WHOLE: Duration = Duration::from_millis(15000);

type BoxErr = Box<dyn std::error::Error + Send + Sync>;
type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

async fn connect(key: &str) -> Result<WsStream, BoxErr> {
    let request = Request::builder()
        .uri(AGENT_URL)
        .header("Authorization", format!("Token {key}"))
        .header("Sec-WebSocket-Key", generate_key())
        .header("Sec-WebSocket-Version", "13")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Host", AGENT_HOST)
        .body(())?;
    let (ws, _) = tokio_tungstenite::connect_async(request).await?;
    Ok(ws)
}

fn settings_message(system: &str) -> String {
    serde_json::json!({
        "type": "Settings",
        "audio": {
            "input": { "encoding": "linear16", "sample_rate": 16000 },
            "output": { "encoding": "linear16", "sample_rate": 16000, "container": "none" }
        },
        "agent": {
            "language": "en",
            "listen": { "provider": { "type": "deepgram", "model": "nova-3" } },
            "think": {
                "provider": { "type": THINK_PROVIDER, "model": THINK_MODEL },
                "prompt": format!("{system}\n\n{DELIVER_INSTRUCTION}"),
                "functions": [{
                    "name": FUNCTION_NAME,
                    "description": "Deliver the final edited text.",
                    "parameters": {
                        "type": "object",
                        "properties": { "text": { "type": "string" } },
                        "required": ["text"]
                    }
                }]
            },
            "speak": { "provider": { "type": "deepgram", "model": "aura-2-thalia-en" } }
        }
    })
    .to_string()
}

/// Rewrite `text` under `system`. The reply arrives whole (not token by token);
/// `is_cancelled` is polled ~8x/second and abandons the session early.
pub async fn rewrite<C>(system: &str, text: &str, is_cancelled: C) -> Result<String, BoxErr>
where
    C: Fn() -> bool,
{
    rewrite_with(system, text, is_cancelled, true).await
}

/// Like `rewrite`, but for edits that change the length (shorten, expand, translate, list):
/// the reply is never judged by its size.
pub async fn rewrite_whole<C>(system: &str, text: &str, is_cancelled: C) -> Result<String, BoxErr>
where
    C: Fn() -> bool,
{
    rewrite_with(system, text, is_cancelled, false).await
}

async fn rewrite_with<C>(
    system: &str,
    text: &str,
    is_cancelled: C,
    early_finish: bool,
) -> Result<String, BoxErr>
where
    C: Fn() -> bool,
{
    let mut last_err: BoxErr = "Deepgram key not available".into();
    for key in secrets::deepgram_key_candidates() {
        match connect(&key).await {
            Ok(ws) => {
                let (tx, rx) = oneshot::channel();
                let _ = tx.send(text.to_string());
                return run_session(ws, system, rx, &is_cancelled, early_finish, SESSION_TIMEOUT)
                    .await;
            }
            Err(e) => {
                log::warn!("Deepgram agent connect failed: {e}");
                last_err = e;
            }
        }
    }
    Err(last_err)
}

/// A session opened while the user is still talking, so releasing the key only pays for the LLM.
/// Dropping it closes the socket.
pub struct AgentSession {
    text_tx: Option<oneshot::Sender<String>>,
    done: tauri::async_runtime::JoinHandle<Result<String, BoxErr>>,
}

/// Open a session for `system` in the background and wait for text via `AgentSession::run`.
pub fn prewarm(system: String) -> AgentSession {
    let (text_tx, text_rx) = oneshot::channel::<String>();
    let done = tauri::async_runtime::spawn(async move {
        let mut last_err: BoxErr = "Deepgram key not available".into();
        let mut text_rx = Some(text_rx);
        for key in secrets::deepgram_key_candidates() {
            match connect(&key).await {
                Ok(ws) => {
                    let rx = text_rx.take().expect("receiver used once");
                    return run_session(ws, &system, rx, &|| false, true, PREWARM_TIMEOUT).await;
                }
                Err(e) => {
                    log::warn!("Deepgram agent connect failed: {e}");
                    last_err = e;
                }
            }
        }
        Err(last_err)
    });
    AgentSession { text_tx: Some(text_tx), done }
}

impl AgentSession {
    /// Hand over the transcript and wait up to `timeout` for the model's text.
    pub async fn run(mut self, text: &str, timeout: Duration) -> Result<String, BoxErr> {
        if let Some(tx) = self.text_tx.take() {
            let _ = tx.send(text.to_string());
        }
        match tokio::time::timeout(timeout, &mut self.done).await {
            Ok(Ok(result)) => result,
            Ok(Err(join)) => Err(format!("Agent task failed: {join}").into()),
            Err(_) => Err("Deepgram agent timed out".into()),
        }
    }
}

impl Drop for AgentSession {
    fn drop(&mut self) {
        self.done.abort();
    }
}

async fn run_session<C>(
    mut ws: WsStream,
    system: &str,
    text_rx: oneshot::Receiver<String>,
    is_cancelled: &C,
    early_finish: bool,
    session_timeout: Duration,
) -> Result<String, BoxErr>
where
    C: Fn() -> bool,
{
    let deadline = Instant::now() + session_timeout;
    let mut text_rx = Some(text_rx);
    let mut input_chars = 0usize;
    let mut out = String::new();
    let mut last_chunk_at: Option<Instant> = None;
    let mut listening = false;
    let idle_limit = if early_finish { REPLY_IDLE } else { REPLY_IDLE_WHOLE };

    let result: Result<(), BoxErr> = loop {
        if is_cancelled() {
            break Ok(());
        }
        if Instant::now() > deadline {
            break Err("Deepgram agent timed out".into());
        }
        let waiting_for_text = async {
            match text_rx.as_mut() {
                Some(rx) if listening => Some(rx.await),
                _ => std::future::pending().await,
            }
        };
        let msg = tokio::select! {
            incoming = waiting_for_text => {
                text_rx = None;
                match incoming {
                    Some(Ok(text)) => {
                        input_chars = text.chars().count();
                        let inject = serde_json::json!({
                            "type": "InjectUserMessage",
                            "content": text
                        });
                        if let Err(e) = ws.send(Message::Text(inject.to_string().into())).await {
                            break Err(e.into());
                        }
                        continue;
                    }
                    // Sender dropped: the dictation was cancelled or produced nothing.
                    _ => break Ok(()),
                }
            }
            next = tokio::time::timeout(Duration::from_millis(120), ws.next()) => match next {
                Err(_) => {
                    if last_chunk_at.is_some_and(|t| t.elapsed() > idle_limit) {
                        break Ok(());
                    }
                    // The agent drops a session that hears no audio for ~12s, so stream silence.
                    if listening
                        && ws
                            .send(Message::Binary(vec![0u8; SILENCE_FRAME_BYTES].into()))
                            .await
                            .is_err()
                    {
                        break Ok(());
                    }
                    continue;
                }
                Ok(None) => break Ok(()),
                Ok(Some(Err(e))) => break Err(e.into()),
                Ok(Some(Ok(m))) => m,
            }
        };
        match msg {
            Message::Text(raw) => {
                let v: serde_json::Value = match serde_json::from_str(&raw) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!("Agent message parse error: {e}");
                        continue;
                    }
                };
                match v["type"].as_str().unwrap_or_default() {
                    "Welcome" => {
                        if let Err(e) = ws.send(Message::Text(settings_message(system).into())).await {
                            break Err(e.into());
                        }
                    }
                    "SettingsApplied" => listening = true,
                    "FunctionCallRequest" => {
                        let delivered = v["functions"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|f| f["name"] == FUNCTION_NAME)
                            .find_map(|f| {
                                let args: serde_json::Value =
                                    serde_json::from_str(f["arguments"].as_str()?).ok()?;
                                Some(args["text"].as_str()?.to_string())
                            });
                        if let Some(text) = delivered {
                            out = text;
                            break Ok(());
                        }
                    }
                    // The model spoke instead of calling the function: collect its sentences.
                    "ConversationText" if v["role"] == "assistant" => {
                        if let Some(piece) = v["content"].as_str() {
                            if !out.is_empty() && !out.ends_with('\n') {
                                out.push(' ');
                            }
                            out.push_str(piece);
                            last_chunk_at = Some(Instant::now());
                            // An edit is about as long as its input, so a reply that size is
                            // complete — no need to wait out the rest of the agent's speech.
                            let reply_chars = out.trim().chars().count();
                            if early_finish && input_chars > 0 && reply_chars * 100 >= input_chars * 85 {
                                break Ok(());
                            }
                        }
                    }
                    "Error" => {
                        let desc = v["description"]
                            .as_str()
                            .or_else(|| v["message"].as_str())
                            .unwrap_or("Deepgram agent error");
                        break Err(desc.to_string().into());
                    }
                    "Warning" => log::warn!("Deepgram agent warning: {v}"),
                    _ => {}
                }
            }
            Message::Close(_) => break Ok(()),
            _ => {}
        }
    };

    let _ = ws.close(None).await;
    result?;
    let out = out.trim().to_string();
    if out.is_empty() && !is_cancelled() {
        return Err("Empty LLM response".into());
    }
    Ok(out)
}

/// One sentence per chunk (shorter ones merge forward past this many chars).
/// The agent releases a reply sentence by sentence at speaking pace, and long
/// inputs get condensed or answered instead of edited, so small chunks in
/// parallel are both faster and more faithful.
const CHUNK_TARGET: usize = 30;
/// Never let a chunk grow past this when the next sentence would overshoot it.
const CHUNK_MAX: usize = 220;

fn floor_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Sentences of one line, force-splitting long unpunctuated runs at a space.
fn split_sentences(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = line.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let ends = matches!(c, '.' | '!' | '?' | '…')
            && chars.peek().map_or(true, |(_, n)| n.is_whitespace());
        if ends {
            let end = i + c.len_utf8();
            out.push(line[start..end].trim());
            start = end;
        }
    }
    if start < line.len() {
        out.push(line[start..].trim());
    }

    let mut pieces = Vec::new();
    for seg in out.into_iter().filter(|s| !s.is_empty()) {
        split_run_on(seg, &mut pieces);
    }
    pieces
}

/// Sentences longer than this are split at a natural break so each piece stays quick.
const RUN_ON_SPLIT_AT: usize = 130;
/// Neither half of a split may be shorter than this many bytes.
const RUN_ON_MIN_HALF: usize = 35;

/// Break a long run-on near its middle at a comma or conjunction (falling back to
/// a space), recursively, so dictated walls of text become short editable pieces.
fn split_run_on<'a>(seg: &'a str, out: &mut Vec<&'a str>) {
    if seg.len() <= RUN_ON_SPLIT_AT {
        out.push(seg);
        return;
    }
    let mid = seg.len() / 2;
    let mut best: Option<(usize, usize, usize)> = None; // (distance, left_end, right_start)
    let mut candidates: Vec<(usize, usize)> = Vec::new(); // (left_end, right_start)
    for (i, _) in seg.match_indices(", ") {
        candidates.push((i + 1, i + 2));
    }
    for word in [" and ", " but ", " so ", " because ", " which ", " cuz ", " then ", " or "] {
        for (i, _) in seg.match_indices(word) {
            candidates.push((i, i + 1));
        }
    }
    if candidates.is_empty() {
        let limit = floor_boundary(seg, mid);
        if let Some(sp) = seg[..limit].rfind(' ') {
            candidates.push((sp, sp + 1));
        }
    }
    for (left_end, right_start) in candidates {
        if left_end >= RUN_ON_MIN_HALF && seg.len() - right_start >= RUN_ON_MIN_HALF {
            let dist = left_end.abs_diff(mid);
            if best.map_or(true, |(d, _, _)| dist < d) {
                best = Some((dist, left_end, right_start));
            }
        }
    }
    match best {
        Some((_, left_end, right_start)) => {
            split_run_on(seg[..left_end].trim(), out);
            split_run_on(seg[right_start..].trim(), out);
        }
        None => out.push(seg),
    }
}

/// Split text into (body, separator) pairs. Bodies are what the model edits;
/// separators (spaces / line breaks) are restored untouched so layout survives.
pub fn split_chunks(text: &str) -> Vec<(String, String)> {
    let mut chunks: Vec<(String, String)> = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let (line, after) = match rest.find('\n') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        let sep_len = after.len() - after.trim_start_matches(['\n', '\r']).len();
        let line_sep = &after[..sep_len];
        rest = &after[sep_len..];

        if line.trim().is_empty() {
            if let Some(last) = chunks.last_mut() {
                last.1.push_str(line_sep);
            }
            continue;
        }

        let mut cur = String::new();
        for sentence in split_sentences(line) {
            if !cur.is_empty() && cur.chars().count() + sentence.chars().count() > CHUNK_MAX {
                chunks.push((std::mem::take(&mut cur), " ".to_string()));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(sentence);
            if cur.chars().count() >= CHUNK_TARGET {
                chunks.push((std::mem::take(&mut cur), " ".to_string()));
            }
        }
        if !cur.is_empty() {
            chunks.push((cur, " ".to_string()));
        }
        if let Some(last) = chunks.last_mut() {
            last.1 = line_sep.to_string();
        }
    }
    chunks
}

/// The reply must be roughly the input's size — anything else is the model
/// answering, summarizing, or inventing rather than editing.
/// Reply length bounds as a percentage of the input's length.
#[derive(Clone, Copy)]
pub struct SizeBounds {
    pub min_pct: usize,
    pub max_pct: usize,
}

impl SizeBounds {
    /// A plain grammar / tone edit stays about as long as the original.
    pub const EDIT: Self = Self { min_pct: 70, max_pct: 200 };
}

fn size_ok(input: &str, reply: &str, bounds: SizeBounds) -> bool {
    let (i, r) = (input.chars().count(), reply.chars().count());
    if r == 0 {
        return false;
    }
    if i < 30 {
        return r <= i * 4 + 20;
    }
    r * 100 >= i * bounds.min_pct && r * 100 <= i * bounds.max_pct
}

/// Edit one chunk, retrying once when the reply is missing or the wrong size.
pub async fn rewrite_chunk<C>(system: &str, body: &str, is_cancelled: &C) -> Result<String, BoxErr>
where
    C: Fn() -> bool,
{
    rewrite_chunk_bounded(system, body, is_cancelled, SizeBounds::EDIT).await
}

pub async fn rewrite_chunk_bounded<C>(
    system: &str,
    body: &str,
    is_cancelled: &C,
    bounds: SizeBounds,
) -> Result<String, BoxErr>
where
    C: Fn() -> bool,
{
    let mut last_err: BoxErr = "Empty LLM response".into();
    for _ in 0..2 {
        if is_cancelled() {
            return Ok(String::new());
        }
        let res = rewrite(system, body, is_cancelled).await;
        match res {
            Ok(reply) => {
                let reply = reply.trim().to_string();
                if is_cancelled() || size_ok(body, &reply, bounds) {
                    return Ok(reply);
                }
                log::warn!(
                    "Agent reply off-size ({} → {} chars), retrying",
                    body.chars().count(),
                    reply.chars().count()
                );
                last_err = "Reply did not match the input".into();
            }
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

#[cfg(test)]
mod tests {
    use super::split_chunks;

    const LONG: &str = "hey team so i wanted to give everyone a quick update on where we are with the launch. first off the backend is basically done we just have a few bugs left in the payment flow that im hoping to squash by tuesday. the design team sent over the new landing page yesterday and honestly its looking really good but theres a couple of things i think we should change like the pricing table is kinda confusing and the button colors dont match the brand. also we still havent heard back from legal about the terms of service which is a bit of a problem cuz we cant launch without it. i talked to sarah about it on friday and she said shed follow up but i havent seen anything yet. on the marketing side were planning to send out the email campaign the week of the 15th and we need the final copy by then so if anyone has feedback please get it to me asap. one more thing the customer support docs are way out of date and nobody has time to fix them so were gonna need to figure out who can take that on. anyway thats about it let me know if you have any questions and lets try to get everything wrapped up before the end of the month.";

    fn words(t: &str) -> Vec<String> {
        t.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn split_keeps_words_and_paragraph_breaks() {
        let text = "first sentence here. second one is a bit longer, and it keeps going because dictation never stops talking about things\n\nnew paragraph? yes! ok";
        let chunks = split_chunks(text);
        let joined: String = chunks.iter().map(|(b, s)| format!("{b}{s}")).collect();
        assert_eq!(words(&joined), words(text));
        assert!(chunks.iter().any(|(_, sep)| sep == "\n\n"));
    }

    #[test]
    fn split_makes_short_chunks_from_long_dictation() {
        let chunks = split_chunks(LONG);
        let joined: String = chunks.iter().map(|(b, s)| format!("{b}{s}")).collect();
        assert_eq!(words(&joined), words(LONG));
        assert!(chunks.iter().all(|(b, _)| b.len() <= 220), "chunk too long");
    }

    #[test]
    fn split_handles_unicode_and_empty() {
        assert!(split_chunks("").is_empty());
        assert!(split_chunks("  \n\n ").is_empty());
        let text = "привет мир это очень длинное предложение без знаков препинания которое нужно разрезать на несколько частей потому что оно слишком длинное для одного запроса";
        let joined: String = split_chunks(text).iter().map(|(b, s)| format!("{b}{s}")).collect();
        assert_eq!(words(&joined), words(text));
    }

    /// Live check of the coding-agent prompt modes. Run with:
    /// cargo test agent_live_prompt -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn agent_live_prompt() {
        let dictated = "so um the login page is broken after the session times out like you get a blank screen instead of going back to the sign in form I think its something in the token refresh in the auth folder can you fix it and make sure the tests still pass";
        for target in ["claude_code", "cursor_codex"] {
            let started = std::time::Instant::now();
            let out = crate::pipeline::tone::stream_enhance_prompt(dictated, target, &[], |_| {}, || false)
                .await
                .expect("prompt rewrite failed");
            println!("--- {target} [{:.2}s]\n{out}\n", started.elapsed().as_secs_f32());
            assert!(!out.is_empty());
        }
    }

    /// Live check against Deepgram. Run with:
    /// cargo test agent_live -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn agent_live_rewrite() {
        let started = std::time::Instant::now();
        let out = crate::pipeline::tone::stream_enhance_selection(
            LONG,
            "professional",
            &[],
            |_| {},
            || false,
        )
        .await
        .expect("agent rewrite failed");
        println!("[{:.2}s] IN {} / OUT {} chars\n{out}", started.elapsed().as_secs_f32(), LONG.len(), out.len());
        assert!(out.len() as f64 > LONG.len() as f64 * 0.8, "reply was condensed");
    }
}

#[cfg(test)]
mod cleanup_live {
    use crate::pipeline::tone::{cleanup_system_prompt, CleanupContext};

    /// Live check of the prewarmed cleanup session. Run with:
    /// cargo test cleanup_live -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn prewarmed_cleanup_latency() {
        let system = cleanup_system_prompt(&CleanupContext {
            tone: "casual",
            app_name: "Discord",
            window_title: "minecraft-mods - Discord",
            dictionary: &["CurseForge".to_string()],
            multilingual: false,
        });
        let cases = [
            "um so you should download the mom or retrospect from curseforge",
            "okay so for the trip I need to pack first my passport second the charger and third some snacks",
            "lets meet thursday no wait friday at like 3",
        ];
        for text in cases {
            let session = super::prewarm(system.clone());
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            let started = std::time::Instant::now();
            let out = session
                .run(&format!("<t>{text}</t>"), std::time::Duration::from_secs(9))
                .await
                .expect("cleanup failed");
            println!("[{} ms] {text}\n  -> {out}\n", started.elapsed().as_millis());
        }
    }
}
