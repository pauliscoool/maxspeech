use crate::context::ForegroundApp;
use crate::secrets;
use crate::store::Store;

pub fn get_tone_for_app(app: &ForegroundApp, store: &Store) -> Option<String> {
    let profiles = store.get_app_profiles().unwrap_or_default();
    let exe = app.exe.to_lowercase();
    let title = app.title.to_lowercase();

    // Prefer title-specific rules over bare-exe wildcards, then longer titles.
    let mut best: Option<(usize, &str)> = None;
    for profile in &profiles {
        if !profile.enabled {
            continue;
        }
        let pat_exe = profile.exe_pattern.to_lowercase();
        if pat_exe.is_empty() || !exe.contains(&pat_exe) {
            continue;
        }
        let pat_title = profile.title_pattern.to_lowercase();
        let title_ok = pat_title.is_empty() || title.contains(&pat_title);
        if !title_ok {
            continue;
        }
        // Score: titled matches beat empty title; longer title beats shorter.
        let score = if pat_title.is_empty() {
            0
        } else {
            1_000 + pat_title.len()
        };
        match best {
            Some((best_score, _)) if score <= best_score => {}
            _ => best = Some((score, profile.tone.as_str())),
        }
    }
    best.map(|(_, tone)| tone.to_string())
}

const SELF_CORRECTION_RULES: &str = "\
CRITICAL — spoken self-corrections (highest priority): \
The speaker often changes their mind mid-sentence. Detect phrases like: \
'oh no I meant', 'I meant', 'wait actually', 'no wait', 'scratch that', \
'correction:', 'wait no', 'or rather', 'sorry I meant'. \
Treat short 'I mean <replacement>' as a correction (e.g. a name). \
Do NOT treat discourse filler 'I mean …' (e.g. 'I mean it can slip') as a correction. \
Keep ONLY the final intended meaning. DELETE the mistaken word/phrase AND all \
correction chatter. When correcting a weekday/date, replace that weekday wherever \
it appears earlier in the sentence — not only the last few words. \
Never delete weekdays or phrases like 'through Tuesday' unless a clear correction \
replaces them. \
\
Examples (input → output): \
1) 'Would you like to go on a trip on Tuesday? Oh no I meant Monday' \
   → 'Would you like to go on a trip on Monday?' \
2) 'for Tuesday would you like to go on a trip? Oh no I meant Monday' \
   → 'for Monday would you like to go on a trip?' \
3) 'Meet me at 3pm wait I meant 4pm' \
   → 'Meet me at 4pm' \
4) 'Send it to Sarah I mean Sandra' \
   → 'Send it to Sandra' \
5) 'The deadline is through Tuesday I mean it can slip' \
   → 'The deadline is through Tuesday I mean it can slip' (unchanged — discourse) \
6) 'The meeting is tomorrow no wait Friday' \
   → 'The meeting is Friday' \
Never leave both the mistake and the correction in the output.";

const GRAMMAR_RULES: &str = "\
Grammarly-style cleanup (always apply): \
- Fix grammar, subject-verb agreement, articles (a/an/the), and awkward phrasing. \
- Fix punctuation: commas, periods, question marks, apostrophes, quotes. \
- Terminal punctuation: when the utterance is a finished statement, end with a period. \
  Never leave a trailing comma or semicolon on a completed sentence (ASR often does that). \
  Keep '?' for questions and '!' for exclamations. Do not force a period on fragments, \
  lists mid-thought, or text that clearly continues (ends with ':' or an ellipsis). \
- Capitalize sentence starts and proper nouns; fix obvious misspellings from speech. \
- Remove filler (um, uh, like, you know) when they add no meaning. \
- Improve clarity lightly — tighten run-ons — but KEEP the speaker's meaning, voice, \
  and intent. Do NOT invent facts, summarize, or change names/numbers/dates. \
- Do NOT add a greeting/sign-off the speaker did not say. \
- Return ONLY the cleaned text, no commentary or quotes around it.";

