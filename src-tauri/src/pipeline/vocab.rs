use crate::store::Store;

/// Expand macro triggers in the text. If a macro matches, return its expansion.
/// Also apply dictionary casing / whole-word corrections for known terms.
pub fn expand_macros(text: &str, store: &Store) -> String {
    let mut macros = store.get_macros().unwrap_or_default();
    // Longest triggers first so "sign off" wins over "sign".
    macros.sort_by(|a, b| b.trigger.len().cmp(&a.trigger.len()));
    let lower = text.to_lowercase();

    // Check for full macro matches first
    for m in &macros {
        if lower.trim() == m.trigger.to_lowercase() {
            return expand_template_vars(&m.expansion);
        }
    }

    // Inline expansions: whole-word / phrase boundaries, all occurrences, UTF-8 safe.
    let mut result = text.to_string();
    for m in &macros {
        let expansion = expand_template_vars(&m.expansion);
        result = replace_phrase_ci(&result, &m.trigger, &expansion);
    }

    // Apply dictionary corrections as whole-word replacements (case-insensitive).
    // Putting "cloud" in the dictionary forces "Cloud"/"CLOUD" → preferred casing,
    // and also helps when ASR returns a differently-cased product name.
    let dict = store.get_dictionary().unwrap_or_default();
    let dict_words: Vec<String> = dict
        .iter()
        .map(|w| w.word.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect();
    for want in &dict_words {
        result = replace_whole_word_ci(&result, want);
    }
    result = apply_learned_possessives(&result, &dict_words);

    fix_common_asr(&result)
}

/// Expand `{clipboard}`, `{date}`, `{time}` in snippet templates.
fn expand_template_vars(expansion: &str) -> String {
    let mut out = expansion.to_string();
    if out.contains("{clipboard}") {
        let clip = arboard::Clipboard::new()
            .ok()
            .and_then(|mut cb| cb.get_text().ok())
            .unwrap_or_default();
        out = out.replace("{clipboard}", &clip);
    }
    if out.contains("{date}") {
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        out = out.replace("{date}", &date);
    }
    if out.contains("{time}") {
        let time = chrono::Local::now().format("%H:%M").to_string();
        out = out.replace("{time}", &time);
    }
    out
}

/// Persist name-like corrections so future ASR prefers the fixed spelling.
/// Learns tokens that look like proper names / jargon when the user (or
/// self-correction) changes one word into another.
pub fn learn_name_corrections(before: &str, after: &str, store: &Store) {
    let before_t = before.trim();
    let after_t = after.trim();
    if before_t.is_empty() || after_t.is_empty() || before_t == after_t {
        return;
    }

    let strip = |s: &str| -> Vec<String> {
        s.split_whitespace()
            .map(|w| {
                w.trim_matches(|c: char| matches!(c, ',' | '.' | '!' | '?' | ';' | ':' | '"' | '\''))
                    .to_string()
            })
            .filter(|w| !w.is_empty())
            .collect()
    };

    let a = strip(before_t);
    let b = strip(after_t);
    if a.is_empty() || b.is_empty() {
        return;
    }

    // Align from the end: self-corrections usually replace the last N words.
    let mut i = a.len();
    let mut j = b.len();
    while i > 0 && j > 0 {
        if a[i - 1].eq_ignore_ascii_case(&b[j - 1]) {
            i -= 1;
            j -= 1;
        } else {
            break;
        }
    }
    // Tokens that differ at the end of `after` are the corrections.
    let n_changed = b.len().saturating_sub(j);
    let changed: Vec<&str> = b[j..]
        .iter()
        .map(|s| s.as_str())
        .filter(|w| {
            // Single-token swaps (classic name fix) are always candidates;
            // multi-word corrections only keep name-like tokens.
            if n_changed == 1 {
                looks_like_learnable_term(w)
            } else {
                looks_like_name_term(w)
            }
        })
        .collect();

    // Also pick up mid-sentence single-token diffs of equal length.
    if changed.is_empty() && a.len() == b.len() {
        for (wa, wb) in a.iter().zip(b.iter()) {
            if !wa.eq_ignore_ascii_case(wb) && looks_like_name_term(wb) {
                let _ = store.add_dict_word(wb, 1.0);
                log::info!("Learned name correction from edit: {wa} → {wb}");
            }
        }
        return;
    }

    for w in changed {
        if store.add_dict_word(w, 1.0).is_ok() {
            log::info!("Learned name correction: {w}");
        }
    }
}

fn looks_like_learnable_term(word: &str) -> bool {
    let w = word.trim();
    if w.len() < 2 || w.len() > 40 {
        return false;
    }
    let letters = w.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 2 {
        return false;
    }
    let lower = w.to_ascii_lowercase();
    !is_stop_word(&lower)
}

fn looks_like_name_term(word: &str) -> bool {
    let w = word.trim();
    if !looks_like_learnable_term(w) {
        return false;
    }
    let first = w.chars().next().unwrap();
    first.is_uppercase()
        || w.contains('-')
        || (w.len() <= 6 && w.chars().all(|c| c.is_ascii_uppercase()))
}

fn is_stop_word(lower: &str) -> bool {
    const STOP: &[&str] = &[
        "a", "an", "the", "and", "or", "but", "to", "of", "in", "on", "for",
        "with", "at", "by", "from", "as", "is", "it", "this", "that", "i",
        "you", "he", "she", "we", "they", "my", "your", "me", "him", "her",
        "monday", "tuesday", "wednesday", "thursday", "friday", "saturday",
        "sunday", "today", "tomorrow", "yesterday", "please", "thanks",
        "yes", "no", "ok", "okay", "um", "uh", "like", "just", "really",
        "hello", "hi", "hey", "thanks", "thank", "sorry", "actually",
    ];
    STOP.contains(&lower)
}

/// Restore `Name's` when the dictionary has `Name` and ASR emitted `Names`.
fn apply_learned_possessives(text: &str, names: &[String]) -> String {
    let mut result = text.to_string();
    let mut candidates: Vec<&str> = names
        .iter()
        .map(|n| n.trim())
        .filter(|n| {
            looks_like_name_term(n)
                && !n.contains('\'')
                && !n.ends_with('s')
                && !n.ends_with('S')
        })
        .collect();
    candidates.sort_by(|a, b| b.len().cmp(&a.len()));
    for name in candidates {
        let from = format!("{name}s");
        let to = format!("{name}'s");
        result = replace_phrase_ci(&result, &from, &to);
    }
    result
}

/// Deterministic fixes for frequent English ASR near-homophones (esp. Git ↔ get).
fn fix_common_asr(text: &str) -> String {
    let mut result = text.to_string();

    // Phrase-level first (longest matches).
    for (from, to) in [
        ("get hub", "GitHub"),
        ("git hub", "GitHub"),
        ("place it to get", "place it to Git"),
        ("placed it to get", "placed it to Git"),
        ("push it to get", "push it to Git"),
        ("pushed it to get", "pushed it to Git"),
        ("push to get", "push to Git"),
        ("pushed to get", "pushed to Git"),
        ("pull from get", "pull from Git"),
        ("pulled from get", "pulled from Git"),
        ("commit to get", "commit to Git"),
        ("committed to get", "committed to Git"),
        ("clone from get", "clone from Git"),
        ("cloned from get", "cloned from Git"),
        ("merge into get", "merge into Git"),
        ("merged into get", "merged into Git"),
        ("branch on get", "branch on Git"),
        ("repo on get", "repo on Git"),
        ("repository on get", "repository on Git"),
        ("type script", "TypeScript"),
        ("java script", "JavaScript"),
        ("node js", "Node.js"),
        ("next js", "Next.js"),
        ("postgres ql", "PostgreSQL"),
        ("post grass", "Postgres"),
        ("verse cell", "Vercel"),
        ("super base", "Supabase"),
        ("cloud flare", "Cloudflare"),
        ("clout flare", "Cloudflare"),
        ("clout storage", "cloud storage"),
        ("on the clout", "on the cloud"),
        ("deep grammar", "Deepgram"),
        ("deep gram", "Deepgram"),
        ("chat gpt", "ChatGPT"),
        ("chat gbt", "ChatGPT"),
        ("open ai", "OpenAI"),
        ("git lab", "GitLab"),
        ("vs code", "VS Code"),
        ("curse forge", "CurseForge"),
        ("graph ql", "GraphQL"),
        ("mongo db", "MongoDB"),
        ("a ws", "AWS"),
    ] {
        result = replace_phrase_ci(&result, from, to);
    }

    // Contextual bare "get" → "Git" after VC verbs / prepositions.
    result = fix_get_as_git(&result);
    result
}

fn fix_get_as_git(text: &str) -> String {
    // Match "... (to|into|from|on|with) get ..." when the preceding clause
    // looks like version-control speech (place/push/pull/commit/…).
    let lower = text.to_lowercase();
    let vc_hint = [
        "place", "placed", "push", "pushed", "pull", "pulled", "commit",
        "committed", "clone", "cloned", "merge", "merged", "rebase", "rebased",
        "fork", "forked", "branch", "checkout", "repo", "repository", "remote",
        "github", "gitlab", "bitbucket", "pr ", " pull request",
    ]
    .iter()
    .any(|h| lower.contains(h));
    if !vc_hint {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Whole-word "get"
        if matches_word_at(&chars, i, "get") {
            let before = preceding_token(&chars, i);
            let after = following_token(&chars, i + 3);
            let before_l = before.to_lowercase();
            let after_l = after.to_lowercase();
            let prep = matches!(
                before_l.as_str(),
                "to" | "into" | "from" | "on" | "with" | "via" | "onto"
            );
            // "get commit", "get push", "get pull", "get repo", "get branch"
            let git_noun = matches!(
                after_l.as_str(),
                "commit" | "commits" | "push" | "pull" | "repo" | "repository"
                    | "branch" | "branches" | "clone" | "merge" | "rebase"
                    | "remote" | "status" | "diff" | "log" | "ignore"
            );
            if prep || git_noun {
                out.push_str("Git");
                i += 3;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn matches_word_at(chars: &[char], i: usize, word: &str) -> bool {
    let w: Vec<char> = word.chars().collect();
    if i + w.len() > chars.len() {
        return false;
    }
    let before_ok = i == 0 || !chars[i - 1].is_alphanumeric();
    let after_i = i + w.len();
    let after_ok = after_i >= chars.len() || !chars[after_i].is_alphanumeric();
    if !before_ok || !after_ok {
        return false;
    }
    chars[i..i + w.len()]
        .iter()
        .zip(w.iter())
        .all(|(a, b)| a.to_ascii_lowercase() == *b)
}

fn preceding_token(chars: &[char], at: usize) -> String {
    if at == 0 {
        return String::new();
    }
    let mut end = at;
    while end > 0 && chars[end - 1].is_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && chars[start - 1].is_alphanumeric() {
        start -= 1;
    }
    chars[start..end].iter().collect()
}

fn following_token(chars: &[char], at: usize) -> String {
    let mut start = at;
    while start < chars.len() && chars[start].is_whitespace() {
        start += 1;
    }
    let mut end = start;
    while end < chars.len() && chars[end].is_alphanumeric() {
        end += 1;
    }
    chars[start..end].iter().collect()
}

fn replace_phrase_ci(text: &str, from: &str, to: &str) -> String {
    let from_lower = from.to_lowercase();
    let from_chars: Vec<char> = from_lower.chars().collect();
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let remaining = chars.len() - i;
        if remaining >= from_chars.len() {
            let slice: String = chars[i..i + from_chars.len()].iter().collect();
            if slice.to_lowercase() == from_lower {
                let before_ok = i == 0 || !chars[i - 1].is_alphanumeric();
                let after_i = i + from_chars.len();
                let after_ok = after_i >= chars.len() || !chars[after_i].is_alphanumeric();
                if before_ok && after_ok {
                    out.push_str(to);
                    i = after_i;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Replace whole-word matches of `want` (case-insensitive) with the exact `want` spelling.
fn replace_whole_word_ci(text: &str, want: &str) -> String {
    let want_lower = want.to_lowercase();
    let want_chars: Vec<char> = want_lower.chars().collect();
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let remaining = chars.len() - i;
        if remaining >= want_chars.len() {
            let slice: String = chars[i..i + want_chars.len()].iter().collect();
            if slice.to_lowercase() == want_lower {
                let before_ok = i == 0 || !chars[i - 1].is_alphanumeric();
                let after_i = i + want_chars.len();
                let after_ok = after_i >= chars.len() || !chars[after_i].is_alphanumeric();
                if before_ok && after_ok {
                    out.push_str(want);
                    i = after_i;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{apply_learned_possessives, fix_common_asr, replace_whole_word_ci};

    #[test]
    fn replaces_whole_word_casing() {
        assert_eq!(replace_whole_word_ci("use Cloud storage", "cloud"), "use cloud storage");
        assert_eq!(replace_whole_word_ci("cloudy day", "cloud"), "cloudy day");
        assert_eq!(replace_whole_word_ci("MaxSpeech rocks", "MaxSpeech"), "MaxSpeech rocks");
    }

    #[test]
    fn fixes_git_vs_get() {
        assert_eq!(
            fix_common_asr("Did you place it to get?"),
            "Did you place it to Git?"
        );
        assert_eq!(
            fix_common_asr("Did you push it to get"),
            "Did you push it to Git"
        );
        assert_eq!(fix_common_asr("open get hub"), "open GitHub");
        // Ordinary English should stay put.
        assert_eq!(
            fix_common_asr("I want to get coffee"),
            "I want to get coffee"
        );
    }

    #[test]
    fn fixes_split_product_names() {
        assert_eq!(fix_common_asr("write it in type script"), "write it in TypeScript");
        assert_eq!(fix_common_asr("deploy on verse cell"), "deploy on Vercel");
        assert_eq!(fix_common_asr("open super base"), "open Supabase");
        assert_eq!(fix_common_asr("ask chat gpt"), "ask ChatGPT");
        assert_eq!(fix_common_asr("install curse forge"), "install CurseForge");
        assert_eq!(fix_common_asr("edit in vs code"), "edit in VS Code");
        assert_eq!(fix_common_asr("on the clout"), "on the cloud");
        assert_eq!(fix_common_asr("install CurseForge"), "install CurseForge");
    }

    #[test]
    fn restores_possessive_from_learned_name() {
        let names = vec!["Sandra".to_string(), "Paul".to_string()];
        assert_eq!(
            apply_learned_possessives("Send it to Sandras desk", &names),
            "Send it to Sandra's desk"
        );
        assert_eq!(
            apply_learned_possessives("Pauls laptop is here", &names),
            "Paul's laptop is here"
        );
        assert_eq!(
            apply_learned_possessives("the reports are ready", &names),
            "the reports are ready"
        );
    }
}
