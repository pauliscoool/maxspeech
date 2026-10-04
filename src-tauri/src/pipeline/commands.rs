/// Voice command detection for Transforms.
/// Returns None if the text is regular dictation, Some(CommandResult) if it's a command.
///
/// A command must be the *whole* utterance (after light normalisation), so a sentence like
/// "make it so that we ship tomorrow" is still dictated normally.

pub enum CommandResult {
    ScratchThat,
    /// LLM rewrite of the last insertion.
    Rewrite(RewriteRequest),
    /// Deterministic edit of the last insertion (no LLM key needed).
    Local(fn(&str) -> String),
    InsertText(String),
}

pub struct RewriteRequest {
    pub instruction: String,
    pub mode: RewriteMode,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RewriteMode {
    /// One edit per sentence, in parallel: fast, for edits that keep the sentence structure.
    Sentences,
    /// One edit per sentence, allowing very different lengths (e.g. into CJK).
    Translate,
    /// The whole passage at once: for edits that merge, drop, add or reorder sentences.
    Whole,
}

struct CommandPattern {
    phrases: &'static [&'static str],
    handler: fn() -> CommandResult,
}

const COMMANDS: &[CommandPattern] = &[
    CommandPattern {
        phrases: &["scratch that", "undo that", "delete that", "never mind"],
        handler: || CommandResult::ScratchThat,
    },
    CommandPattern {
        phrases: &["new line", "newline"],
        handler: || CommandResult::InsertText("\n".to_string()),
    },
    CommandPattern {
        phrases: &["new paragraph"],
        handler: || CommandResult::InsertText("\n\n".to_string()),
    },
    CommandPattern {
        phrases: &["period", "full stop"],
        handler: || CommandResult::InsertText(".".to_string()),
    },
    CommandPattern {
        phrases: &["comma"],
        handler: || CommandResult::InsertText(",".to_string()),
    },
    CommandPattern {
        phrases: &["question mark"],
        handler: || CommandResult::InsertText("?".to_string()),
    },
    CommandPattern {
        phrases: &["exclamation mark", "exclamation point"],
        handler: || CommandResult::InsertText("!".to_string()),
    },
    CommandPattern {
        phrases: &[
            "all caps",
            "make it all caps",
            "make it uppercase",
            "make it all uppercase",
            "make it capital letters",
            "uppercase it",
        ],
        handler: || CommandResult::Local(to_upper),
    },
    CommandPattern {
        phrases: &[
            "all lowercase",
            "make it lowercase",
            "make it all lowercase",
            "make it small letters",
            "lowercase it",
        ],
        handler: || CommandResult::Local(to_lower),
    },
];

struct RewritePattern {
    mode: RewriteMode,
    /// Canonical phrases: "that" / "this" are already folded to "it" before matching.
    phrases: &'static [&'static str],
    instruction: &'static str,
}

