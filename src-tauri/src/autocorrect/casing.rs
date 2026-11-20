/// Copy the capitalization pattern of `original` onto `replacement` (teh/Teh/TEH -> the/The/THE).
pub fn apply_case(original: &str, replacement: &str) -> String {
    let letters: Vec<char> = original.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
        return replacement.to_uppercase();
    }
    if letters.first().is_some_and(|c| c.is_uppercase()) {
        return capitalize_first(replacement);
    }
    replacement.to_string()
}

pub fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_lower_title_and_upper() {
        assert_eq!(apply_case("teh", "the"), "the");
        assert_eq!(apply_case("Teh", "the"), "The");
        assert_eq!(apply_case("TEH", "the"), "THE");
    }

    #[test]
    fn single_capital_letter_is_title_case_not_shouting() {
        assert_eq!(apply_case("I", "we"), "We");
    }
}
