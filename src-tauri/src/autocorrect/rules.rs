//! Deterministic fixes that need no scoring: pronoun "I", missing apostrophes, curated typos.
use std::collections::HashMap;
use std::sync::OnceLock;

use super::casing::apply_case;

/// Words that are not real words (or are overwhelmingly typos) and have one obvious fix.
const TYPOS: &[(&str, &str)] = &[
    ("teh", "the"), ("hte", "the"), ("thier", "their"), ("adn", "and"), ("nad", "and"),
    ("taht", "that"), ("thta", "that"), ("waht", "what"), ("wiht", "with"), ("whit", "with"),
    ("jsut", "just"), ("jstu", "just"), ("yuo", "you"), ("yoru", "your"), ("sicne", "since"),
    ("becuase", "because"), ("beacuse", "because"), ("becasue", "because"), ("befor", "before"),
    ("recieve", "receive"), ("recieved", "received"), ("reciept", "receipt"),
    ("beleive", "believe"), ("belive", "believe"), ("wierd", "weird"), ("freind", "friend"),
    ("freinds", "friends"), ("occured", "occurred"), ("occurence", "occurrence"),
    ("seperate", "separate"), ("seperately", "separately"), ("definately", "definitely"),
    ("definitly", "definitely"), ("definetly", "definitely"), ("tommorow", "tomorrow"),
    ("tomorow", "tomorrow"), ("tommorrow", "tomorrow"), ("untill", "until"), ("alot", "a lot"),
    ("accomodate", "accommodate"), ("acheive", "achieve"), ("adress", "address"),
    ("agressive", "aggressive"), ("apparant", "apparent"), ("arguement", "argument"),
    ("basicly", "basically"), ("begining", "beginning"), ("buisness", "business"),
    ("calender", "calendar"), ("catagory", "category"), ("cheif", "chief"),
    ("collegue", "colleague"), ("comming", "coming"), ("commited", "committed"),
    ("concious", "conscious"), ("curiousity", "curiosity"), ("decison", "decision"),
    ("diffrent", "different"), ("dissapoint", "disappoint"), ("embarass", "embarrass"),
    ("enviroment", "environment"), ("existance", "existence"), ("experiance", "experience"),
    ("familar", "familiar"), ("finaly", "finally"), ("foriegn", "foreign"),
    ("goverment", "government"), ("grammer", "grammar"), ("happend", "happened"),
    ("hieght", "height"), ("immediatly", "immediately"), ("independant", "independent"),
    ("interupt", "interrupt"), ("knowlege", "knowledge"), ("lenght", "length"),
    ("liason", "liaison"), ("libary", "library"), ("maintainance", "maintenance"),
    ("neccessary", "necessary"), ("necesary", "necessary"), ("noticable", "noticeable"),
    ("occassion", "occasion"), ("persistant", "persistent"), ("posession", "possession"),
    ("prefered", "preferred"), ("probaly", "probably"), ("publically", "publicly"),
    ("realy", "really"), ("recomend", "recommend"), ("refered", "referred"),
    ("relevent", "relevant"), ("religous", "religious"), ("remeber", "remember"),
    ("resturant", "restaurant"), ("sieze", "seize"), ("sucessful", "successful"),
    ("succesful", "successful"), ("suprise", "surprise"), ("tendancy", "tendency"),
    ("thourough", "thorough"), ("tounge", "tongue"), ("truely", "truly"),
    ("wether", "whether"), ("writting", "writing"), ("wich", "which"), ("whihc", "which"),
    ("woudl", "would"), ("coudl", "could"), ("shoudl", "should"), ("abotu", "about"),
    ("anwser", "answer"), ("aroudn", "around"),
];

