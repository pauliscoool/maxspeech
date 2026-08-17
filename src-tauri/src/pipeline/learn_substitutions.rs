use crate::store::Store;
use std::time::Duration;

/// Re-dictate / erase-then-redictate window. Clocked from last paste → next
/// recording start so enhance lag does not eat the budget.
pub const LEARN_WINDOW: Duration = Duration::from_secs(5);

const MAX_CHANGED_TOKENS: usize = 2;

/// Word-level substitution learned from a small correction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Substitution {
    /// Case-insensitive trigger (stored lowercase).
    pub from: String,
    /// Preferred spelling / casing to paste next time.
    pub to: String,
}

pub fn elapsed_within_window(elapsed: Duration) -> bool {
    elapsed <= LEARN_WINDOW
}

/// Persist small diffs between a previous paste and a follow-up dictation.
pub fn learn_from_redictate(previous: &str, next: &str, store: &Store) {
    persist_pairs(&substitutions_from_redictate(previous, next), store);
}

/// Same mapping for spoken self-correct and history edits (no time window).
pub fn learn_from_edit(before: &str, after: &str, store: &Store) {
    persist_pairs(&substitutions_from_redictate(before, after), store);
}

/// Apply longest learned phrases first so bigrams beat bare tokens.
pub fn apply(text: &str, store: &Store) -> String {
    let mut pairs = store.get_substitutions().unwrap_or_default();
    pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    apply_pairs(
        text,
        &pairs
            .into_iter()
            .map(|(from, to)| Substitution { from, to })
            .collect::<Vec<_>>(),
    )
}

fn persist_pairs(pairs: &[Substitution], store: &Store) {
    for sub in pairs {
        if store
            .upsert_substitution(&sub.from, &sub.to)
            .is_ok()
        {
            log::info!("Learned substitution: {} → {}", sub.from, sub.to);
        }
        for token in tokenize(&sub.to) {
            if looks_like_name_term(&token) {
                let _ = store.add_dict_word(&token, 1.0);
            }
        }
    }
}

/// Map a previous paste to a correction. Empty when the edit is not a small
/// word-level fix (new sentence, password, stop-word swap, …).
pub fn substitutions_from_redictate(previous: &str, next: &str) -> Vec<Substitution> {
    let prev = previous.trim();
    let nxt = next.trim();
    if prev.is_empty() || nxt.is_empty() || prev.eq_ignore_ascii_case(nxt) {
        return Vec::new();
    }
    if looks_like_secret(prev) || looks_like_secret(nxt) {
        return Vec::new();
    }

    let a = tokenize(prev);
    let b = tokenize(nxt);
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }

    if a.len() == b.len() {
        return equal_length_subs(&a, &b);
    }

    // Erase-then-redictate the replacement word(s) only: last 1–2 tokens of the
    // previous paste vs a short new utterance. No key capture — paste texts only.
    if b.len() <= MAX_CHANGED_TOKENS && a.len() > b.len() {
        return suffix_replacement_subs(&a, &b);
    }

    Vec::new()
}

fn equal_length_subs(a: &[String], b: &[String]) -> Vec<Substitution> {
    let diffs: Vec<usize> = (0..a.len())
        .filter(|&i| !eq_ci(&a[i], &b[i]))
        .collect();
    if diffs.is_empty() || diffs.len() > MAX_CHANGED_TOKENS {
        return Vec::new();
    }
    if !diffs
        .iter()
        .all(|&i| similar_token_len(&a[i], &b[i]) && should_learn_pair(&a[i], &b[i]))
    {
        return Vec::new();
    }

    if diffs.len() == 2 && diffs[1] == diffs[0] + 1 {
        if let Some(sub) = adjacent_phrase_sub(a, b, diffs[0], diffs[1]) {
            return vec![sub];
        }
    }

    diffs.iter().filter_map(|&i| pair_at(a, b, i)).collect()
}