const ASR_CORRECTION_RULES: &str = "\
CRITICAL — speech-to-text errors (high priority): \
The input is an ASR transcript and often contains wrong near-homophones. \
Using surrounding context, fix obvious mishears to the word the speaker clearly meant. \
Prefer the reading that makes the sentence sensible. Examples: \
- Git / version control (VERY common): 'get'→'Git' when talking about repos, \
  push/pull/commit/clone/merge/place/branch. \
  'Did you place it to get?' → 'Did you place it to Git?' \
  'push it to get' → 'push it to Git'; 'get hub' → 'GitHub' \
- tech/cloud: 'clout'→'cloud', 'a WS'→'AWS', 'verse cell'→'Vercel', \
  'type script'→'TypeScript', 'post grass'→'Postgres' \
- product names: Deepgram, Claude, ChatGPT, Notion, Slack, CurseForge \
  (do NOT rewrite 'curse forge' / 'CurseForge' to 'Cursor' — different product) \
  Cursor only when clearly the editor/IDE, not gaming/modding context \
  (do NOT assume the speaker means the MaxSpeech app itself unless truly unambiguous — \
  names like 'Maximus Dev' or similar-sounding phrases are NOT the app name) \
- common: 'there'/'their'/'they're', 'to'/'too'/'two', 'its'/'it's' by grammar \
- numbers: ASR often inserts digits for homophones ('for'→'4', 'to'→'2', 'won'→'1'). \
  Prefer the word that fits the sentence; only use digits when the speaker clearly \
  dictated a number, code, time, or quantity (e.g. 'meet at 4pm', 'room 101'). \
  In normal prose keep spelled-out numbers as words unless obviously numeric. \
Do NOT invent new content. Only swap clearly wrong ASR tokens. \
Do NOT change ordinary English 'get' ('I want to get coffee'). \
If both readings are plausible, keep the transcript as-is.";

const MULTILINGUAL_RULES: &str = "\
CRITICAL — multilingual / code-switched dictation: \
The transcript may mix languages in one utterance (e.g. Russian then English). \
- Preserve EVERY language and script exactly as spoken. \
- Do NOT translate between languages. \
- Do NOT transliterate Cyrillic, CJK, Arabic, etc. into Latin letters. \
- Do NOT drop words from a language you understand less well. \
- Only lightly fix punctuation/spacing; leave mixed-language wording intact. \
- English self-correction rules apply only to clearly English correction chatter.";

fn system_prompt_for_tone(tone: &str, multilingual: bool) -> String {
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
    if multilingual {
        format!(
            "{base}\n\n{GRAMMAR_RULES}\n\n{ASR_CORRECTION_RULES}\n\n{SELF_CORRECTION_RULES}\n\n{MULTILINGUAL_RULES}"
        )
    } else {
        format!("{base}\n\n{GRAMMAR_RULES}\n\n{ASR_CORRECTION_RULES}\n\n{SELF_CORRECTION_RULES}")
    }
}

fn dictionary_prompt_block(terms: &[String]) -> String {
    let cleaned: Vec<&str> = terms
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .take(60)
        .collect();
    if cleaned.is_empty() {
        return String::new();
    }
    format!(
        "\n\nPreferred vocabulary (spell and capitalize exactly when the user says these): {}.",
        cleaned.join(", ")
    )
}

fn with_dictionary(system: String, terms: &[String]) -> String {
    let block = dictionary_prompt_block(terms);
    if block.is_empty() {
        system
    } else {
        format!("{system}{block}")
    }
}

/// True when the transcript likely contains non-Latin script (Cyrillic, CJK, Arabic, etc.).
pub fn has_non_latin_script(text: &str) -> bool {
    text.chars().any(|c| {
        let n = c as u32;
        // Cyrillic, Greek, Hebrew, Arabic, Devanagari, CJK, Hangul, Thai, etc.
        (0x0400..=0x04FF).contains(&n)
            || (0x0500..=0x052F).contains(&n)
            || (0x0370..=0x03FF).contains(&n)
            || (0x0590..=0x05FF).contains(&n)
            || (0x0600..=0x06FF).contains(&n)
            || (0x0900..=0x097F).contains(&n)
            || (0x0E00..=0x0E7F).contains(&n)
            || (0x3040..=0x30FF).contains(&n)
            || (0x3400..=0x9FFF).contains(&n)
            || (0xAC00..=0xD7AF).contains(&n)
    })
}

const WEEKDAYS: &[&str] = &[
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "mon",
    "tue",
    "tues",
    "wed",
    "thu",
    "thur",
    "thurs",
    "fri",
    "sat",
    "sun",
];

fn bare_alpha(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphabetic())
        .collect::<String>()
        .to_lowercase()
}

fn word_is_weekday(word: &str) -> bool {
    WEEKDAYS.contains(&bare_alpha(word).as_str())
}

fn looks_like_discourse_filler(correction: &str) -> bool {
    let words: Vec<&str> = correction.split_whitespace().collect();
    if words.is_empty() {
        return true;
    }
    // Real replacements are short (name, day, time). Long clauses after "I mean" are filler.
    if words.len() > 3 {
        return true;
    }
    let first = bare_alpha(words[0]);
    matches!(
        first.as_str(),
        "it" | "that" | "because" | "we" | "you" | "i" | "the" | "this" | "there"
            | "he" | "she" | "they" | "weve" | "im" | "its" | "thats"
    )
}

