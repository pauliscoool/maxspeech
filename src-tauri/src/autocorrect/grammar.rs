//! Sentence-level grammar suggestions from Harper (offline, rule based, English only).
//! Never auto-applied: the caller shows each one as a suggestion the user accepts or ignores.
use harper_core::linting::{LintGroup, LintKind, Linter, Suggestion};
use harper_core::parsers::PlainEnglish;
use harper_core::spell::FstDictionary;
use harper_core::{Dialect, Document};

/// Only categories that held up on real sentences. Harper's broad `Grammar` category wrongly
/// turned "We was there yesterday" into "their", and its `a`/`an` rule is filed under
/// `Miscellaneous`. Spelling and capitalization belong to the word engine; style is opinion.
fn is_wanted(kind: &LintKind, original: &str, replacement: &str) -> bool {
    match kind {
        LintKind::Agreement | LintKind::Repetition => true,
        LintKind::Miscellaneous => {
            let (o, r) = (original.to_lowercase(), replacement.to_lowercase());
            matches!((o.as_str(), r.as_str()), ("a", "an") | ("an", "a"))
        }
        _ => false,
    }
}

const MAX_SPAN_CHARS: usize = 60;

/// One replacement inside a sentence, in character offsets of the text that was checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarFix {
    pub start: usize,
    pub end: usize,
    pub original: String,
    pub replacement: String,
    pub message: String,
}

pub struct GrammarChecker {
    linter: LintGroup,
}

impl GrammarChecker {
    pub fn new() -> Self {
        let dict = FstDictionary::curated();
        Self { linter: LintGroup::new_curated(dict, Dialect::American) }
    }

    /// Fixes for a finished sentence, earliest first.
    pub fn check(&mut self, sentence: &str) -> Vec<GrammarFix> {
        let chars: Vec<char> = sentence.chars().collect();
        let document = Document::new_curated(sentence, &PlainEnglish);
        let mut fixes: Vec<GrammarFix> = self
            .linter
            .lint(&document)
            .into_iter()
            .filter(|lint| lint.span.end <= chars.len() && lint.span.end - lint.span.start <= MAX_SPAN_CHARS)
            .filter_map(|lint| {
                let replacement = lint.suggestions.iter().find_map(|s| match s {
                    Suggestion::ReplaceWith(r) => Some(r.iter().collect::<String>()),
                    Suggestion::Remove => Some(String::new()),
                    _ => None,
                })?;
                let original: String = chars[lint.span.start..lint.span.end].iter().collect();
                (replacement != original && is_wanted(&lint.lint_kind, &original, &replacement)).then_some(GrammarFix {
                    start: lint.span.start,
                    end: lint.span.end,
                    original,
                    replacement,
                    message: lint.message,
                })
            })
            .collect();
        fixes.sort_by_key(|f| f.start);
        fixes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(g: &mut GrammarChecker, s: &str) -> Vec<(String, String)> {
        g.check(s).into_iter().map(|f| (f.original, f.replacement)).collect()
    }

    #[test]
    fn flags_agreement_and_repeated_words() {
        let mut g = GrammarChecker::new();
        assert!(pairs(&mut g, "She go to school every day.").contains(&("go".into(), "goes".into())));
        assert!(pairs(&mut g, "We was happy.").contains(&("was".into(), "were".into())));
        assert!(pairs(&mut g, "This is is a test.").iter().any(|(_, r)| r == "is"));
    }

    #[test]
    fn harpers_unreliable_grammar_lint_is_not_shown() {
        let mut g = GrammarChecker::new();
        let fixes = pairs(&mut g, "We was there yesterday.");
        assert!(!fixes.iter().any(|(_, r)| r == "their"), "{fixes:?}");
    }

    #[test]
    fn flags_a_wrong_article() {
        let mut g = GrammarChecker::new();
        let fixes = g.check("This is an test.");
        assert!(
            fixes.iter().any(|f| f.original == "an" && f.replacement == "a"),
            "got {fixes:?}"
        );
    }

    #[test]
    fn clean_sentences_produce_nothing() {
        let mut g = GrammarChecker::new();
        for s in ["The meeting is tomorrow at noon.", "I think we should ship it today.", "Thanks for the update."] {
            let fixes = g.check(s);
            assert!(fixes.is_empty(), "{s}: {fixes:?}");
        }
    }

    #[test]
    fn spans_index_the_checked_text() {
        let mut g = GrammarChecker::new();
        let text = "This is an test.";
        for f in g.check(text) {
            let chars: Vec<char> = text.chars().collect();
            assert_eq!(chars[f.start..f.end].iter().collect::<String>(), f.original);
        }
    }
}
