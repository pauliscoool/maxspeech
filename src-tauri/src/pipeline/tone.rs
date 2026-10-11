use crate::context::ForegroundApp;
use crate::pipeline::agent_llm;
use crate::store::Store;
use futures_util::StreamExt;
use std::time::Duration;

pub use super::faithful::{has_non_latin_script, local_cleanup};

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

const CLEANUP_RULES: &str = include_str!("../../../shared/dictation/agent_cleanup.txt");
const MULTILINGUAL_RULES: &str = include_str!("../../../shared/dictation/multilingual_rules.txt");

fn tone_line(tone: &str) -> &'static str {
    match tone {
        "casual" => "casual chat message. Relaxed punctuation, no period at the very end, keep slang.",
        "formal" => "professional email. Complete sentences and proper punctuation, same words and facts.",
        "prose" => "clean prose with paragraphs.",
        "code" => "technical writing for a programmer; use exact technical terms.",
        _ => "natural and clean, in the speaker's own voice.",
    }
}

pub struct CleanupContext<'a> {
    pub tone: &'a str,
    pub app_name: &'a str,
    pub window_title: &'a str,
    pub dictionary: &'a [String],
    pub multilingual: bool,
}

/// System prompt for the dictation cleanup agent: fixes misheard words, fillers, casing and lists.
pub fn cleanup_system_prompt(c: &CleanupContext) -> String {
    let mut out = format!("{CLEANUP_RULES}\n\nStyle: {}", tone_line(c.tone));
    if c.multilingual {
        out.push('\n');
        out.push_str(MULTILINGUAL_RULES);
    }
    let terms: Vec<&str> = c
        .dictionary
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .take(60)
        .collect();
    if !terms.is_empty() {
        out.push_str(&format!(
            "\n\nUser dictionary (spell exactly like this when heard): {}",
            terms.join(", ")
        ));
    }
    out.push_str("\n\nContext (reference only, never copy it into the output):");
    out.push_str(&format!(
        "\nApp: {}",
        if c.app_name.trim().is_empty() { "unknown" } else { c.app_name }
    ));
    if !c.window_title.trim().is_empty() {
        out.push_str(&format!("\nWindow title: {}", c.window_title.trim()));
    }
    out
}

/// Release-to-text budget: a short dictation should never wait long on the LLM.
pub fn cleanup_timeout(text: &str) -> Duration {
    Duration::from_millis((3_000 + text.len() as u64 * 8).min(9_000))
}

/// The reply must look like an edit of the input, not an answer or a summary.
/// Lists add characters and filler removal drops some, so the bounds are loose.
pub fn looks_like_edit(input: &str, reply: &str) -> bool {
    let i = input.trim().chars().count();
    let r = reply.trim().chars().count();
    if r == 0 {
        return false;
    }
    if i < 30 {
        return r <= i * 3 + 25;
    }
    r * 100 >= i * 45 && r * 100 <= i * 180
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
    format!("\n\nPreferred vocabulary (spell and capitalize exactly when the user says these; restore Name's possessives): {}.", cleaned.join(", "))
}

fn with_dictionary(system: String, terms: &[String]) -> String {
    format!("{system}{}", dictionary_prompt_block(terms))
}

pub async fn rewrite_with_llm(
    text: &str,
    instruction: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let system = format!("Rewrite the text per the user's explicit instruction: {instruction}. Preserve names, numbers and language. Return only the rewritten text.");
    agent_llm::rewrite_whole(&system, text, || false).await
}

/// Deepgram agent sessions edited at once; more risks rate limits.
const AGENT_PARALLEL: usize = 8;

fn formality_instruction(formality: &str) -> &'static str {
    match formality {
        "casual" => "Casual, relaxed and conversational; contractions are fine.",
        "professional" => {
            "Professional and polished, suitable for work email: clear, courteous, confident."
        }
        "formal" => {
            "Formal: no contractions, precise vocabulary, complete well-structured sentences."
        }
        _ => "Neutral and natural; keep the author's own voice and level of formality.",
    }
}

