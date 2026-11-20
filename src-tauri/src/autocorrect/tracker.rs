//! Platform-independent state machine that turns a stream of typed characters into
//! finished words. It only ever knows about the current run of typing: anything that could
//! move the caret (clicks, arrows, focus changes) resets it rather than guessing.
use super::rules::ends_sentence;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Char(char),
    Backspace,
    /// Enter: a new line starts, so the next word starts a sentence.
    Enter,
    /// Anything that may have moved the caret or changed the field.
    Reset,
}

/// A word the user just finished with a space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Boundary {
    pub leading: String,
    pub core: String,
    pub trailing: String,
    pub delimiter: char,
    pub prev: Option<String>,
    pub sentence_start: bool,
    /// The run began mid-word (after a reset), so `core` may be only the tail of a word.
    pub partial: bool,
}

impl Boundary {
    /// Characters to delete to remove the token and its delimiter.
    pub fn chars_to_delete(&self) -> usize {
        self.leading.chars().count() + self.core.chars().count() + self.trailing.chars().count() + 1
    }
}

#[derive(Default)]
pub struct Tracker {
    token: String,
    prev: Option<String>,
    sentence_start: bool,
    partial: bool,
}

fn is_edge_punct(c: char) -> bool {
    matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '"' | '\'' | '\u{201d}' | '\u{2019}')
}

fn is_lead_punct(c: char) -> bool {
    matches!(c, '(' | '[' | '{' | '"' | '\'' | '\u{201c}' | '\u{2018}')
}

impl Tracker {
    pub fn new() -> Self {
        Self { partial: true, ..Self::default() }
    }

    pub fn feed(&mut self, event: Event) -> Option<Boundary> {
        match event {
            Event::Char(c) if c.is_whitespace() && c != '\n' && c != '\t' => self.finish_token(c),
            Event::Char(c) if c.is_control() || c == '\n' || c == '\t' => {
                self.reset();
                None
            }
            Event::Char(c) => {
                self.token.push(c);
                None
            }
            Event::Backspace => {
                if self.token.pop().is_none() {
                    // We backed into a word we no longer have.
                    self.reset();
                }
                None
            }
            Event::Enter => {
                self.reset();
                self.partial = false;
                self.sentence_start = true;
                None
            }
            Event::Reset => {
                self.reset();
                None
            }
        }
    }

    /// Put a previously finished token back as the one being typed (used after an undo).
    pub fn restore_token(&mut self, token: &str) {
        self.token = token.to_string();
    }

    fn reset(&mut self) {
        self.token.clear();
        self.prev = None;
        self.sentence_start = false;
        self.partial = true;
    }

    fn finish_token(&mut self, delimiter: char) -> Option<Boundary> {
        if self.token.is_empty() {
            return None;
        }
        let token = std::mem::take(&mut self.token);
        let leading: String = token.chars().take_while(|c| is_lead_punct(*c)).collect();
        let rest = &token[leading.len()..];
        let trailing_len: usize = rest
            .chars()
            .rev()
            .take_while(|c| is_edge_punct(*c))
            .map(char::len_utf8)
            .sum();
        let core = rest[..rest.len() - trailing_len].to_string();
        let trailing = rest[rest.len() - trailing_len..].to_string();

        let boundary = Boundary {
            leading,
            core: core.clone(),
            trailing: trailing.clone(),
            delimiter,
            prev: self.prev.clone(),
            sentence_start: self.sentence_start,
            partial: self.partial,
        };

        self.partial = false;
        if core.is_empty() {
            return None;
        }
        self.sentence_start = ends_sentence(&core, &trailing);
        self.prev = if self.sentence_start || !trailing.is_empty() && trailing.chars().any(|c| matches!(c, ',' | ';' | ':')) {
            None
        } else {
            Some(core)
        };
        Some(boundary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_str(t: &mut Tracker, s: &str) -> Vec<Boundary> {
        s.chars().filter_map(|c| t.feed(Event::Char(c))).collect()
    }

    fn fresh_line() -> Tracker {
        let mut t = Tracker::new();
        t.feed(Event::Enter);
        t
    }

    #[test]
    fn finishes_a_word_on_space() {
        let mut t = fresh_line();
        let b = type_str(&mut t, "hello ");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].core, "hello");
        assert!(b[0].sentence_start);
        assert!(!b[0].partial);
    }

    #[test]
    fn splits_edge_punctuation_from_the_word() {
        let mut t = fresh_line();
        let b = type_str(&mut t, "(recieve), ");
        assert_eq!(b[0].leading, "(");
        assert_eq!(b[0].core, "recieve");
        assert_eq!(b[0].trailing, "),");
        assert_eq!(b[0].chars_to_delete(), 1 + 7 + 2 + 1);
    }

    #[test]
    fn tracks_previous_word_and_sentence_start() {
        let mut t = fresh_line();
        let b = type_str(&mut t, "see you later. ok ");
        assert_eq!(b[1].prev.as_deref(), Some("see"));
        assert!(!b[1].sentence_start);
        assert!(b[3].sentence_start);
        assert_eq!(b[3].prev, None);
    }

    #[test]
    fn backspace_edits_the_current_word() {
        let mut t = fresh_line();
        for c in "helo".chars() {
            t.feed(Event::Char(c));
        }
        t.feed(Event::Backspace);
        t.feed(Event::Char('l'));
        t.feed(Event::Char('l'));
        let b = t.feed(Event::Char(' ')).unwrap();
        assert_eq!(b.core, "helll");
    }

    #[test]
    fn backspacing_past_the_word_start_loses_context() {
        let mut t = fresh_line();
        type_str(&mut t, "one ");
        t.feed(Event::Backspace);
        let b = type_str(&mut t, "two ");
        assert!(b[0].partial);
        assert_eq!(b[0].prev, None);
    }

    #[test]
    fn reset_marks_the_next_word_partial() {
        let mut t = fresh_line();
        type_str(&mut t, "one ");
        t.feed(Event::Reset);
        let b = type_str(&mut t, "tw ");
        assert!(b[0].partial);
        assert!(!b[0].sentence_start);
    }

    #[test]
    fn abbreviations_do_not_start_a_new_sentence() {
        let mut t = fresh_line();
        let b = type_str(&mut t, "cats vs. dogs ");
        assert!(!b[2].sentence_start);
    }

    #[test]
    fn double_space_yields_nothing() {
        let mut t = fresh_line();
        let b = type_str(&mut t, "hi  there ");
        assert_eq!(b.len(), 2);
    }
}
