pub struct GuardDecision {
    pub text: String,
    pub drift: f64,
    pub reason: &'static str,
}

fn bare(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

pub fn has_non_latin_script(text: &str) -> bool {
    text.chars().any(|c| c.is_alphabetic() && c as u32 > 0x024f)
}

pub fn formatting_command(text: &str) -> Option<&'static str> {
    match text
        .trim()
        .trim_end_matches(['.', '?', '!'])
        .to_lowercase()
        .as_str()
    {
        "new line" | "newline" => Some("\n"),
        "new paragraph" => Some("\n\n"),
        "insert comma" | "comma" => Some(","),
        "insert period" | "period" | "full stop" => Some("."),
        "question mark" => Some("?"),
        "exclamation mark" | "exclamation point" => Some("!"),
        _ => None,
    }
}

pub fn replace_phrase(text: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let needle: Vec<char> = from.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let end = i + needle.len();
        if end <= chars.len()
            && (i == 0 || !chars[i - 1].is_alphanumeric())
            && (end == chars.len() || !chars[end].is_alphanumeric())
            && chars[i..end].iter().collect::<String>().to_lowercase() == from.to_lowercase()
        {
            out.push_str(to);
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

pub fn expand_explicit(text: &str, dictionary: &[String], snippets: &[(String, String)]) -> String {
    let mut ordered: Vec<_> = snippets.iter().collect();
    ordered.sort_by_key(|s| std::cmp::Reverse(s.0.chars().count()));
    let mut result = text.to_string();
    for (trigger, expansion) in ordered {
        result = replace_phrase(&result, trigger, expansion);
    }
    for term in dictionary.iter().filter(|s| !s.trim().is_empty()) {
        result = replace_phrase(&result, term.trim(), term.trim());
    }
    result
}

fn weekday(word: &str) -> bool {
    matches!(
        bare(word).as_str(),
        "monday" | "tuesday" | "wednesday" | "thursday" | "friday" | "saturday" | "sunday"
    )
}

fn name(word: &str) -> bool {
    let clean = word.trim_matches(|c: char| !c.is_alphanumeric());
    clean.chars().count() >= 2
        && clean.chars().next().is_some_and(char::is_uppercase)
        && clean.chars().all(|c| c.is_alphabetic() || c == '-')
        && !matches!(
            bare(clean).as_str(),
            "i" | "the"
                | "so"
                | "send"
                | "meet"
                | "lets"
                | "please"
                | "we"
                | "you"
                | "it"
                | "this"
                | "that"
        )
}

fn self_correct(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let normalized: Vec<String> = words.iter().map(|w| bare(w)).collect();
    let markers: &[&[&str]] = &[
        &["oh", "no", "i", "meant"],
        &["sorry", "i", "meant"],
        &["wait", "i", "meant"],
        &["i", "meant"],
        &["i", "mean"],
        &["no", "wait"],
        &["wait", "no"],
        &["wait", "actually"],
        &["actually"],
    ];
    for i in (0..words.len()).rev() {
        let Some(marker) = markers.iter().find(|m| {
            i + m.len() < words.len()
                && normalized[i..i + m.len()]
                    .iter()
                    .map(String::as_str)
                    .eq(m.iter().copied())
        }) else {
            continue;
        };
        if markers.iter().any(|longer| {
            longer.len() > marker.len()
                && i >= longer.len() - marker.len()
                && normalized[i - (longer.len() - marker.len())..i + marker.len()]
                    .iter()
                    .map(String::as_str)
                    .eq(longer.iter().copied())
        }) {
            continue;
        }
        if i == 0 {
            continue;
        }
        let replacement = &words[i + marker.len()..];
        let prefix = self_correct(&words[..i].join(" "));
        let mut before: Vec<String> = prefix.split_whitespace().map(str::to_string).collect();
        if marker.len() == 1
            && !before.last().is_some_and(|w| {
                bare(w).chars().any(|c| c.is_ascii_digit())
                    || matches!(bare(w).as_str(), "am" | "pm")
            })
        {
            return text.to_string();
        }
        let mut target = None;
        if replacement.len() == 1 {
            let rep = replacement[0];
            if weekday(rep) {
                let candidates: Vec<usize> = before
                    .iter()
                    .enumerate()
                    .filter(|(_, w)| weekday(w))
                    .map(|(i, _)| i)
                    .collect();
                if candidates.len() == 1 {
                    target = Some(candidates[0]);
                }
            } else if name(rep) {
                let candidates: Vec<usize> = before
                    .iter()
                    .enumerate()
                    .filter(|(_, w)| name(w) && !weekday(w))
                    .map(|(i, _)| i)
                    .collect();
                if candidates.len() == 1 {
                    target = Some(candidates[0]);
                }
            } else if bare(rep).chars().any(|c| c.is_ascii_digit()) {
                let candidates: Vec<usize> = before
                    .iter()
                    .enumerate()
                    .filter(|(_, w)| bare(w).chars().any(|c| c.is_ascii_digit()))
                    .map(|(i, _)| i)
                    .collect();
                if candidates.len() == 1 {
                    target = Some(candidates[0]);
                }
            }
            if let Some(target) = target {
                let punctuation: String = before[target]
                    .chars()
                    .rev()
                    .take_while(|c| matches!(c, '.' | '?' | '!'))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                before[target] = format!(
                    "{}{punctuation}",
                    rep.trim_end_matches(['.', '?', '!', ','])
                );
                return before.join(" ");
            }
        }
        if replacement.len() == 2
            && matches!(bare(replacement[1]).as_str(), "am" | "pm")
            && bare(replacement[0]).chars().all(|c| c.is_ascii_digit())
        {
            let candidates: Vec<usize> = before
                .windows(2)
                .enumerate()
                .filter(|(_, pair)| {
                    matches!(bare(&pair[1]).as_str(), "am" | "pm")
                        && bare(&pair[0]).chars().all(|c| c.is_ascii_digit())
                })
                .map(|(i, _)| i)
                .collect();
            if candidates.len() == 1 {
                let target = candidates[0];
                before.splice(
                    target..target + 2,
                    replacement.iter().map(|w| w.to_string()),
                );
                return before.join(" ");
            }
        }
        // A repeated clause identifies its span; never delete an arbitrary suffix.
        if replacement.len() >= 3 && before.len() >= replacement.len() {
            let start = before.len() - replacement.len();
            let old = &before[start..];
            if old[1..]
                .iter()
                .map(|w| bare(w))
                .eq(replacement[1..].iter().map(|w| bare(w)))
                && name(&old[0])
                && name(replacement[0])
            {
                before.truncate(start);
                before.extend(replacement.iter().map(|w| w.to_string()));
                return before.join(" ");
            }
        }
        return text.to_string();
    }
    text.to_string()
}

fn remove_hesitations(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out = Vec::new();
    for (i, word) in words.iter().enumerate() {
        let literal = word.contains(['"', '\'', '`'])
            || *word == word.to_uppercase()
            || (i > 0
                && matches!(
                    bare(words[i - 1]).as_str(),
                    "a" | "the"
                        | "word"
                        | "words"
                        | "say"
                        | "said"
                        | "says"
                        | "spell"
                        | "typed"
                        | "literal"
                        | "named"
                        | "called"
                ));
        if !literal && matches!(bare(word).as_str(), "um" | "uh" | "umm" | "uhh") {
            continue;
        }
        // A hyphenated partial word is a stutter; repeated complete words may be emphasis.
        if word.ends_with('-') && i + 1 < words.len() {
            let prefix = bare(word);
            if !prefix.is_empty()
                && prefix.len() <= 2
                && bare(words[i + 1]).starts_with(&prefix)
                && words[i + 1]
                    .trim_end_matches(['.', ',', '?', '!'])
                    .chars()
                    .all(char::is_alphabetic)
                && bare(words[i + 1]).len() > prefix.len() + 1
            {
                continue;
            }
        }
        out.push(*word);
    }
    out.join(" ")
}

fn spoken_format(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < words.len() {
        let first = bare(words[i]);
        let second = words.get(i + 1).map(|w| bare(w)).unwrap_or_default();
        let literal = words[i].contains(['"', '\'', '`'])
            || (i > 0
                && matches!(
                    bare(words[i - 1]).as_str(),
                    "a" | "the"
                        | "about"
                        | "word"
                        | "words"
                        | "phrase"
                        | "says"
                        | "said"
                        | "say"
                        | "named"
                        | "not"
                        | "dont"
                        | "never"
                        | "avoid"
                        | "without"
                        | "explain"
                        | "discuss"
                        | "discussed"
                ))
            || words.get(i + 2).is_some_and(|w| {
                matches!(
                    bare(w).as_str(),
                    "command" | "commands" | "syntax" | "means" | "symbol" | "character"
                )
            });
        let command = if !literal {
            match (first.as_str(), second.as_str()) {
                ("new", "paragraph") => Some("\n\n"),
                ("new", "line") => Some("\n"),
                ("question", "mark") if i + 2 == words.len() => Some("?"),
                ("exclamation", "mark") if i + 2 == words.len() => Some("!"),
                ("insert", "comma") => Some(","),
                ("insert", "period") => Some("."),
                _ => None,
            }
        } else {
            None
        };
        if let Some(mark) = command {
            out = out.trim_end().to_string();
            out.push_str(mark);
            i += 2;
        } else {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push(' ');
            }
            out.push_str(words[i]);
            i += 1;
        }
    }
    out
}

fn numbered_list(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let ordinals = [
        "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    ];
    let mut markers = Vec::new();
    for i in 0..words.len() {
        let expected = markers.len();
        let number = (expected + 1).to_string();
        if i + 2 < words.len()
            && bare(words[i]) == "number"
            && (bare(words[i + 1]) == number
                || ordinals
                    .get(expected)
                    .is_some_and(|ordinal| bare(words[i + 1]) == *ordinal))
        {
            markers.push((i, i + 2));
        } else if i + 1 < words.len() && words[i] == format!("{number}.") {
            markers.push((i, i + 1));
        }
    }
    if markers.len() < 2 || markers[0].0 != 0 {
        return text.to_string();
    }
    let mut lines = Vec::new();
    for (n, &(_, content)) in markers.iter().enumerate() {
        let end = markers.get(n + 1).map(|m| m.0).unwrap_or(words.len());
        if content >= end {
            return text.to_string();
        }
        lines.push(format!("{}. {}", n + 1, words[content..end].join(" ")));
    }
    lines.join("\n")
}

fn render_surface(text: &str, tone: &str) -> String {
    let mut out = String::new();
    let mut sentence_start = true;
    for c in text.chars() {
        if c.is_alphabetic() {
            if sentence_start && tone != "casual" {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
            sentence_start = false;
        } else {
            out.push(c);
            if matches!(c, '.' | '!' | '?' | '\n') {
                sentence_start = true;
            } else if c.is_numeric() {
                sentence_start = false;
            }
        }
    }
    let s = out.trim_end();
    if s.is_empty() {
        return String::new();
    }
    let last = s.chars().last().unwrap();
    if matches!(last, ',' | ';') {
        let stem = s[..s.len() - 1].trim_end();
        return if tone == "casual" {
            stem.to_string()
        } else {
            format!("{stem}.")
        };
    }
    if tone != "casual"
        && !s.contains('\n')
        && last.is_alphanumeric()
        && s.split_whitespace().count() >= 3
    {
        return format!("{s}.");
    }
    s.to_string()
}

pub fn local_cleanup(text: &str, tone: &str, english: bool) -> String {
    if !english || has_non_latin_script(text) {
        return text.trim().to_string();
    }
    if let Some(command) = formatting_command(text) {
        return command.to_string();
    }
    let lines: Vec<String> = text
        .lines()
        .map(|line| remove_hesitations(&self_correct(line)))
        .collect();
    let formatted: Vec<String> = lines
        .iter()
        .map(|line| numbered_list(&spoken_format(line)))
        .collect();
    render_surface(&formatted.join("\n"), tone)
}

fn tokens(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(bare)
        .filter(|s| !s.is_empty() && !matches!(s.as_str(), "um" | "uh" | "umm" | "uhh"))
        .collect()
}

pub fn guard_output(local: &str, candidate: &str) -> GuardDecision {
    let a = tokens(local);
    let b = tokens(candidate);
    if a == b {
        let reason = if candidate.trim().to_lowercase() == local.trim().to_lowercase() {
            "accepted"
        } else {
            "surface_difference"
        };
        return GuardDecision {
            text: local.to_string(),
            drift: 0.0,
            reason,
        };
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, left) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, right) in b.iter().enumerate() {
            let previous = row[j + 1];
            row[j + 1] = (row[j] + 1)
                .min(previous + 1)
                .min(diagonal + usize::from(left != right));
            diagonal = previous;
        }
    }
    let drift = row[b.len()] as f64 / a.len().max(1) as f64;
    // Exact rendering also prevents network timing from changing punctuation across platforms.
    let reason = if candidate.trim().is_empty() && !local.trim().is_empty() {
        "empty"
    } else if a != b {
        "lexical_drift"
    } else if candidate.trim().to_lowercase() != local.trim().to_lowercase() {
        "surface_difference"
    } else {
        "accepted"
    };
    GuardDecision {
        text: local.to_string(),
        drift,
        reason,
    }
}
