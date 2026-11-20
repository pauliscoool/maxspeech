//! Typing-time autocorrect (typing only: dictated text never passes through here).
//! This module is the pure, platform-independent core; the Windows hook and the keystroke
//! injector live in sibling modules and call `evaluate`.
#![allow(dead_code)]

pub mod casing;
pub mod engine;
pub mod error_model;
pub mod grammar;
pub mod protect;
pub mod rules;
pub mod tracker;

#[cfg(test)]
mod eval;

#[cfg(windows)]
mod chip;
#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::{accept, dismiss, note_external_edit, set_enabled, status};

#[cfg(not(windows))]
pub fn accept(_id: u64) {}
#[cfg(not(windows))]
pub fn dismiss(_id: u64, _ignore: bool) {}
#[cfg(not(windows))]
pub fn set_enabled(_app: &tauri::AppHandle, _on: bool) {}
#[cfg(not(windows))]
pub fn note_external_edit() {}
#[cfg(not(windows))]
pub fn status() -> Status {
    Status { enabled: false, ready: false, fixed: 0, suggested: 0, keys: 0, note: String::new() }
}

use std::collections::HashSet;

use casing::capitalize_first;
use engine::{Context, Decision, Engine};
use tracker::Boundary;

use crate::store::Store;

#[derive(serde::Serialize)]
pub struct Status {
    pub enabled: bool,
    pub ready: bool,
    pub fixed: usize,
    pub suggested: usize,
    /// Letters seen from the keyboard hook (a count only, never the text).
    pub keys: usize,
    /// Why autocorrect is or isn't acting in the app you last typed in.
    pub note: String,
}

/// Apps where rewriting typed text is unsafe or unwanted: terminals, code editors,
/// password managers, remote desktops.
pub const DEFAULT_EXCLUDED: &[&str] = &[
    "windowsterminal.exe", "wt.exe", "cmd.exe", "powershell.exe", "pwsh.exe", "conhost.exe",
    "code.exe", "cursor.exe", "devenv.exe", "idea64.exe", "pycharm64.exe", "webstorm64.exe",
    "sublime_text.exe", "notepad++.exe", "putty.exe", "mstsc.exe", "vmware.exe", "vmware-vmx.exe",
    "1password.exe", "keepass.exe", "keepassxc.exe", "bitwarden.exe",
];

/// The user's exclusion list (one exe per line), or the defaults until they edit it.
pub fn excluded_apps(store: &Store) -> HashSet<String> {
    match store.get_setting("autocorrect_excluded_apps").ok().flatten() {
        Some(raw) => raw.lines().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty()).collect(),
        None => DEFAULT_EXCLUDED.iter().map(|s| (*s).to_string()).collect(),
    }
}

// SymSpell word lists (MIT, see data/autocorrect/SYMSPELL-LICENSE.txt).
const FREQ_EN: &str = include_str!("../../data/autocorrect/freq_en.txt");
const BIGRAM_EN: &str = include_str!("../../data/autocorrect/bigram_en.txt");
const MAX_WORDS: usize = 60_000;

/// Build the English engine from the bundled lists. Takes a moment, so call it off the UI thread.
pub fn bundled_engine() -> Engine {
    let mut engine = Engine::from_freq_text(FREQ_EN, MAX_WORDS);
    engine.add_bigrams(BIGRAM_EN);
    engine
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub original: String,
    pub replacement: String,
    /// True: apply silently. False: only offer it as a suggestion.
    pub auto: bool,
}

fn starts_lowercase(s: &str) -> bool {
    s.chars().next().is_some_and(char::is_lowercase)
}

/// Decide what, if anything, to do with the word the user just finished.
pub fn evaluate(
    engine: &Engine,
    boundary: &Boundary,
    user_known: &dyn Fn(&str) -> bool,
) -> Option<Fix> {
    let core = boundary.core.as_str();
    // The user's own words keep their exact spelling and casing, even at a sentence start.
    if boundary.partial || user_known(&core.to_lowercase()) {
        return None;
    }
    let original = core.to_string();

    let (mut replacement, auto) = if let Some(fixed) = rules::word_rule(core) {
        (fixed, true)
    } else if protect::is_protected(core, boundary.sentence_start) {
        return None;
    } else {
        let ctx = Context { prev: boundary.prev.as_deref() };
        match engine.decide(core, &ctx, user_known) {
            Decision::AutoFix(r) => (r, true),
            Decision::Suggest(r) => (r, false),
            Decision::Leave => (core.to_string(), true),
        }
    };

    if boundary.sentence_start && starts_lowercase(&replacement) {
        replacement = capitalize_first(&replacement);
    }
    (replacement != original).then_some(Fix { original, replacement, auto })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracker::{Event, Tracker};

    const SEED: &str = "the 1000000\nword 20000\nworld 40000\nreceive 5000\nbecause 70000\n\
tomorrow 9000\nhello 50000\nmore 800000\nthan 140000\nthen 150000\nsee 90000\nyou 900000";

    fn engine() -> Engine {
        let mut e = Engine::from_freq_text(SEED, 100);
        e.add_bigrams("more than 5000\nmore then 5");
        e
    }

    /// Type `s` on a fresh line and return the fix (if any) for each finished word.
    fn run(s: &str) -> Vec<Option<Fix>> {
        let e = engine();
        let mut t = Tracker::new();
        t.feed(Event::Enter);
        s.chars()
            .filter_map(|c| t.feed(Event::Char(c)))
            .map(|b| evaluate(&e, &b, &|_| false))
            .collect()
    }

    fn fix(original: &str, replacement: &str, auto: bool) -> Option<Fix> {
        Some(Fix { original: original.into(), replacement: replacement.into(), auto })
    }

    #[test]
    fn fixes_typos_and_capitalizes_sentence_starts() {
        let out = run("teh world. recieve ");
        assert_eq!(out[0], fix("teh", "The", true));
        assert_eq!(out[1], None);
        assert_eq!(out[2], fix("recieve", "Receive", true));
    }

    #[test]
    fn lone_i_and_contractions_are_fixed_mid_sentence() {
        let out = run("hello i dont ");
        assert_eq!(out[0], fix("hello", "Hello", true));
        assert_eq!(out[1], fix("i", "I", true));
        assert_eq!(out[2], fix("dont", "don't", true));
    }

    #[test]
    fn protected_tokens_and_partial_words_are_left_alone() {
        let e = engine();
        let mut t = Tracker::new();
        let mut fixes = Vec::new();
        for c in "wprd MaxSpeech user_name ".chars() {
            if let Some(b) = t.feed(Event::Char(c)) {
                fixes.push(evaluate(&e, &b, &|_| false));
            }
        }
        // First word came after no Enter, so the tracker could not know it was whole.
        assert_eq!(fixes[0], None);
        assert_eq!(fixes[1], None);
        assert_eq!(fixes[2], None);
    }

    #[test]
    fn confusable_word_with_context_is_a_suggestion_not_an_auto_fix() {
        let out = run("hello more then ");
        assert_eq!(out[2], fix("then", "than", false));
    }

    #[test]
    fn user_dictionary_words_are_respected() {
        let e = engine();
        let mut t = Tracker::new();
        t.feed(Event::Enter);
        let b = "wprd "
            .chars()
            .filter_map(|c| t.feed(Event::Char(c)))
            .next()
            .unwrap();
        assert_eq!(evaluate(&e, &b, &|w| w == "wprd"), None);
    }
}
