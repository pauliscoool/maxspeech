use crate::context::ForegroundApp;
use crate::secrets;
use crate::store::Store;

pub fn get_tone_for_app(app: &ForegroundApp, store: &Store) -> Option<String> {
    let profiles = store.get_app_profiles().unwrap_or_default();
    for profile in &profiles {
        if !profile.enabled {
            continue;
        }
        let exe_match = app
            .exe
            .to_lowercase()
            .contains(&profile.exe_pattern.to_lowercase());
        let title_match = profile.title_pattern.is_empty()
            || app
                .title
                .to_lowercase()
                .contains(&profile.title_pattern.to_lowercase());
        if exe_match && title_match {
            return Some(profile.tone.clone());
        }
    }
    None
}

const SELF_CORRECTION_RULES: &str = "\
CRITICAL — spoken self-corrections (highest priority): \
The speaker often changes their mind mid-sentence. Detect phrases like: \
'oh no I meant', 'I meant', 'I mean', 'wait actually', 'no wait', 'scratch that', \
'correction:', 'wait no', 'or rather', 'sorry I meant'. \
Keep ONLY the final intended meaning. DELETE the mistaken word/phrase AND all \
correction chatter. \
\
Examples (input → output): \
1) 'Would you like to go on a trip on Tuesday? Oh no I meant Monday' \
   → 'Would you like to go on a trip on Monday?' \
2) 'Meet me at 3pm wait I meant 4pm' \
   → 'Meet me at 4pm' \
3) 'Send it to Sarah I mean Sandra' \
   → 'Send it to Sandra' \
4) 'The meeting is tomorrow no wait Friday' \
   → 'The meeting is Friday' \
Never leave both the mistake and the correction in the output.";

const GRAMMAR_RULES: &str = "\
Grammarly-style cleanup (always apply): \
- Fix grammar, subject-verb agreement, articles (a/an/the), and awkward phrasing. \
- Fix punctuation: commas, periods, question marks, apostrophes, quotes. \
- Capitalize sentence starts and proper nouns; fix obvious misspellings from speech. \
- Remove filler (um, uh, like, you know) when they add no meaning. \
- Improve clarity lightly — tighten run-ons — but KEEP the speaker's meaning, voice, \
  and intent. Do NOT invent facts, summarize, or change names/numbers/dates. \
- Do NOT add a greeting/sign-off the speaker did not say. \
- Return ONLY the cleaned text, no commentary or quotes around it.";

fn system_prompt_for_tone(tone: &str) -> String {
    let base = match tone {
        "casual" => {
            "You are a Grammarly-like dictation assistant. Rewrite in a casual, terse chat style. \
             Prefer lowercase; skip a trailing period. Keep it brief. \
             Still fix grammar/clarity so it reads cleanly as a message."
        }
        "formal" => {
            "You are a Grammarly-like dictation assistant. Rewrite in a professional, formal style \
             suitable for email: proper capitalization, punctuation, and complete sentences."
        }
        "code" => {
            "You are a Grammarly-like dictation assistant for a programmer. Clean up grammar and \
             use precise technical terms. If it sounds like a code comment, format it as one."
        }
        "prose" => {
            "You are a Grammarly-like dictation assistant. Rewrite as clean prose with proper \
             paragraphs, punctuation, and grammar."
        }
        _ => {
            "You are a Grammarly-like dictation assistant. Clean up grammar, punctuation, and \
             clarity while keeping the original meaning and style."
        }
    };
    format!("{base}\n\n{GRAMMAR_RULES}\n\n{SELF_CORRECTION_RULES}")
}

/// Local heuristic: fix "… Tuesday oh no I meant Monday" without needing an LLM.
pub fn local_self_correct(text: &str) -> String {
    let markers = [
        "oh no i meant ",
        "oh no, i meant ",
        "oh wait i meant ",
        "oh wait, i meant ",
        "no wait i meant ",
        "no, i meant ",
        "no i meant ",
        "wait i meant ",
        "wait, i meant ",
        "actually i meant ",
        "sorry i meant ",
        "scratch that i meant ",
        "correction: ",
        "correction ",
        "i meant ",
        "i mean ",
        "or rather ",
    ];

    let lower = text.to_lowercase();
    let mut best: Option<(usize, usize)> = None; // (byte start, marker len)

    for m in markers {
        if let Some(idx) = lower.rfind(m) {
            let end = idx + m.len();
            match best {
                None => best = Some((idx, m.len())),
                Some((bi, bl)) => {
                    let bend = bi + bl;
                    // Prefer the match that ends latest; on a tie, prefer the longer marker
                    // so "oh no i meant" wins over nested "i meant".
                    if end > bend || (end == bend && m.len() > bl) {
                        best = Some((idx, m.len()));
                    }
                }
            }
        }
    }

    // Also: "… no wait Friday" / "… wait no Friday" where correction is the rest
    let alt_markers = [" no wait ", " wait no ", " wait actually "];
    for m in alt_markers {
        if let Some(idx) = lower.rfind(m) {
            let end = idx + m.len();
            match best {
                None => best = Some((idx, m.len())),
                Some((bi, bl)) => {
                    let bend = bi + bl;
                    if end > bend || (end == bend && m.len() > bl) {
                        best = Some((idx, m.len()));
                    }
                }
            }
        }
    }

    let Some((idx, mlen)) = best else {
        return text.to_string();
    };

    // Need char-safe slicing via the same indices on original (ASCII markers only)
    if !text.is_char_boundary(idx) || !text.is_char_boundary(idx + mlen) {
        return text.to_string();
    }

    let before = text[..idx].trim_end();
    let mut correction = text[idx + mlen..].trim();
    correction = correction.trim_end_matches(|c: char| matches!(c, '.' | '!' | '?' | ','));
    correction = correction.trim();

    if before.is_empty() || correction.is_empty() {
        return text.to_string();
    }

    // Replace the last "content" word in `before` with the correction.
    let (stem, trailing_punct) = strip_trailing_punct(before);
    let mut words: Vec<String> = stem.split_whitespace().map(|w| w.to_string()).collect();
    if words.is_empty() {
        return format!("{correction}{trailing_punct}");
    }

    // If correction is multi-word, replace last N words; else replace last word.
    let corr_words: Vec<&str> = correction.split_whitespace().collect();
    let n = corr_words.len().min(words.len());
    words.truncate(words.len() - n);
    for w in corr_words {
        words.push(w.to_string());
    }

    let mut out = words.join(" ");
    out.push_str(trailing_punct);
    // Prefer a sentence-ending mark if the original had one after the mistake
    if trailing_punct.is_empty() && before.ends_with(['.', '?', '!']) {
        // already handled
    }
    out
}