const REWRITES: &[RewritePattern] = &[
    RewritePattern {
        mode: RewriteMode::Sentences,
        phrases: &[
            "make it formal",
            "make it more formal",
            "make it professional",
            "make it more professional",
            "rewrite it formally",
            "rewrite formally",
        ],
        instruction: "Rewrite in a formal, professional tone suitable for work email: no slang, \
             no contractions, precise vocabulary, courteous. Keep every idea, fact, name and number.",
    },
    RewritePattern {
        mode: RewriteMode::Whole,
        phrases: &[
            "make it shorter",
            "make it short",
            "make it more concise",
            "make it concise",
            "shorten it",
            "shorten",
            "condense it",
            "trim it",
            "tighten it",
            "cut it down",
        ],
        instruction: "Condense to about half the length or less. Keep the key points and every name, \
             number and date; drop filler, repetition and detail that isn't needed.",
    },
    RewritePattern {
        mode: RewriteMode::Sentences,
        phrases: &[
            "make it casual",
            "make it more casual",
            "make it informal",
            "make it less formal",
            "make it friendly",
            "make it friendlier",
            "make it relaxed",
            "make it conversational",
        ],
        instruction: "Rewrite in a casual, friendly, conversational tone like a message to a \
             colleague: contractions are fine, no stiff phrasing. Keep every idea, fact and number.",
    },
    RewritePattern {
        mode: RewriteMode::Whole,
        phrases: &[
            "make it longer",
            "expand it",
            "expand on it",
            "elaborate",
            "flesh it out",
            "add more detail",
        ],
        instruction: "Expand into a fuller version, roughly 1.5 to 2 times as long, by elaborating \
             on ideas already present. Do not invent facts, names, numbers or commitments.",
    },
    RewritePattern {
        mode: RewriteMode::Sentences,
        phrases: &[
            "make it clearer",
            "make it clear",
            "make it simpler",
            "make it simple",
            "simplify it",
            "make it easier to read",
        ],
        instruction: "Rewrite so it is clearer and simpler: plain words, short sentences, no \
             jargon. Keep every idea and fact.",
    },
    RewritePattern {
        mode: RewriteMode::Sentences,
        phrases: &[
            "make it polite",
            "make it more polite",
            "make it nicer",
            "make it kinder",
            "make it softer",
        ],
        instruction: "Rewrite so it is polite, warm and diplomatic without changing what is being \
             asked or said. Keep every fact.",
    },
    RewritePattern {
        mode: RewriteMode::Sentences,
        phrases: &["make it direct", "make it more direct", "make it punchier"],
        instruction: "Rewrite so it is direct and punchy: lead with the point, cut hedging and \
             padding. Keep every fact.",
    },
    RewritePattern {
        mode: RewriteMode::Sentences,
        phrases: &["make it confident", "make it more confident"],
        instruction: "Rewrite so it sounds confident and assured: remove hedges like 'I think' or \
             'maybe' and apologetic phrasing. Keep every fact.",
    },
    RewritePattern {
        mode: RewriteMode::Whole,
        phrases: &[
            "bullet it",
            "bullet point it",
            "bullet points",
            "bullet point",
            "bulletize it",
            "make it bullets",
            "make it bullet points",
            "make it bulleted",
            "make it a list",
            "turn it into a list",
            "turn it into bullets",
            "turn it into bullet points",
            "list it",
        ],
        instruction: "Format as a bulleted list. One item per line, each starting with '- ' and \
             written as a short parallel phrase without ending punctuation. Keep every item and fact mentioned, in the \
             original order, and add none. If it is one idea, split it into its key points. \
             No intro line, no closing line.",
    },
    RewritePattern {
        mode: RewriteMode::Sentences,
        phrases: &[
            "fix grammar",
            "fix the grammar",
            "fix my grammar",
            "fix spelling",
            "fix grammar and spelling",
            "fix spelling and grammar",
            "proofread it",
            "proofread",
        ],
        instruction: "Proofread only: fix spelling, grammar, punctuation and capitalization. Do \
             not change word choice, tone, structure or length beyond that.",
    },
    RewritePattern {
        mode: RewriteMode::Whole,
        phrases: &[
            "summarize it",
            "summarise it",
            "summarize",
            "summarise",
            "sum it up",
            "tldr",
            "tldr it",
            "make it a summary",
            "make it a tldr",
        ],
        instruction: "Summarize in one to two sentences capturing the main point and any \
             decision, name, number or date. No commentary.",
    },
    RewritePattern {
        mode: RewriteMode::Whole,
        phrases: &[
            "make it an email",
            "make it email",
            "turn it into an email",
            "write it as an email",
            "rewrite it as an email",
            "format it as an email",
            "email format",
        ],
        instruction: "Turn into a short, well-structured email: a brief greeting line ('Hi,' when \
             no name is given), the body in clear short paragraphs, then a sign-off line \
             ('Thanks,'). Keep every fact; invent no names, dates or promises. No subject line.",
    },
];

const TRANSLATE_PREFIXES: &[&str] = &[
    "translate it to ",
    "translate it into ",
    "translate it in ",
    "translate to ",
    "translate into ",
];

const LANGUAGES: &[&str] = &[
    "arabic", "bengali", "bulgarian", "chinese", "croatian", "czech", "danish", "dutch",
    "english", "estonian", "filipino", "finnish", "french", "german", "greek", "hebrew",
    "hindi", "hungarian", "indonesian", "italian", "japanese", "korean", "latvian",
    "lithuanian", "malay", "norwegian", "persian", "polish", "portuguese", "romanian",
    "russian", "serbian", "slovak", "slovenian", "spanish", "swahili", "swedish", "tamil",
    "thai", "turkish", "ukrainian", "urdu", "vietnamese",
];

fn to_upper(text: &str) -> String {
    text.to_uppercase()
}

fn to_lower(text: &str) -> String {
    text.to_lowercase()
}

/// Lowercase, strip punctuation (ASR adds "Make it shorter." / "Make it, shorter"), and drop
/// polite filler around the command so "okay make it shorter please" still matches.
fn normalize(text: &str) -> String {
    let cleaned: String = text
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' })
        .collect();
    let mut words: Vec<&str> = cleaned.split_whitespace().collect();

    while matches!(words.first(), Some(&("hey" | "okay" | "ok" | "please" | "now"))) {
        words.remove(0);
    }
    loop {
        match words.as_slice() {
            [.., "for", "me"] => {
                words.truncate(words.len() - 2);
            }
            [.., "please" | "thanks" | "thx"] => {
                words.pop();
            }
            _ => break,
        }
    }

    words.join(" ").replace("e mail", "email")
}

