//! Text polish / rewrite via OpenAI Chat Completions (GPT-6 Luna).
//!
//! Used for dictation cleanup, the enhancer widget, spoken rewrite, and remake.
//! Prefer this over Deepgram Voice Agent — cheaper and no agent websocket.

use std::time::{Duration, Instant};

use crate::secrets;

const MODEL: &str = "gpt-6-luna";
const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";
const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

struct Endpoint {
    url: &'static str,
    model: &'static str,
    key: String,
}

fn resolve_endpoint() -> Result<Endpoint, BoxErr> {
    if let Some(key) = secrets::resolve_llm_api_key() {
        // OpenRouter keys are typically `sk-or-...`; route them there with the
        // OpenAI-compatible model id.
        if key.starts_with("sk-or-") {
            return Ok(Endpoint {
                url: OPENROUTER_URL,
                model: "openai/gpt-6-luna",
                key,
            });
        }
        return Ok(Endpoint {
            url: OPENAI_URL,
            model: MODEL,
            key,
        });
    }
    Err("LLM API key not set (llm_api_key / MAXSPEECH_LLM_API_KEY / OPENAI_API_KEY)".into())
}

/// Polish / rewrite text. `reasoning_effort: none` keeps Luna cheap and fast.
pub async fn polish(system: &str, user: &str, timeout: Duration) -> Result<String, BoxErr> {
    let endpoint = resolve_endpoint()?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(timeout)
        .build()?;

    let body = serde_json::json!({
        "model": endpoint.model,
        "reasoning_effort": "none",
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ]
    });

    let started = Instant::now();
    let resp = client
        .post(endpoint.url)
        .bearer_auth(&endpoint.key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let json: serde_json::Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        let err = json["error"]["message"]
            .as_str()
            .or_else(|| json["error"].as_str())
            .unwrap_or("OpenAI request failed");
        return Err(format!("OpenAI {status}: {err}").into());
    }

    let text = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    if text.is_empty() {
        return Err("OpenAI returned empty content".into());
    }
    log::info!(
        "OpenAI polish model={} {} ms, {} → {} chars",
        endpoint.model,
        started.elapsed().as_millis(),
        user.len(),
        text.len()
    );
    Ok(text)
}

fn edit_size_ok(input: &str, reply: &str) -> bool {
    let (i, r) = (input.chars().count(), reply.chars().count());
    if r == 0 {
        return false;
    }
    if i < 30 {
        return r <= i * 4 + 20;
    }
    r * 100 >= i * 70 && r * 100 <= i * 200
}

/// Edit one enhancer chunk; retry once if the reply is empty or the wrong size.
pub async fn rewrite_chunk<C>(system: &str, body: &str, is_cancelled: &C) -> Result<String, BoxErr>
where
    C: Fn() -> bool,
{
    let timeout = Duration::from_millis((2_500 + body.len() as u64 * 6).min(8_000));
    let mut last_err: BoxErr = "Empty LLM response".into();
    for _ in 0..2 {
        if is_cancelled() {
            return Err("cancelled".into());
        }
        match polish(system, body, timeout).await {
            Ok(reply) if edit_size_ok(body, &reply) => return Ok(reply),
            Ok(reply) => {
                last_err = format!(
                    "LLM reply off-size ({} → {} chars)",
                    body.chars().count(),
                    reply.chars().count()
                )
                .into();
            }
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

/// Whole-passage rewrite (prompt enhancer / spoken rewrite) — no size guard.
pub async fn rewrite_whole<C>(system: &str, text: &str, is_cancelled: C) -> Result<String, BoxErr>
where
    C: Fn() -> bool,
{
    if is_cancelled() {
        return Err("cancelled".into());
    }
    let timeout = Duration::from_millis((4_000 + text.len() as u64 * 10).min(20_000));
    let out = polish(system, text, timeout).await?;
    if is_cancelled() {
        return Err("cancelled".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live smoke: cargo test --bin maxspeech polish_live -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn polish_live() {
        let system = "You clean dictation transcripts. Output ONLY the cleaned text.";
        let user = "<t>um so I was thinking we could uh meet on Tuesday no wait Wednesday at like 3</t>";
        let t = Instant::now();
        let out = polish(system, user, Duration::from_secs(15))
            .await
            .expect("polish");
        println!("LUNA_LIVE {} ms: {out}", t.elapsed().as_millis());
        assert!(out.to_lowercase().contains("wednesday"));
        assert!(!out.to_lowercase().contains("um"));
    }
}