/// Contractions typed without the apostrophe. Only forms that are not common real words.
const CONTRACTIONS: &[(&str, &str)] = &[
    ("dont", "don't"), ("doesnt", "doesn't"), ("didnt", "didn't"), ("isnt", "isn't"),
    ("arent", "aren't"), ("wasnt", "wasn't"), ("werent", "weren't"), ("hasnt", "hasn't"),
    ("havent", "haven't"), ("hadnt", "hadn't"), ("couldnt", "couldn't"),
    ("wouldnt", "wouldn't"), ("shouldnt", "shouldn't"), ("cant", "can't"),
    ("im", "I'm"), ("ive", "I've"), ("youre", "you're"), ("theyre", "they're"),
    ("thats", "that's"), ("whats", "what's"), ("hes", "he's"), ("shes", "she's"),
    ("theres", "there's"), ("wouldve", "would've"), ("couldve", "could've"),
    ("shouldve", "should've"),
];

/// Abbreviations whose trailing period does not end a sentence.
const ABBREVIATIONS: &[&str] = &[
    "vs", "mr", "mrs", "ms", "dr", "prof", "etc", "inc", "ltd", "no", "approx", "est", "st",
];

fn table() -> &'static HashMap<&'static str, &'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| {
        TYPOS
            .iter()
            .chain(CONTRACTIONS.iter())
            .filter(|(from, to)| from != to)
            .copied()
            .collect()
    })
}

fn normalize_apostrophe(word: &str) -> String {
    word.replace('\u{2019}', "'")
}

/// Fix for words with a single obvious correction. Preserves the user's casing.
pub fn word_rule(core: &str) -> Option<String> {
    let lower = normalize_apostrophe(core).to_lowercase();
    if lower == "i" {
        return (core != "I").then(|| "I".to_string());
    }
    if let Some(rest) = lower.strip_prefix("i'") {
        if matches!(rest, "m" | "ll" | "ve" | "d") {
            let fixed = format!("I'{rest}");
            return (core != fixed).then_some(fixed);
        }
    }
    let to = table().get(lower.as_str())?;
    // "im" and "ive" always want a capital I regardless of how they were typed.
    if to.starts_with("I'") {
        return Some((*to).to_string());
    }
    Some(apply_case(core, to))
}

/// Does a token ending in `trailing` punctuation close a sentence?
pub fn ends_sentence(core: &str, trailing: &str) -> bool {
    if !trailing.chars().any(|c| matches!(c, '.' | '!' | '?')) {
        return false;
    }
    let lower = core.to_lowercase();
    if trailing.contains('.') && !trailing.contains(['!', '?']) {
        if core.chars().count() < 2 || ABBREVIATIONS.contains(&lower.as_str()) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lone_i_becomes_capital() {
        assert_eq!(word_rule("i").as_deref(), Some("I"));
        assert_eq!(word_rule("I"), None);
        assert_eq!(word_rule("i'm").as_deref(), Some("I'm"));
        assert_eq!(word_rule("i\u{2019}ll").as_deref(), Some("I'll"));
    }

    #[test]
    fn contractions_get_apostrophes() {
        assert_eq!(word_rule("dont").as_deref(), Some("don't"));
        assert_eq!(word_rule("Dont").as_deref(), Some("Don't"));
        assert_eq!(word_rule("im").as_deref(), Some("I'm"));
    }

    #[test]
    fn real_words_that_look_like_contractions_are_left_alone() {
        for w in ["its", "ill", "id", "lets", "well", "were", "wont"] {
            assert_eq!(word_rule(w), None, "{w}");
        }
    }

    #[test]
    fn curated_typos_fix_with_case() {
        assert_eq!(word_rule("teh").as_deref(), Some("the"));
        assert_eq!(word_rule("Teh").as_deref(), Some("The"));
        assert_eq!(word_rule("recieve").as_deref(), Some("receive"));
        assert_eq!(word_rule("alot").as_deref(), Some("a lot"));
    }

    #[test]
    fn no_typo_entry_maps_to_itself() {
        for (from, to) in TYPOS {
            assert_ne!(from, to, "{from}");
        }
    }

    #[test]
    fn abbreviations_and_initials_do_not_end_sentences() {
        assert!(ends_sentence("done", "."));
        assert!(ends_sentence("ok", "?!"));
        assert!(!ends_sentence("vs", "."));
        assert!(!ends_sentence("g", "."));
        assert!(!ends_sentence("hello", ","));
    }
}
