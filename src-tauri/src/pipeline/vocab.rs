use crate::store::Store;

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