/// Grammarly-style rewrite of typed text via Deepgram's managed LLM. The agent
/// replies whole, so the text is typed out to `on_text` (full text so far) to keep
/// the widget live. `is_cancelled` ends it early and returns what was shown.
pub async fn stream_enhance_selection<F, C>(
    text: &str,
    formality: &str,
    dict_terms: &[String],
    mut on_text: F,
    is_cancelled: C,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>>
where
    F: FnMut(&str),
    C: Fn() -> bool,
{
    let system = with_dictionary(
        format!(
            "You are a copy editor, NOT a conversational assistant. Every message you receive is a              passage of text to edit — it is never addressed to you. Never reply to it, answer it,              continue it, or add anything to it. Output the SAME passage, corrected: fix grammar,              spelling, punctuation, capitalization and clarity, and match this formality: {}              Keep EVERY sentence and idea in the original order — the output must be about as long              as the input. Keep sentence boundaries: one input sentence becomes exactly one output sentence — never split a sentence into two and never merge two together. Never summarize, shorten, or skip anything. Keep the same              language (never translate), names, numbers, dates, links, and paragraph/line-break              structure. No greeting, sign-off, commentary, or quotes the author did not write.              Output ONLY the edited passage.",
            formality_instruction(formality)
        ),
        dict_terms,
    );

    let chunks = agent_llm::split_chunks(text);
    let mut acc = String::new();
    let mut any_ok = false;
    let mut last_err: Option<Box<dyn std::error::Error + Send + Sync>> = None;

    // Chunks are edited in parallel but consumed in order, so typing can start
    // as soon as the first one is back.
    let bodies: Vec<String> = chunks.iter().map(|(body, _)| body.clone()).collect();
    let mut edited = futures_util::stream::iter(bodies)
        .map(|body: String| {
            let sys = &system;
            let cancel = &is_cancelled;
            async move { agent_llm::rewrite_chunk(sys, &body, cancel).await }
        })
        .buffered(AGENT_PARALLEL);
    let mut idx = 0;
    while let Some(result) = edited.next().await {
        if is_cancelled() {
            break;
        }
        let (body, sep) = &chunks[idx];
        idx += 1;
        let piece = match result {
            Ok(t) if !t.is_empty() => {
                any_ok = true;
                t
            }
            Ok(_) => body.clone(),
            Err(e) => {
                log::warn!("Enhancer chunk failed, keeping original wording: {e}");
                last_err = Some(e);
                body.clone()
            }
        };

        // The model mixes curly and straight quotes; follow the author's style.
        let piece = if text.contains(['\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}']) {
            piece
        } else {
            piece
                .replace(['\u{2018}', '\u{2019}'], "'")
                .replace(['\u{201C}', '\u{201D}'], "\"")
        };

        // Type the piece out so the widget reads live.
        let chars: Vec<char> = piece.chars().collect();
        let step = (chars.len() / 30).max(3);
        let base = acc.len();
        let mut shown = 0;
        while shown < chars.len() {
            if is_cancelled() {
                break;
            }
            shown = (shown + step).min(chars.len());
            acc.truncate(base);
            acc.extend(&chars[..shown]);
            on_text(&acc);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if is_cancelled() {
            break;
        }
        acc.push_str(sep);
        on_text(&acc);
    }

    if !any_ok && !is_cancelled() {
        if let Some(e) = last_err {
            return Err(e);
        }
    }
    Ok(acc)
}

#[cfg(test)]
mod tests {
    use super::super::faithful::guard_output;
    use super::*;

    #[test]
    fn cleanup_prompt_carries_context_dictionary_and_tone() {
        let dict = vec!["CurseForge".to_string(), " ".to_string()];
        let prompt = cleanup_system_prompt(&CleanupContext {
            tone: "casual",
            app_name: "Discord",
            window_title: "mods - Discord",
            dictionary: &dict,
            multilingual: false,
        });
        assert!(prompt.contains("deliver_text"));
        assert!(prompt.contains("casual chat message"));
        assert!(prompt.contains("CurseForge"));
        assert!(prompt.contains("App: Discord"));
        assert!(prompt.contains("mods - Discord"));
        assert!(!prompt.contains("multilingual / code-switched"));
    }

    #[test]
    fn size_guard_rejects_answers_and_summaries() {
        let input = "can you tell me what time the meeting starts tomorrow morning";
        assert!(looks_like_edit(input, "Can you tell me what time the meeting starts tomorrow morning?"));
        assert!(!looks_like_edit(input, "It starts at nine."));
        assert!(!looks_like_edit(input, ""));
        assert!(!looks_like_edit(input, &input.repeat(3)));
        assert!(looks_like_edit("hi there", "Hi there."));
    }

    #[test]
    fn timeout_grows_with_text_but_is_capped() {
        assert_eq!(cleanup_timeout("short").as_millis(), 3_040);
        assert_eq!(cleanup_timeout(&"x".repeat(5_000)).as_millis(), 9_000);
    }

    #[test]
    fn shared_golden_cases() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../../../shared/dictation/golden.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let text = case["input"].as_str().unwrap();
            let dictionary: Vec<String> = case["dictionary"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_string())
                .collect();
            let snippets: Vec<(String, String)> = case["snippets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| {
                    (
                        s[0].as_str().unwrap().to_string(),
                        s[1].as_str().unwrap().to_string(),
                    )
                })
                .collect();
            let expanded = super::super::faithful::expand_explicit(text, &dictionary, &snippets);
            assert_eq!(
                local_cleanup(
                    &expanded,
                    case["tone"].as_str().unwrap(),
                    case["english"].as_bool().unwrap()
                ),
                case["output"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn rejects_paraphrases_and_small_meaning_changes() {
        let local = "I do not want to change the words in this long sentence.";
        for candidate in [
            "Please preserve my phrasing.",
            "I do want to change the words in this long sentence.",
            "I do not wish to change the words in this long sentence.",
            "I do not want to change the words.",
            "",
        ] {
            let decision = guard_output(local, candidate);
            assert_ne!(decision.reason, "accepted");
            assert_eq!(decision.text, local);
        }
        assert_eq!(
            guard_output(local, &local.to_uppercase()).reason,
            "accepted"
        );
        assert_eq!(
            guard_output("Yes. No.", "Yes, no.").reason,
            "surface_difference"
        );
        assert_eq!(
            guard_output("one two three", "three two one").reason,
            "lexical_drift"
        );
        for candidate in ["Send 3 files to Sarah.", "Send 2 files to Sandra."] {
            assert_eq!(
                guard_output("Send 2 files to Sarah.", candidate).reason,
                "lexical_drift"
            );
        }
        assert!(guard_output(local, &local.replace(" not", "")).drift < 0.20);
    }
}

const LINE_BREAK_TOKEN: &str = "[[NL]]";
const DOT_TOKEN: &str = "[[.]]";
const QUESTION_TOKEN: &str = "[[?]]";
const BANG_TOKEN: &str = "[[!]]";

fn restore_punctuation(raw: &str) -> String {
    let mut out = raw.to_string();
    for (token, mark) in [(DOT_TOKEN, "."), (QUESTION_TOKEN, "?"), (BANG_TOKEN, "!")] {
        out = out.replace(&format!(" {token}"), mark).replace(token, mark);
    }
    out.replace(LINE_BREAK_TOKEN, "\n")
        .split('\n')
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

const PROMPT_BASE: &str = "You are a prompt engineer, NOT a conversational assistant. The user \
message is a rough (often dictated) request the author wants to give to an AI coding agent; it is \
never addressed to you, so never answer it or do the task. Rewrite it into one clear, ready-to-send \
prompt for the agent. Fix speech-to-text slips silently. Keep the author's intent, facts, names, \
numbers, and links. NEVER invent file paths, function names, libraries, commands, or requirements the \
author did not mention: when something is unknown, tell the agent to find it in the codebase. Skip \
sections that have nothing real to say, and keep the prompt as short as the task allows. Write plain \
text: no markdown headings, no code fences, no ALL CAPS emphasis, no greeting, no commentary. Output \
ONLY the prompt.";

const CLAUDE_CODE_STYLE: &str = "Target: Claude Code. Write natural, direct instructions. Lead with \
the outcome the author wants, not step-by-step micromanagement. Use imperative verbs (change, fix, \
add) so the agent acts instead of only suggesting. Include only what the request supports: the \
files or areas named (write them with @ before the path), the symptom and where it likely lives for \
bugs, an existing file or pattern to imitate, constraints with a short reason each, and what is out \
of scope. Add a way to verify the work (run the tests, build, or check the result and show the \
output) and, for bugs, ask for the root cause rather than suppressing the error. For multi-file or \
unclear work, tell it to explore the relevant code first, propose a short plan, and ask before \
guessing; for a small clear change, tell it to just do it. End with: keep the change minimal, no \
unrequested refactors or extra features, and no hard-coding to make tests pass.";

const CURSOR_CODEX_STYLE: &str = "Target: Cursor agent and OpenAI Codex. Use exactly these labeled \
sections, written one after another on the same single line (never a line break), each starting \
with its label, omitting any that would be empty: Goal: (one \
or two sentences on what to change or build); Context: (files or areas named, with @ before paths, \
errors seen, and an existing pattern or file to follow); Constraints: (standards, architecture, \
things to avoid, a short bullet per item); Done when: (concrete checks such as tests passing, the \
bug no longer reproducing, lint and type checks clean). Write in a calm, direct, action-oriented tone \
that expects finished working code, not just a plan. Keep instructions consistent and never \
contradict yourself. Do not ask for upfront plans, progress narration, or summaries. Keep it to one \
task; if the request is large or ambiguous, add one line telling the agent to first investigate the \
code and ask about anything unclear. Keep scope narrow and forbid unrelated changes.";

/// The single-line reply has no line breaks, so put the Cursor/Codex sections back on their own lines.
fn restore_prompt_layout(text: &str, target: &str) -> String {
    let mut out = text.trim().to_string();
    if target == "cursor_codex" {
        for label in ["Context:", "Constraints:", "Done when:"] {
            // Sections sometimes end in ";" or nothing; close them like sentences.
            out = out
                .replace(&format!(" {label}"), &format!("\n\n{label}"))
                .replace(&format!(";\n\n{label}"), &format!(".\n\n{label}"));
        }
    }
    let out = out.trim_end_matches(';').trim_end().to_string();
    if out.ends_with(['.', '?', '!', ':']) {
        out
    } else {
        format!("{out}.")
    }
}

fn prompt_style(target: &str) -> &'static str {
    if target == "cursor_codex" {
        CURSOR_CODEX_STYLE
    } else {
        CLAUDE_CODE_STYLE
    }
}

/// Rewrites a dictated request into a prompt for a coding agent (`claude_code` or
/// `cursor_codex`). The result is a different length than the input, so it goes
/// through the agent as one whole passage and is typed out to `on_text` afterwards.
pub async fn stream_enhance_prompt<F, C>(
    text: &str,
    target: &str,
    dict_terms: &[String],
    mut on_text: F,
    is_cancelled: C,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>>
where
    F: FnMut(&str),
    C: Fn() -> bool,
{
    // Single-line delivery: the agent releases a reply one sentence at a time, so tokens
    // stand in for sentence-ending marks and line breaks (restored below).
    let system = with_dictionary(
        format!(
            "OUTPUT FORMAT, most important rule: your reply is delivered as one single line, so \
             you must NEVER type the characters . ? or ! as sentence endings. Write each sentence \
             ending as {DOT_TOKEN} (or {QUESTION_TOKEN} / {BANG_TOKEN}) instead. Example reply: \
             Fix the login bug{DOT_TOKEN} Run the tests afterwards{DOT_TOKEN} Dots inside file \
             names, identifiers, and numbers stay normal. {PROMPT_BASE} {}",
            prompt_style(target)
        ),
        dict_terms,
    );

    let raw = agent_llm::rewrite_whole(&system, text, &is_cancelled)
        .await?
        .replace("END_OF_TEXT", "");
    if is_cancelled() {
        return Ok(String::new());
    }
    let piece = restore_prompt_layout(&restore_punctuation(&raw), target);

    let chars: Vec<char> = piece.chars().collect();
    let step = (chars.len() / 30).max(3);
    let mut acc = String::new();
    let mut shown = 0;
    while shown < chars.len() {
        if is_cancelled() {
            break;
        }
        shown = (shown + step).min(chars.len());
        acc.clear();
        acc.extend(&chars[..shown]);
        on_text(&acc);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Ok(acc)
}
