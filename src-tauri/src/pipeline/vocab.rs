use crate::store::Store;

/// Expand macro triggers in the text. If a macro matches, return its expansion.
/// Also apply dictionary corrections for commonly misheard words.
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

    // Apply dictionary corrections (case-insensitive replacement)
    let dict = store.get_dictionary().unwrap_or_default();
    for word in &dict {
        let word_lower = word.word.to_lowercase();
        let result_lower = result.to_lowercase();
        if let Some(pos) = result_lower.find(&word_lower) {
            // Only replace if the dictionary word has different casing (suggests a correction)
            let matched = &result[pos..pos + word.word.len()];
            if matched != word.word {
                let before = &result[..pos];
                let after = &result[pos + word.word.len()..];
                result = format!("{before}{}{after}", word.word);
            }
        }
    }

    result
}
