use crate::store::Store;
use std::collections::HashSet;

const COMMON_WORDS: &str = include_str!("../../../shared/dictation/common_words.txt");
const PURGE_KEY: &str = "dictionary_purge_v1";

fn common_words() -> HashSet<String> {
    COMMON_WORDS
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .flat_map(str::split_whitespace)
        .map(str::to_lowercase)
        .collect()
}

/// A dictionary entry made only of everyday words ("What", "Do", "If they're") is learning junk:
/// it force-capitalizes those words everywhere and biases Deepgram toward them.
pub fn is_junk_dictionary_entry(entry: &str, common: &HashSet<String>) -> bool {
    let tokens: Vec<String> = entry
        .split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '\u{2019}'))
        .filter(|t| !t.is_empty())
        .map(|t| t.replace('\u{2019}', "'").to_lowercase())
        .collect();
    !tokens.is_empty() && tokens.iter().all(|t| common.contains(t) || t.chars().count() < 2)
}

/// Once per install: drop auto-learned junk from the dictionary and the learned substitutions.
pub fn purge_learned_junk(store: &Store) {
    if store.get_setting(PURGE_KEY).ok().flatten().is_some() {
        return;
    }
    let common = common_words();
    let mut removed = 0;
    for entry in store.get_dictionary().unwrap_or_default() {
        if is_junk_dictionary_entry(&entry.word, &common) && store.delete_dict_word(entry.id).is_ok() {
            removed += 1;
        }
    }
    let cleared = store.clear_substitutions().unwrap_or(0);
    log::info!("Dictionary purge: removed {removed} junk entries, {cleared} learned substitutions");
    let _ = store.set_setting(PURGE_KEY, "done");
}

pub fn expand_macros(text: &str, store: &Store) -> String {
    let snippets: Vec<(String, String)> = store
        .get_macros()
        .unwrap_or_default()
        .into_iter()
        .map(|m| (m.trigger, expand_template_vars(&m.expansion)))
        .collect();
    let dictionary: Vec<String> = store
        .get_dictionary()
        .unwrap_or_default()
        .into_iter()
        .map(|w| w.word)
        .collect();
    super::faithful::expand_explicit(text, &dictionary, &snippets)
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

#[cfg(test)]
mod tests {
    #[test]
    fn junk_entries_are_everyday_words_only() {
        let common = super::common_words();
        for junk in ["What", "Do", "If they're", "they\u{2019}re"] {
            assert!(super::is_junk_dictionary_entry(junk, &common), "{junk}");
        }
        for keep in ["CurseForge", "Sarah Connor", "Tailscale"] {
            assert!(!super::is_junk_dictionary_entry(keep, &common), "{keep}");
        }
    }

    use super::super::faithful::expand_explicit;

    #[test]
    fn ordinary_words_and_product_guesses_are_unchanged() {
        let input = "then get hub Covenant court 10 times";
        assert_eq!(expand_explicit(input, &[], &[]), input);
    }

    #[test]
    fn explicit_expansions_use_whole_words_and_longest_first() {
        let snippets = vec![
            ("sign".into(), "X".into()),
            ("sign off".into(), "Regards".into()),
        ];
        assert_eq!(
            expand_explicit("sign off signal", &[], &snippets),
            "Regards signal"
        );
        assert_eq!(
            expand_explicit("maxspeech then", &["MaxSpeech".into(), "Than".into()], &[]),
            "MaxSpeech then"
        );
    }
}
