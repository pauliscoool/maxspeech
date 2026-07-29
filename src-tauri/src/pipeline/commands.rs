/// Fast regex-based voice command detection.
/// Returns None if the text is regular dictation, Some(CommandResult) if it's a command.

pub enum CommandResult {
    ScratchThat,
    Rewrite(String),
    InsertText(String),
}

struct CommandPattern {
    phrases: &'static [&'static str],
    handler: fn(&str) -> CommandResult,
}

const COMMANDS: &[CommandPattern] = &[
    CommandPattern {
        phrases: &["scratch that", "undo that", "delete that", "never mind"],
        handler: |_| CommandResult::ScratchThat,
    },
    CommandPattern {
        phrases: &["new line", "newline"],
        handler: |_| CommandResult::InsertText("\n".to_string()),
    },
    CommandPattern {
        phrases: &["new paragraph"],
        handler: |_| CommandResult::InsertText("\n\n".to_string()),
    },
    CommandPattern {
        phrases: &["period", "full stop"],
        handler: |_| CommandResult::InsertText(".".to_string()),
    },
    CommandPattern {
        phrases: &["comma"],
        handler: |_| CommandResult::InsertText(",".to_string()),
    },
    CommandPattern {
        phrases: &["question mark"],
        handler: |_| CommandResult::InsertText("?".to_string()),
    },
    CommandPattern {
        phrases: &["exclamation mark", "exclamation point"],
        handler: |_| CommandResult::InsertText("!".to_string()),
    },
];

const REWRITE_PREFIXES: &[&str] = &[
    "make it ",
    "make that ",
    "rewrite as ",
    "change to ",
    "make this ",
];

pub fn check_command(text: &str) -> Option<CommandResult> {
    let lower = text.to_lowercase();
    let trimmed = lower.trim().trim_end_matches('.');

    // Check exact-match commands
    for cmd in COMMANDS {
        for phrase in cmd.phrases {
            if trimmed == *phrase {
                return Some((cmd.handler)(trimmed));
            }
        }
    }

    // Check rewrite commands ("make it formal", "make that shorter")
    for prefix in REWRITE_PREFIXES {
        if let Some(instruction) = trimmed.strip_prefix(prefix) {
            if !instruction.is_empty() {
                return Some(CommandResult::Rewrite(instruction.to_string()));
            }
        }
    }

    None
}