/// "that"/"this" both mean "the last thing I dictated" — fold to "it" so each rewrite needs
/// only one phrasing.
fn fold_pronouns(norm: &str) -> String {
    norm.split(' ')
        .map(|w| if w == "that" || w == "this" { "it" } else { w })
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn translate_language(canonical: &str) -> Option<String> {
    let lang = TRANSLATE_PREFIXES
        .iter()
        .find_map(|p| canonical.strip_prefix(p))?;
    if LANGUAGES.contains(&lang) {
        Some(capitalize(lang))
    } else {
        None
    }
}

pub fn check_command(text: &str) -> Option<CommandResult> {
    let norm = normalize(text);
    if norm.is_empty() {
        return None;
    }

    for cmd in COMMANDS {
        if cmd.phrases.contains(&norm.as_str()) {
            return Some((cmd.handler)());
        }
    }

    let canonical = fold_pronouns(&norm);
    for cmd in COMMANDS {
        if cmd.phrases.contains(&canonical.as_str()) {
            return Some((cmd.handler)());
        }
    }

    for rewrite in REWRITES {
        if rewrite.phrases.contains(&canonical.as_str()) {
            return Some(CommandResult::Rewrite(RewriteRequest {
                instruction: rewrite.instruction.to_string(),
                mode: rewrite.mode,
            }));
        }
    }

    if let Some(lang) = translate_language(&canonical) {
        return Some(CommandResult::Rewrite(RewriteRequest {
            instruction: format!(
                "Translate the passage into {lang}. Keep names, numbers, links and line breaks. \
                 Output only the translation."
            ),
            mode: RewriteMode::Translate,
        }));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instruction(text: &str) -> Option<String> {
        match check_command(text) {
            Some(CommandResult::Rewrite(r)) => Some(r.instruction),
            _ => None,
        }
    }

    fn is_scratch(text: &str) -> bool {
        matches!(check_command(text), Some(CommandResult::ScratchThat))
    }

    fn inserted(text: &str) -> Option<String> {
        match check_command(text) {
            Some(CommandResult::InsertText(t)) => Some(t),
            _ => None,
        }
    }

    fn local(text: &str, input: &str) -> Option<String> {
        match check_command(text) {
            Some(CommandResult::Local(f)) => Some(f(input)),
            _ => None,
        }
    }

    #[test]
    fn scratch_that_variants() {
        for t in ["scratch that", "Scratch that.", "Okay, scratch that!", "undo that", "Never mind."] {
            assert!(is_scratch(t), "{t}");
        }
    }

    #[test]
    fn new_line_and_paragraph() {
        assert_eq!(inserted("New line."), Some("\n".into()));
        assert_eq!(inserted("newline"), Some("\n".into()));
        assert_eq!(inserted("New paragraph"), Some("\n\n".into()));
    }

    #[test]
    fn formal_shorter_casual_match_with_asr_noise() {
        assert!(instruction("Make it formal.").unwrap().contains("formal"));
        assert!(instruction("make that formal").is_some());
        assert!(instruction("Make it, shorter.").unwrap().contains("half"));
        assert!(instruction("okay make this shorter please").is_some());
        assert!(instruction("Make it casual").unwrap().contains("casual"));
    }

    #[test]
    fn bullet_that_is_a_command() {
        let i = instruction("Bullet that.").unwrap();
        assert!(i.contains("bulleted list"));
        assert!(instruction("bullet points").is_some());
        assert!(instruction("make it a list").is_some());
    }

    #[test]
    fn new_transforms() {
        assert!(instruction("Fix grammar.").unwrap().contains("Proofread"));
        assert!(instruction("Summarize that").unwrap().contains("Summarize"));
        assert!(instruction("turn that into an email").unwrap().contains("email"));
        assert!(instruction("Make it longer").unwrap().contains("Expand"));
        assert_eq!(local("All caps.", "hi there"), Some("HI THERE".into()));
        assert_eq!(local("make it lowercase", "Hi THERE"), Some("hi there".into()));
    }

    #[test]
    fn translate_needs_a_known_language() {
        assert!(instruction("Translate that to Spanish.").unwrap().contains("Spanish"));
        assert!(instruction("translate to japanese").unwrap().contains("Japanese"));
        assert!(instruction("translate it into french please").unwrap().contains("French"));
        assert!(instruction("translate to the store").is_none());
    }

    #[test]
    fn ordinary_dictation_is_not_a_command() {
        for t in [
            "make it so that we ship tomorrow",
            "Make it to the airport by noon.",
            "please scratch that from the list and send it to Bob",
            "I need a new line item on the invoice",
            "Can you summarize the meeting notes for me later today",
            "hello world",
        ] {
            assert!(check_command(t).is_none(), "{t}");
        }
    }
}