fn suffix_replacement_subs(a: &[String], b: &[String]) -> Vec<Substitution> {
    let n = b.len();
    let tail = &a[a.len() - n..];
    if tail.iter().zip(b.iter()).all(|(x, y)| eq_ci(x, y)) {
        return Vec::new();
    }
    if !tail
        .iter()
        .zip(b.iter())
        .all(|(x, y)| similar_token_len(x, y) && should_learn_pair(x, y))
    {
        return Vec::new();
    }

    let both_names = tail
        .iter()
        .zip(b.iter())
        .all(|(from, to)| looks_like_name_term(from) && looks_like_name_term(to));
    if both_names {
        return tail
            .iter()
            .zip(b.iter())
            .filter(|(from, to)| !eq_ci(from, to))
            .map(|(from, to)| Substitution {
                from: from.to_lowercase(),
                to: to.clone(),
            })
            .collect();
    }

    let prefix_len = a.len() - n;
    if prefix_len == 0 {
        return zip_learnable(tail, b);
    }
    let left = &a[prefix_len - 1];
    if !looks_like_name_term(left) {
        return Vec::new();
    }

    let from_tokens = &a[prefix_len - 1..];
    let mut to_tokens = Vec::with_capacity(1 + n);
    to_tokens.push(left.clone());
    to_tokens.extend(b.iter().cloned());
    vec![Substitution {
        from: from_tokens.join(" ").to_lowercase(),
        to: to_tokens.join(" "),
    }]
}

fn zip_learnable(from: &[String], to: &[String]) -> Vec<Substitution> {
    from.iter()
        .zip(to.iter())
        .filter(|(a, b)| !eq_ci(a, b) && should_learn_pair(a, b))
        .map(|(a, b)| Substitution {
            from: a.to_lowercase(),
            to: b.clone(),
        })
        .collect()
}

fn pair_at(a: &[String], b: &[String], i: usize) -> Option<Substitution> {
    let from = &a[i];
    let to = &b[i];
    if !should_learn_pair(from, to) {
        return None;
    }
    if looks_like_name_term(from) && looks_like_name_term(to) {
        return Some(Substitution {
            from: from.to_lowercase(),
            to: to.clone(),
        });
    }
    if i > 0 {
        return Some(Substitution {
            from: format!("{} {}", a[i - 1], from).to_lowercase(),
            to: format!("{} {}", b[i - 1], to),
        });
    }
    if i + 1 < a.len() && eq_ci(&a[i + 1], &b[i + 1]) {
        return Some(Substitution {
            from: format!("{} {}", from, a[i + 1]).to_lowercase(),
            to: format!("{} {}", to, b[i + 1]),
        });
    }
    Some(Substitution {
        from: from.to_lowercase(),
        to: to.clone(),
    })
}

fn adjacent_phrase_sub(a: &[String], b: &[String], i: usize, j: usize) -> Option<Substitution> {
    let from_span = a[i..=j].join(" ");
    let to_span = b[i..=j].join(" ");
    let both_names = a[i..=j]
        .iter()
        .zip(b[i..=j].iter())
        .all(|(from, to)| looks_like_name_term(from) && looks_like_name_term(to));
    if both_names || i == 0 {
        return Some(Substitution {
            from: from_span.to_lowercase(),
            to: to_span,
        });
    }
    Some(Substitution {
        from: format!("{} {}", a[i - 1], from_span).to_lowercase(),
        to: format!("{} {}", b[i - 1], to_span),
    })
}

fn should_learn_pair(from: &str, to: &str) -> bool {
    if from.eq_ignore_ascii_case(to) {
        return false;
    }
    if looks_like_secret(from) || looks_like_secret(to) {
        return false;
    }
    let from_letters = from.chars().filter(|c| c.is_alphabetic()).count();
    let to_letters = to.chars().filter(|c| c.is_alphabetic()).count();
    if from_letters < 2 || to_letters < 2 || from.len() > 40 || to.len() > 40 {
        return false;
    }
    let both_names = looks_like_name_shape(from) && looks_like_name_shape(to);
    let from_l = from.to_ascii_lowercase();
    let to_l = to.to_ascii_lowercase();
    if is_stop_word(&from_l) || is_stop_word(&to_l) {
        return both_names;
    }
    true
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
    !is_stop_word(&w.to_ascii_lowercase())
}

fn looks_like_name_term(word: &str) -> bool {
    looks_like_learnable_term(word) && looks_like_name_shape(word)
}

fn looks_like_name_shape(word: &str) -> bool {
    let w = word.trim();
    if w.len() < 2 || w.len() > 40 {
        return false;
    }
    let Some(first) = w.chars().next() else {
        return false;
    };
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
        "hello", "hi", "hey", "thank", "sorry", "actually",
    ];
    STOP.contains(&lower)
}

fn looks_like_secret(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    if t.contains(char::is_whitespace) {
        return t.split_whitespace().any(token_looks_like_secret);
    }
    token_looks_like_secret(t)
}

fn token_looks_like_secret(t: &str) -> bool {
    if t.len() < 6 {
        return false;
    }
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    let has_lower = t.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = t.chars().any(|c| c.is_ascii_uppercase());
    let has_sym = t.chars().any(|c| !c.is_alphanumeric());
    (has_digit && has_sym) || (has_digit && has_lower && has_upper)
}