fn strip_trailing_punct(s: &str) -> (&str, &str) {
    let trimmed = s.trim_end();
    let bytes = trimmed.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        let c = bytes[i - 1] as char;
        if matches!(c, '.' | '!' | '?' | ',' | ';' | ':') {
            i -= 1;
        } else {
            break;
        }
    }
    if i == bytes.len() {
        (trimmed, "")
    } else {
        (&trimmed[..i], &trimmed[i..])
    }
}

fn llm_key() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    secrets::resolve_llm_api_key().ok_or_else(|| "LLM API key not set".into())
}

pub async fn apply_tone(
    text: &str,
    tone: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = llm_key()?;
    let system = system_prompt_for_tone(tone);
    call_llm(&api_key, &system, text, 1024).await
}

pub async fn rewrite_with_llm(
    text: &str,
    instruction: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = llm_key()?;

    let system = format!(
        "You are a Grammarly-like dictation assistant. Rewrite the text per the instruction. \
         Instruction: {instruction}. Only return the rewritten text, nothing else.\n\n\
         {GRAMMAR_RULES}\n\n{SELF_CORRECTION_RULES}"
    );
    call_llm(&api_key, &system, text, 1024).await
}

/// Stronger AI pass for longer dictations (up to ~2 minutes of speech).
pub async fn enhance_long_dictation(
    text: &str,
    tone: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = llm_key()?;

    let style = system_prompt_for_tone(tone);
    let system = format!(
        "{style}\n\nThis is a longer dictation. Apply Grammarly-style grammar, punctuation, \
         and clarity fixes throughout. Remove filler (um, uh, like). Break into clear paragraphs \
         when natural. Apply self-correction rules carefully. Do not summarize — return the full \
         cleaned transcript only."
    );
    call_llm(&api_key, &system, text, 4096).await
}

/// Dedicated pass whose only job is cleanup + self-corrections.
pub async fn cleanup_self_corrections(
    text: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = llm_key()?;

    let system = format!(
        "You are a Grammarly-like cleanup pass for spoken dictation. \
         {GRAMMAR_RULES} {SELF_CORRECTION_RULES} \
         Only return the cleaned text, nothing else."
    );
    call_llm(&api_key, &system, text, 2048).await
}

/// Full enhance path used by the dictation pipeline.
pub async fn enhance_dictation(
    text: &str,
    tone: &str,
    long: bool,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Local pass first so corrections work even if the API call fails
    let local = local_self_correct(text);
    if long {
        enhance_long_dictation(&local, tone).await
    } else if tone == "default" {
        // Default = cleanup + self-corrections (not a heavy rewrite)
        cleanup_self_corrections(&local).await
    } else {
        apply_tone(&local, tone).await
    }
}

async fn call_llm(
    api_key: &str,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": "gpt-4o-mini",
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ],
        "max_tokens": max_tokens,
        "temperature": 0.1
    });

    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let json: serde_json::Value = resp.json().await?;

    if !status.is_success() {
        let err = json["error"]["message"]
            .as_str()
            .unwrap_or("OpenAI request failed");
        log::error!("LLM error ({status}): {err}");
        return Err(err.into());
    }

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("Empty LLM response")?
        .trim()
        .trim_matches('"')
        .to_string();

    if content.is_empty() {
        return Err("Empty LLM response".into());
    }

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::local_self_correct;

    #[test]
    fn corrects_tuesday_to_monday() {
        let out = local_self_correct(
            "Would you like to go on a trip on Tuesday? Oh no I meant Monday",
        );
        assert!(out.to_lowercase().contains("monday"), "{out}");
        assert!(!out.to_lowercase().contains("tuesday"), "{out}");
        assert!(!out.to_lowercase().contains("meant"), "{out}");
    }

    #[test]
    fn corrects_i_mean() {
        let out = local_self_correct("Send it to Sarah I mean Sandra");
        assert!(out.to_lowercase().contains("sandra"), "{out}");
        assert!(!out.to_lowercase().contains("sarah"), "{out}");
    }
}
