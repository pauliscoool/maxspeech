use crate::store::Store;

/// Expand macro triggers in the text. If a macro matches, return its expansion.
/// Also apply dictionary casing / whole-word corrections for known terms.
pub fn expand_macros(text: &str, store: &Store) -> String {
    let macros = store.get_macros().unwrap_or_default();
    let lower = text.to_lowercase();

    // Check for full macro matches first
    for m in &macros {
        if lower.trim() == m.trigger.to_lowercase() {
            return m.expansion.clone();
        }
    }

    // Apply inline macro expansions
    let mut result = text.to_string();
    for m in &macros {
        let trigger_lower = m.trigger.to_lowercase();
        if let Some(pos) = result.to_lowercase().find(&trigger_lower) {
            let before = &result[..pos];
            let after = &result[pos + m.trigger.len()..];
            result = format!("{before}{}{after}", m.expansion);
        }
    }

    // Apply dictionary corrections as whole-word replacements (case-insensitive).
    // Putting "cloud" in the dictionary forces "Cloud"/"CLOUD" → preferred casing,
    // and also helps when ASR returns a differently-cased product name.
    let dict = store.get_dictionary().unwrap_or_default();
    for word in &dict {
        let want = word.word.trim();
        if want.is_empty() {
            continue;
        }
        result = replace_whole_word_ci(&result, want);
    }

    fix_common_asr(&result)
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
    use super::{fix_common_asr, replace_whole_word_ci};

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
}
