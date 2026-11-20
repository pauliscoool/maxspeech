//! Keyboard-aware edit distance: the "typo given intended word" half of the noisy-channel model.

const ROWS: [&str; 3] = ["qwertyuiop", "asdfghjkl", "zxcvbnm"];
const ROW_OFFSET: [f32; 3] = [0.0, 0.25, 0.75];

const ADJACENT_SUB: f32 = 0.5;
const VOWEL_SUB: f32 = 0.7;
const OTHER_EDIT: f32 = 1.0;
const DOUBLED_LETTER: f32 = 0.5;
const TRANSPOSE: f32 = 0.6;
/// First-letter typos are rarer than interior ones, so they cost more.
const FIRST_LETTER_FACTOR: f32 = 1.5;

fn key_pos(c: char) -> Option<(f32, f32)> {
    for (r, row) in ROWS.iter().enumerate() {
        if let Some(i) = row.find(c) {
            return Some((r as f32, i as f32 + ROW_OFFSET[r]));
        }
    }
    None
}

pub fn keys_adjacent(a: char, b: char) -> bool {
    if a == b {
        return false;
    }
    match (key_pos(a), key_pos(b)) {
        (Some((r1, c1)), Some((r2, c2))) => ((r1 - r2).powi(2) + (c1 - c2).powi(2)).sqrt() < 1.55,
        _ => false,
    }
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

fn sub_cost(x: char, y: char, first: bool) -> f32 {
    let base = if keys_adjacent(x, y) {
        ADJACENT_SUB
    } else if is_vowel(x) && is_vowel(y) {
        VOWEL_SUB
    } else {
        OTHER_EDIT
    };
    if first { base * FIRST_LETTER_FACTOR } else { base }
}

/// Cost of dropping `a[idx]` from the typed word (the user typed an extra char).
fn del_cost(a: &[char], idx: usize) -> f32 {
    let base = if idx >= 1 && a[idx] == a[idx - 1] { DOUBLED_LETTER } else { OTHER_EDIT };
    if idx == 0 { base * FIRST_LETTER_FACTOR } else { base }
}

/// Cost of adding `b[idx]` that the user failed to type.
fn ins_cost(b: &[char], idx: usize) -> f32 {
    let base = if idx >= 1 && b[idx] == b[idx - 1] { DOUBLED_LETTER } else { OTHER_EDIT };
    if idx == 0 { base * FIRST_LETTER_FACTOR } else { base }
}

fn lower_chars(s: &str) -> Vec<char> {
    s.chars().flat_map(char::to_lowercase).collect()
}

/// Weighted Damerau-Levenshtein from what was typed to a candidate word.
pub fn weighted_distance(typo: &str, cand: &str) -> f32 {
    let a = lower_chars(typo);
    let b = lower_chars(cand);
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0f32; m + 1]; n + 1];
    for i in 1..=n {
        d[i][0] = d[i - 1][0] + del_cost(&a, i - 1);
    }
    for j in 1..=m {
        d[0][j] = d[0][j - 1] + ins_cost(&b, j - 1);
    }
    for i in 1..=n {
        for j in 1..=m {
            let sub = if a[i - 1] == b[j - 1] {
                0.0
            } else {
                sub_cost(a[i - 1], b[j - 1], i == 1 && j == 1)
            };
            let mut best = (d[i - 1][j - 1] + sub)
                .min(d[i - 1][j] + del_cost(&a, i - 1))
                .min(d[i][j - 1] + ins_cost(&b, j - 1));
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] && a[i - 1] != a[i - 2] {
                best = best.min(d[i - 2][j - 2] + TRANSPOSE);
            }
            d[i][j] = best;
        }
    }
    d[n][m]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn identical_words_cost_nothing() {
        assert!(close(weighted_distance("word", "word"), 0.0));
        assert!(close(weighted_distance("Word", "word"), 0.0));
    }

    #[test]
    fn transposition_is_cheap() {
        assert!(close(weighted_distance("teh", "the"), TRANSPOSE * 1.0));
        assert!(close(weighted_distance("recieve", "receive"), TRANSPOSE));
    }

    #[test]
    fn adjacent_key_substitution_is_half_price() {
        assert!(close(weighted_distance("wprd", "word"), ADJACENT_SUB));
        assert!(weighted_distance("wxrd", "word") > ADJACENT_SUB);
    }

    #[test]
    fn missed_doubled_letter_is_cheap() {
        assert!(close(weighted_distance("occurence", "occurrence"), DOUBLED_LETTER));
        assert!(close(weighted_distance("tomorrrow", "tomorrow"), DOUBLED_LETTER));
    }

    #[test]
    fn first_letter_errors_cost_more() {
        assert!(weighted_distance("xord", "word") > weighted_distance("wxrd", "word"));
    }

    #[test]
    fn adjacency_matches_qwerty() {
        assert!(keys_adjacent('o', 'p'));
        assert!(keys_adjacent('a', 's'));
        assert!(keys_adjacent('e', 'd'));
        assert!(!keys_adjacent('q', 'p'));
        assert!(!keys_adjacent('a', 'a'));
    }
}