fn similar_token_len(a: &str, b: &str) -> bool {
    let la = a.chars().count();
    let lb = b.chars().count();
    let (lo, hi) = (la.min(lb), la.max(lb));
    if lo == 0 {
        return false;
    }
    hi <= lo * 2 + 2
}

fn tokenize(s: &str) -> Vec<String> {
    s.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| {
                matches!(c, ',' | '.' | '!' | '?' | ';' | ':' | '"' | '\'' | '(' | ')')
            })
            .to_string()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn apply_pairs(text: &str, pairs: &[Substitution]) -> String {
    let mut ordered: Vec<&Substitution> = pairs.iter().collect();
    ordered.sort_by(|a, b| b.from.len().cmp(&a.from.len()));
    let mut result = text.to_string();
    for sub in ordered {
        result = replace_phrase_ci(&result, &sub.from, &sub.to);
    }
    result
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_court_to_core_in_bigram() {
        let pairs = substitutions_from_redictate("Covenant court", "Covenant Core");
        assert_eq!(
            pairs,
            vec![Substitution {
                from: "covenant court".into(),
                to: "Covenant Core".into(),
            }]
        );
        assert_eq!(
            apply_pairs("open Covenant court please", &pairs),
            "open Covenant Core please"
        );
        assert_eq!(
            substitutions_from_redictate("Covenant court ", "Covenant Core "),
            pairs
        );
        // Unrelated "court" stays put.
        assert_eq!(apply_pairs("see you in court", &pairs), "see you in court");
    }

    #[test]
    fn learns_daniel_to_samuel() {
        let pairs = substitutions_from_redictate("Daniel", "Samuel");
        assert_eq!(
            pairs,
            vec![Substitution {
                from: "daniel".into(),
                to: "Samuel".into(),
            }]
        );
        assert_eq!(apply_pairs("Daniel walked out", &pairs), "Samuel walked out");
    }

    #[test]
    fn learns_mid_sentence_name_swap() {
        let pairs = substitutions_from_redictate(
            "Send it to Daniel please",
            "Send it to Samuel please",
        );
        assert_eq!(
            pairs,
            vec![Substitution {
                from: "daniel".into(),
                to: "Samuel".into(),
            }]
        );
    }

    #[test]
    fn learns_suffix_after_erase_then_redictate() {
        let pairs = substitutions_from_redictate("Covenant court", "Core");
        assert_eq!(
            pairs,
            vec![Substitution {
                from: "covenant court".into(),
                to: "Covenant Core".into(),
            }]
        );
    }

    #[test]
    fn learns_suffix_name_after_erase() {
        let pairs = substitutions_from_redictate("Meet Daniel", "Samuel");
        assert_eq!(
            pairs,
            vec![Substitution {
                from: "daniel".into(),
                to: "Samuel".into(),
            }]
        );
    }

    #[test]
    fn skips_unrelated_suffix_without_name_context() {
        let pairs = substitutions_from_redictate("see you in court", "Core");
        assert!(pairs.is_empty());
    }

    #[test]
    fn skips_stop_words_unless_both_names() {
        assert!(substitutions_from_redictate("the", "a").is_empty());
        assert!(substitutions_from_redictate("to", "for").is_empty());
        assert!(substitutions_from_redictate("I", "we").is_empty());
    }

    #[test]
    fn skips_single_letters() {
        assert!(substitutions_from_redictate("a", "b").is_empty());
        assert!(substitutions_from_redictate("X", "Y").is_empty());
    }

    #[test]
    fn skips_password_like_tokens() {
        assert!(substitutions_from_redictate("MyP@ssw0rd", "N3wSecret!").is_empty());
        assert!(substitutions_from_redictate("Abcdef1", "Xyzabc2").is_empty());
    }

    #[test]
    fn skips_large_rewrite() {
        assert!(
            substitutions_from_redictate("hello there friend", "goodbye now pal").is_empty()
        );
        assert!(substitutions_from_redictate(
            "Let's schedule a meeting tomorrow",
            "Ship the installer tonight"
        )
        .is_empty());
    }

    #[test]
    fn window_is_five_seconds() {
        assert!(elapsed_within_window(Duration::from_secs(0)));
        assert!(elapsed_within_window(Duration::from_millis(4999)));
        assert!(elapsed_within_window(Duration::from_secs(5)));
        assert!(!elapsed_within_window(Duration::from_millis(5001)));
        assert!(!elapsed_within_window(Duration::from_secs(6)));
    }
}
