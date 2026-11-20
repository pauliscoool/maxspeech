/// Tokens autocorrect must never touch: names, acronyms, identifiers, URLs, numbers.
pub fn is_protected(core: &str, sentence_start: bool) -> bool {
    let chars: Vec<char> = core.chars().collect();
    if chars.is_empty() {
        return true;
    }
    if chars.iter().any(|c| !(c.is_alphabetic() || *c == '\'' || *c == '\u{2019}')) {
        return true;
    }
    let upper = chars.iter().filter(|c| c.is_uppercase()).count();
    // ALLCAPS (acronyms) and camelCase / MaxSpeech-style brands.
    if chars.len() > 1 && upper == chars.len() {
        return true;
    }
    if chars.iter().skip(1).any(|c| c.is_uppercase()) {
        return true;
    }
    // A capitalized word mid-sentence is most likely a name.
    if chars[0].is_uppercase() && !sentence_start && chars.len() > 1 {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_identifiers_and_numbers() {
        for t in ["user_name", "foo123", "a@b.com", "C:\\x", "http://x", "v2", "100%"] {
            assert!(is_protected(t, false), "{t}");
        }
    }

    #[test]
    fn protects_brands_acronyms_and_names() {
        for t in ["MaxSpeech", "iPhone", "NASA", "API", "Sarah"] {
            assert!(is_protected(t, false), "{t}");
        }
    }

    #[test]
    fn plain_words_and_sentence_starts_are_checkable() {
        for (t, start) in [("recieve", false), ("Recieve", true), ("don't", false)] {
            assert!(!is_protected(t, start), "{t}");
        }
    }
}