fn apply_weekday_correction(words: &mut Vec<String>, corr_words: &[&str]) -> bool {
    let Some(corr_day_i) = corr_words.iter().rposition(|w| word_is_weekday(w)) else {
        return false;
    };
    let Some(mistaken_i) = words.iter().rposition(|w| word_is_weekday(w)) else {
        return false;
    };

    // Preserve original casing style lightly: keep replacement text as spoken.
    words[mistaken_i] = corr_words[corr_day_i].to_string();

    // If correction includes a leading prep (on/for/through/this/next), and the
    // mistaken day also has one, leave the existing prep alone — only the day swaps.
    // If correction is just the day, we're done.
    let _ = corr_day_i;
    true
}

/// Fix weak ASR endings so finished dictation doesn't land with a trailing comma.
/// Keeps `?` / `!` / `.` / `…`, leaves casual tone without forcing a new period,
/// and skips short fragments that don't look like full sentences.
pub fn normalize_terminal_punctuation(text: &str, tone: &str) -> String {
    let s = text.trim_end();
    if s.is_empty() {
        return String::new();
    }

    let last = s.chars().last().unwrap_or('\0');

    // Already a strong sentence end (including ellipsis / multi-char …).
    if matches!(last, '.' | '!' | '?' | '…') || s.ends_with("...") {
        return s.to_string();
    }

    // Closing quotes/brackets after content — leave alone if already punctuated inside.
    if matches!(last, '"' | '\'' | ')' | ']' | '}') {
        return s.to_string();
    }

    // Intro / list lead-in — not a finished sentence.
    if last == ':' {
        return s.to_string();
    }

    // Deepgram often ends a held utterance with "," or ";" — treat as a full stop.
    if matches!(last, ',' | ';') {
        let stem = s[..s.len() - last.len_utf8()].trim_end();
        if stem.is_empty() {
            return s.to_string();
        }
        return format!("{stem}.");
    }

    // No terminal punctuation: add a period when it reads like a finished sentence.
    // Casual chat tone prefers no trailing period.
    if tone == "casual" || !last.is_alphanumeric() {
        return s.to_string();
    }
    if looks_like_finished_sentence(s) {
        return format!("{s}.");
    }
    s.to_string()
}

fn looks_like_finished_sentence(s: &str) -> bool {
    let words: Vec<&str> = s.split_whitespace().collect();
    // One- or two-word replies ("yes", "ok thanks") shouldn't get a forced period.
    if words.len() < 3 {
        return false;
    }
    let first = s.chars().find(|c| !c.is_whitespace()).unwrap_or('\0');
    // Prefer capitalized starts; still accept longer uncapitalized dictation.
    first.is_uppercase() || words.len() >= 5
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

    let marker_slice = &lower[idx..idx + mlen];
    let before = text[..idx].trim_end();
    let mut correction = text[idx + mlen..].trim();
    correction = correction.trim_end_matches(|c: char| matches!(c, '.' | '!' | '?' | ','));
    correction = correction.trim();

    if before.is_empty() || correction.is_empty() {
        return text.to_string();
    }

    // "I mean it can slip" is discourse, not a word swap — leave the sentence alone.
    if marker_slice.trim() == "i mean" && looks_like_discourse_filler(correction) {
        return text.to_string();
    }

    let (stem, trailing_punct) = strip_trailing_punct(before);
    let mut words: Vec<String> = stem.split_whitespace().map(|w| w.to_string()).collect();
    if words.is_empty() {
        return format!("{correction}{trailing_punct}");
    }

    let corr_words: Vec<&str> = correction.split_whitespace().collect();

    // Weekday corrections: replace the last weekday earlier in the sentence
    // (handles "for Tuesday … I meant Monday", not only end-position mistakes).
    if !apply_weekday_correction(&mut words, &corr_words) {
        // Fallback: replace the last N words with the correction.
        let n = corr_words.len().min(words.len());
        words.truncate(words.len() - n);
        for w in corr_words {
            words.push(w.to_string());
        }
    }

    let mut out = words.join(" ");
    out.push_str(trailing_punct);
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

async fn apply_tone_ex(
    text: &str,
    tone: &str,
    multilingual: bool,
    dict_terms: &[String],
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = llm_key()?;
    let system = with_dictionary(system_prompt_for_tone(tone, multilingual), dict_terms);
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
         {GRAMMAR_RULES}\n\n{ASR_CORRECTION_RULES}\n\n{SELF_CORRECTION_RULES}"
    );
    call_llm(&api_key, &system, text, 1024).await
}

async fn enhance_long_dictation_ex(
    text: &str,
    tone: &str,
    multilingual: bool,
    dict_terms: &[String],
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = llm_key()?;

    let style = system_prompt_for_tone(tone, multilingual);
    let system = with_dictionary(
        format!(
            "{style}\n\nThis is a longer dictation. Apply Grammarly-style grammar, punctuation, \
             and clarity fixes throughout. Remove filler (um, uh, like). Break into clear paragraphs \
             when natural. Apply self-correction rules carefully. Do not summarize — return the full \
             cleaned transcript only."
        ),
        dict_terms,
    );
    call_llm(&api_key, &system, text, 4096).await
}

async fn cleanup_self_corrections_ex(
    text: &str,
    multilingual: bool,
    dict_terms: &[String],
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = llm_key()?;

    let multi = if multilingual {
        format!(" {MULTILINGUAL_RULES}")
    } else {
        String::new()
    };
    let system = with_dictionary(
        format!(
            "You are a Grammarly-like cleanup pass for spoken dictation. \
             {GRAMMAR_RULES} {ASR_CORRECTION_RULES} {SELF_CORRECTION_RULES}{multi} \
             Only return the cleaned text, nothing else."
        ),
        dict_terms,
    );
    call_llm(&api_key, &system, text, 2048).await
}

/// Enhance path used by the dictation pipeline (preserves code-switched scripts).
pub async fn enhance_dictation_ex(
    text: &str,
    tone: &str,
    long: bool,
    multilingual: bool,
    dict_terms: &[String],
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Local English self-correction can mangle mixed-script text — skip it when
    // the transcript already contains non-Latin characters.
    let local = if multilingual || has_non_latin_script(text) {
        text.to_string()
    } else {
        local_self_correct(text)
    };
    let multi = multilingual || has_non_latin_script(&local);
    if long {
        enhance_long_dictation_ex(&local, tone, multi, dict_terms).await
    } else if tone == "default" {
        cleanup_self_corrections_ex(&local, multi, dict_terms).await
    } else {
        apply_tone_ex(&local, tone, multi, dict_terms).await
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
    use super::{local_self_correct, normalize_terminal_punctuation};

    #[test]
    fn trailing_comma_becomes_period() {
        assert_eq!(
            normalize_terminal_punctuation("This is a finished sentence,", "default"),
            "This is a finished sentence."
        );
        assert_eq!(
            normalize_terminal_punctuation("Also done;", "default"),
            "Also done."
        );
    }

    #[test]
    fn keeps_question_and_exclamation() {
        assert_eq!(
            normalize_terminal_punctuation("Are you free tomorrow?", "default"),
            "Are you free tomorrow?"
        );
        assert_eq!(
            normalize_terminal_punctuation("That was amazing!", "default"),
            "That was amazing!"
        );
    }

    #[test]
    fn adds_period_to_complete_statement() {
        assert_eq!(
            normalize_terminal_punctuation("Please send the report today", "default"),
            "Please send the report today."
        );
    }

    #[test]
    fn casual_tone_does_not_force_period() {
        assert_eq!(
            normalize_terminal_punctuation("hey can you check this later", "casual"),
            "hey can you check this later"
        );
        // Weak ASR endings still get cleaned even in casual.
        assert_eq!(
            normalize_terminal_punctuation("hey can you check this later,", "casual"),
            "hey can you check this later."
        );
    }

    #[test]
    fn short_fragment_stays_unpunctuated() {
        assert_eq!(normalize_terminal_punctuation("ok", "default"), "ok");
        assert_eq!(
            normalize_terminal_punctuation("got it", "default"),
            "got it"
        );
    }

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
    fn corrects_early_tuesday_to_monday() {
        let out = local_self_correct(
            "for Tuesday would you like to go on a trip? Oh no I meant Monday",
        );
        assert!(out.to_lowercase().contains("monday"), "{out}");
        assert!(!out.to_lowercase().contains("tuesday"), "{out}");
        assert!(!out.to_lowercase().contains("meant"), "{out}");
    }

    #[test]
    fn corrects_thursday_to_friday() {
        let out = local_self_correct("Let's meet through Thursday I meant Friday");
        assert!(out.to_lowercase().contains("friday"), "{out}");
        assert!(!out.to_lowercase().contains("thursday"), "{out}");
        assert!(out.to_lowercase().contains("through"), "{out}");
    }

    #[test]
    fn keeps_through_tuesday_when_i_mean_is_discourse() {
        let out =
            local_self_correct("The deadline is through Tuesday I mean it can slip");
        assert!(out.to_lowercase().contains("through tuesday"), "{out}");
        assert!(out.to_lowercase().contains("i mean"), "{out}");
    }

    #[test]
    fn corrects_i_mean() {
        let out = local_self_correct("Send it to Sarah I mean Sandra");
        assert!(out.to_lowercase().contains("sandra"), "{out}");
        assert!(!out.to_lowercase().contains("sarah"), "{out}");
    }
}
