use crate::context::ForegroundApp;
use crate::secrets;
use crate::store::Store;

pub fn get_tone_for_app(app: &ForegroundApp, store: &Store) -> Option<String> {
    let profiles = store.get_app_profiles().unwrap_or_default();
    let exe = app.exe.to_lowercase();
    let title = app.title.to_lowercase();

    // Prefer title-specific rules over bare-exe wildcards, then longer titles.
    let mut best: Option<(usize, &str)> = None;
    for profile in &profiles {
        if !profile.enabled {
            continue;
        }
        let pat_exe = profile.exe_pattern.to_lowercase();
        if pat_exe.is_empty() || !exe.contains(&pat_exe) {
            continue;
        }
        let pat_title = profile.title_pattern.to_lowercase();
        let title_ok = pat_title.is_empty() || title.contains(&pat_title);
        if !title_ok {
            continue;
        }
        // Score: titled matches beat empty title; longer title beats shorter.
        let score = if pat_title.is_empty() {
            0
        } else {
            1_000 + pat_title.len()
        };
        match best {
            Some((best_score, _)) if score <= best_score => {}
            _ => best = Some((score, profile.tone.as_str())),
        }
    }
    best.map(|(_, tone)| tone.to_string())
}

const SELF_CORRECTION_RULES: &str = "\
CRITICAL — spoken self-corrections (highest priority): \
The speaker often changes their mind mid-sentence. Detect phrases like: \
'oh no I meant', 'I meant', 'wait actually', 'no wait', 'scratch that', \
'correction:', 'wait no', 'or rather', 'sorry I meant'. \
Treat short 'I mean <replacement>' as a correction (e.g. a name). \
Do NOT treat discourse filler 'I mean …' (e.g. 'I mean it can slip') as a correction. \
Keep ONLY the final intended meaning. DELETE the mistaken word/phrase AND all \
correction chatter. When correcting a weekday/date, replace that weekday wherever \
it appears earlier in the sentence — not only the last few words. \
Never delete weekdays or phrases like 'through Tuesday' unless a clear correction \
replaces them. \
\
Examples (input → output): \
1) 'Would you like to go on a trip on Tuesday? Oh no I meant Monday' \
   → 'Would you like to go on a trip on Monday?' \
2) 'for Tuesday would you like to go on a trip? Oh no I meant Monday' \
   → 'for Monday would you like to go on a trip?' \
3) 'Meet me at 3pm wait I meant 4pm' \
   → 'Meet me at 4pm' \
4) 'Send it to Sarah I mean Sandra' \
   → 'Send it to Sandra' \
5) 'The deadline is through Tuesday I mean it can slip' \
   → 'The deadline is through Tuesday I mean it can slip' (unchanged — discourse) \
6) 'The meeting is tomorrow no wait Friday' \
   → 'The meeting is Friday' \
7) 'Daniel walked out. I meant Samuel walked out' \
   → 'Samuel walked out' \
Never leave both the mistake and the correction in the output.";

const GRAMMAR_RULES: &str = "\
Grammarly-style cleanup (always apply): \
- Fix grammar, subject-verb agreement, articles (a/an/the), and awkward phrasing. \
- Fix punctuation: commas, periods, question marks, apostrophes, quotes. \
- Terminal punctuation: when the utterance is a finished statement, end with a period. \
  Never leave a trailing comma or semicolon on a completed sentence (ASR often does that). \
  Keep '?' for questions and '!' for exclamations. Do not force a period on fragments, \
  lists mid-thought, or text that clearly continues (ends with ':' or an ellipsis). \
- Capitalize sentence starts and proper nouns; fix obvious misspellings from speech. \
- Remove filler (um, uh, like, you know) when they add no meaning. \
- Improve clarity lightly — tighten run-ons — but KEEP the speaker's meaning, voice, \
  and intent. Do NOT invent facts, summarize, or change names/numbers/dates. \
- Do NOT add a greeting/sign-off the speaker did not say. \
- Return ONLY the cleaned text, no commentary or quotes around it.";

const ASR_CORRECTION_RULES: &str = "\
CRITICAL — speech-to-text errors (high priority): \
The input is an ASR transcript and often contains wrong near-homophones. \
Using surrounding context, fix obvious mishears to the word the speaker clearly meant. \
Prefer the reading that makes the sentence sensible. Examples: \
- Git / version control (VERY common): 'get'→'Git' when talking about repos, \
  push/pull/commit/clone/merge/place/branch. \
  'Did you place it to get?' → 'Did you place it to Git?' \
  'push it to get' → 'push it to Git'; 'get hub' → 'GitHub' \
- tech/cloud: 'clout'→'cloud', 'a WS'→'AWS', 'verse cell'→'Vercel', \
  'type script'→'TypeScript', 'post grass'→'Postgres' \
- product names: Deepgram, Claude, ChatGPT, Notion, Slack, CurseForge, Covenant Core \
  (do NOT rewrite 'curse forge' / 'CurseForge' to 'Cursor' — different product) \
  Cursor only when clearly the editor/IDE, not gaming/modding context \
  (do NOT assume the speaker means the MaxSpeech app itself unless truly unambiguous — \
  names like 'Maximus Dev' or similar-sounding phrases are NOT the app name) \
  'Covenant court' / 'Covenant Court' → 'Covenant Core'; \
  'Covenant corner' / 'Covenant Corner' → 'Covenant Core' \
  (do NOT rewrite to 'Covenant Corner' — the product is Covenant Core) \
- common: 'there'/'their'/'they're', 'to'/'too'/'two', 'its'/'it's' by grammar \
- numbers: ASR often inserts digits for homophones ('for'→'4', 'to'→'2', 'won'→'1'). \
  Prefer the word that fits the sentence; only use digits when the speaker clearly \
  dictated a number, code, time, or quantity (e.g. 'meet at 4pm', 'room 101'). \
  In normal prose keep spelled-out numbers as words unless obviously numeric. \
- percents (VERY common): '10 times' / 'ten times' → '10%' when the speaker meant \
  a percentage (at/by/of/about/only/discount/rate/tax/tip), NOT repetition \
  ('do it 10 times') or comparison ('10 times faster'). \
  'ten percent' / '10 percent' → '10%'. \
- contractions: ASR drops apostrophes — dont→don't, doesnt→doesn't, im→I'm, \
  ive→I've, thats→that's, youre→you're, theyre→they're, wont→won't, cant→can't. \
  'lets go/see/try' → 'let's …'. 'id like' → 'I'd like' (not user id). \
- numeral homophones (numerals=true): 'thanks 4 the'→'thanks for the', \
  'need 2 go'→'need to go', '2 much'→'too much', '1 of'→'one of', 'no 1'→'no one'. \
  Keep real quantities, times, and codes ('room 2', 'meet at 4pm', 'version 2'). \
- split product names: 'type script'→TypeScript, 'java script'→JavaScript, \
  'super base'→Supabase, 'verse cell'→Vercel, 'cloud flare'→Cloudflare, \
  'chat gpt'→ChatGPT, 'open ai'→OpenAI, 'vs code'→VS Code, \
  'curse forge'→CurseForge (not Cursor), 'post grass'→Postgres, \
  'covenant court'/'covenant corner'/'covenant core'→Covenant Core. \
- 'could of'/'would of'/'should of' → could've/would've/should've \
  (unless 'of the/a/course'). \
- comparatives: 'better then' / 'more then' / 'rather then' → than. \
- 'to much' / 'to many' / 'to late' → too. \
- 'its a' / 'its not' / 'its been' → it's; 'your going' / 'your welcome' → you're. \
- possessives: if a known name appears as Names, restore Name's. \
Do NOT invent new content. Only swap clearly wrong ASR tokens. \
Do NOT change ordinary English 'get' ('I want to get coffee'). \
If both readings are plausible, keep the transcript as-is.";

const MULTILINGUAL_RULES: &str = "\
CRITICAL — multilingual / code-switched dictation: \
The transcript may mix languages in one utterance (e.g. Russian then English). \
- Preserve EVERY language and script exactly as spoken. \
- Do NOT translate between languages. \
- Do NOT transliterate Cyrillic, CJK, Arabic, etc. into Latin letters. \
- Do NOT drop words from a language you understand less well. \
- Only lightly fix punctuation/spacing; leave mixed-language wording intact. \
- English self-correction rules apply only to clearly English correction chatter.";

fn system_prompt_for_tone(tone: &str, multilingual: bool) -> String {
    let base = match tone {
        "casual" => {
            "You are a Grammarly-like dictation assistant. Rewrite in a casual, terse chat style. \
             Prefer lowercase; skip a trailing period. Keep it brief. \
             Still fix grammar/clarity so it reads cleanly as a message."
        }
        "formal" => {
            "You are a Grammarly-like dictation assistant. Rewrite in a professional, formal style \
             suitable for email: proper capitalization, punctuation, and complete sentences."
        }
        "code" => {
            "You are a Grammarly-like dictation assistant for a programmer. Clean up grammar and \
             use precise technical terms. If it sounds like a code comment, format it as one."
        }
        "prose" => {
            "You are a Grammarly-like dictation assistant. Rewrite as clean prose with proper \
             paragraphs, punctuation, and grammar."
        }
        _ => {
            "You are a Grammarly-like dictation assistant. Clean up grammar, punctuation, and \
             clarity while keeping the original meaning and style."
        }
    };
    if multilingual {
        format!(
            "{base}\n\n{GRAMMAR_RULES}\n\n{ASR_CORRECTION_RULES}\n\n{SELF_CORRECTION_RULES}\n\n{MULTILINGUAL_RULES}"
        )
    } else {
        format!("{base}\n\n{GRAMMAR_RULES}\n\n{ASR_CORRECTION_RULES}\n\n{SELF_CORRECTION_RULES}")
    }
}

fn dictionary_prompt_block(terms: &[String]) -> String {
    let cleaned: Vec<&str> = terms
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .take(60)
        .collect();
    if cleaned.is_empty() {
        return String::new();
    }
    format!(
        "\n\nPreferred vocabulary (spell and capitalize exactly when the user says these; restore Name's possessives): {}.",
        cleaned.join(", ")
    )
}

fn with_dictionary(system: String, terms: &[String]) -> String {
    let block = dictionary_prompt_block(terms);
    if block.is_empty() {
        system
    } else {
        format!("{system}{block}")
    }
}

/// True when the transcript likely contains non-Latin script (Cyrillic, CJK, Arabic, etc.).
pub fn has_non_latin_script(text: &str) -> bool {
    text.chars().any(|c| {
        let n = c as u32;
        // Cyrillic, Greek, Hebrew, Arabic, Devanagari, CJK, Hangul, Thai, etc.
        (0x0400..=0x04FF).contains(&n)
            || (0x0500..=0x052F).contains(&n)
            || (0x0370..=0x03FF).contains(&n)
            || (0x0590..=0x05FF).contains(&n)
            || (0x0600..=0x06FF).contains(&n)
            || (0x0900..=0x097F).contains(&n)
            || (0x0E00..=0x0E7F).contains(&n)
            || (0x3040..=0x30FF).contains(&n)
            || (0x3400..=0x9FFF).contains(&n)
            || (0xAC00..=0xD7AF).contains(&n)
    })
}

const WEEKDAYS: &[&str] = &[
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "mon",
    "tue",
    "tues",
    "wed",
    "thu",
    "thur",
    "thurs",
    "fri",
    "sat",
    "sun",
];

fn bare_alpha(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphabetic())
        .collect::<String>()
        .to_lowercase()
}

fn word_is_weekday(word: &str) -> bool {
    WEEKDAYS.contains(&bare_alpha(word).as_str())
}

fn looks_like_discourse_filler(correction: &str) -> bool {
    let words: Vec<&str> = correction.split_whitespace().collect();
    if words.is_empty() {
        return true;
    }
    // Real replacements are short (name, day, time). Long clauses after "I mean" are filler.
    if words.len() > 3 {
        return true;
    }
    let first = bare_alpha(words[0]);
    matches!(
        first.as_str(),
        "it" | "that" | "because" | "we" | "you" | "i" | "the" | "this" | "there"
            | "he" | "she" | "they" | "weve" | "im" | "its" | "thats"
    )
}

fn apply_weekday_correction(words: &mut Vec<String>, corr_words: &[&str]) -> bool {
    let Some(corr_day_i) = corr_words.iter().rposition(|w| word_is_weekday(w)) else {
        return false;
    };
    let Some(mistaken_i) = words.iter().rposition(|w| word_is_weekday(w)) else {
        return false;
    };

    // Preserve original casing style lightly: keep replacement text as spoken.
    words[mistaken_i] = corr_words[corr_day_i].to_string();

    // If correction includes a leading prep (on/for/through/this/next), and the
    // mistaken day also has one, leave the existing prep alone — only the day swaps.
    // If correction is just the day, we're done.
    let _ = corr_day_i;
    true
}

/// Fix weak ASR endings so finished dictation doesn't land with a trailing comma.
/// Keeps `?` / `!` / `.` / `…`, leaves casual tone without forcing a new period,
/// and skips short fragments that don't look like full sentences.
pub fn normalize_terminal_punctuation(text: &str, tone: &str) -> String {
    let s = text.trim_end();
    if s.is_empty() {
        return String::new();
    }

    let last = s.chars().last().unwrap_or('\0');

    // Already a strong sentence end (including ellipsis / multi-char …).
    if matches!(last, '.' | '!' | '?' | '…') || s.ends_with("...") {
        return s.to_string();
    }

    // Closing quotes/brackets after content — leave alone if already punctuated inside.
    if matches!(last, '"' | '\'' | ')' | ']' | '}') {
        return s.to_string();
    }

    // Intro / list lead-in — not a finished sentence.
    if last == ':' {
        return s.to_string();
    }

    // Deepgram often ends a held utterance with "," or ";" — treat as a full stop.
    if matches!(last, ',' | ';') {
        let stem = s[..s.len() - last.len_utf8()].trim_end();
        if stem.is_empty() {
            return s.to_string();
        }
        return format!("{stem}.");
    }

    // No terminal punctuation: add a period when it reads like a finished sentence.
    // Casual chat tone prefers no trailing period.
    if tone == "casual" || !last.is_alphanumeric() {
        return s.to_string();
    }
    if looks_like_finished_sentence(s) {
        return format!("{s}.");
    }
    s.to_string()
}

fn looks_like_finished_sentence(s: &str) -> bool {
    let words: Vec<&str> = s.split_whitespace().collect();
    // One- or two-word replies ("yes", "ok thanks") shouldn't get a forced period.
    if words.len() < 3 {
        return false;
    }
    let first = s.chars().find(|c| !c.is_whitespace()).unwrap_or('\0');
    // Prefer capitalized starts; still accept longer uncapitalized dictation.
    first.is_uppercase() || words.len() >= 5
}

/// Local heuristic: fix "… Tuesday oh no I meant Monday" without needing an LLM.
pub fn local_self_correct(text: &str) -> String {
    let after_markers = local_self_correct_markers(text);
    local_asr_cleanup(&after_markers)
}

/// Fix common ASR mangling that shouldn't wait on an LLM (quick sessions skip enhance).
pub fn local_asr_cleanup(text: &str) -> String {
    let after_contractions = fix_spoken_contractions(text);
    let after_numerals = fix_numeral_homophones(&after_contractions);
    let after_homophones = fix_common_homophones(&after_numerals);
    let after_percent_word = fix_spoken_percent_word(&after_homophones);
    fix_percent_heard_as_times(&after_percent_word)
}

fn split_word_punct(w: &str) -> (&str, &str, &str) {
    let leading_len = w
        .chars()
        .take_while(|c| matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '"' | '(' | '['))
        .map(|c| c.len_utf8())
        .sum::<usize>();
    let trailing_len = w[leading_len..]
        .chars()
        .rev()
        .take_while(|c| matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '"' | ')' | ']'))
        .map(|c| c.len_utf8())
        .sum::<usize>();
    let mid_end = w.len() - trailing_len;
    (&w[..leading_len], &w[leading_len..mid_end], &w[mid_end..])
}

fn copy_casing(src: &str, dest: &str) -> String {
    let alpha: String = src.chars().filter(|c| c.is_alphabetic()).collect();
    if !alpha.is_empty() && alpha.chars().all(|c| c.is_uppercase()) {
        return dest.to_uppercase();
    }
    let Some(first) = src.chars().find(|c| c.is_alphabetic()) else {
        return dest.to_string();
    };
    if first.is_uppercase() {
        let mut chars = dest.chars();
        if let Some(d0) = chars.next() {
            return d0.to_uppercase().collect::<String>() + chars.as_str();
        }
    }
    dest.to_string()
}

fn next_bare_lower(words: &[&str], i: usize) -> String {
    words
        .get(i + 1)
        .map(|n| split_word_punct(n).1.to_ascii_lowercase())
        .unwrap_or_default()
}

fn fix_spoken_contractions(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return text.to_string();
    }
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    for (i, w) in words.iter().enumerate() {
        let (lead, bare, trail) = split_word_punct(w);
        if bare.chars().all(|c| !c.is_alphabetic() || c.is_ascii_uppercase())
            && matches!(bare, "IM" | "ID" | "OK")
        {
            out.push((*w).to_string());
            continue;
        }
        let lower = bare.to_ascii_lowercase();
        if let Some(repl) = contraction_for(&lower, &next_bare_lower(&words, i)) {
            out.push(format!("{lead}{}{trail}", copy_casing(bare, repl)));
        } else {
            out.push((*w).to_string());
        }
    }
    out.join(" ")
}

fn contraction_for(lower: &str, next: &str) -> Option<&'static str> {
    if lower == "lets" {
        return matches!(
            next,
            "go" | "see" | "try" | "make" | "get" | "do" | "start" | "talk"
                | "look" | "check" | "wait" | "say" | "take" | "put" | "add"
                | "keep" | "move" | "open" | "use" | "run" | "build" | "fix"
                | "test" | "ship" | "meet" | "eat" | "play" | "watch" | "read"
                | "write" | "call" | "ask" | "stop" | "begin"
        )
        .then_some("let's");
    }
    if lower == "id" {
        return matches!(
            next,
            "like" | "love" | "rather" | "be" | "have" | "want" | "prefer"
                | "say" | "go" | "do" | "suggest" | "recommend"
        )
        .then_some("I'd");
    }
    Some(match lower {
        "dont" => "don't",
        "doesnt" => "doesn't",
        "didnt" => "didn't",
        "wont" => "won't",
        "cant" => "can't",
        "isnt" => "isn't",
        "arent" => "aren't",
        "wasnt" => "wasn't",
        "werent" => "weren't",
        "havent" => "haven't",
        "hasnt" => "hasn't",
        "hadnt" => "hadn't",
        "wouldnt" => "wouldn't",
        "couldnt" => "couldn't",
        "shouldnt" => "shouldn't",
        "mustnt" => "mustn't",
        "im" => "I'm",
        "ive" => "I've",
        "youre" => "you're",
        "theyre" => "they're",
        "weve" => "we've",
        "youve" => "you've",
        "theyve" => "they've",
        "thats" => "that's",
        "whats" => "what's",
        "whos" => "who's",
        "wheres" => "where's",
        "heres" => "here's",
        "theres" => "there's",
        "shes" => "she's",
        "hes" => "he's",
        _ => return None,
    })
}

fn is_quantity_prev(prev: &str) -> bool {
    matches!(
        prev,
        "at" | "around" | "about" | "room" | "page" | "version" | "v"
            | "chapter" | "item" | "number" | "line" | "port" | "issue"
            | "age" | "aged" | "volume" | "size" | "count" | "plus" | "minus"
            | "versus" | "vs" | "episode" | "season" | "track" | "level"
            | "floor" | "apartment" | "apt" | "suite" | "gate" | "build"
            | "revision" | "model" | "of" | "no" | "than" | "between" | "over"
            | "under" | "from" | "last" | "next" | "first" | "step" | "part"
            | "day" | "days" | "hour" | "hours" | "minute" | "minutes" | "week"
            | "weeks" | "month" | "months" | "year" | "years" | "dollar"
            | "dollars" | "pound" | "pounds" | "euro" | "euros" | "percent"
    )
}

fn is_unit_or_quantity_next(next: &str) -> bool {
    if next.chars().all(|c| c.is_ascii_digit()) && !next.is_empty() {
        return true;
    }
    matches!(
        next,
        "times" | "time" | "percent" | "percentage" | "pm" | "am" | "st"
            | "nd" | "rd" | "th" | "dollars" | "cents" | "minutes" | "hours"
            | "seconds" | "days" | "weeks" | "months" | "years" | "people"
            | "items" | "plus" | "minus" | "bucks" | "km" | "miles" | "meters"
            | "kg" | "lbs" | "gb" | "mb" | "kb" | "tb" | "ghz" | "mhz" | "px"
            | "bit" | "bits" | "bytes"
    )
}

fn is_for_next(next: &str) -> bool {
    matches!(
        next,
        "the" | "a" | "an" | "you" | "me" | "us" | "them" | "him" | "her"
            | "it" | "this" | "that" | "those" | "these" | "my" | "your" | "our"
            | "their" | "his" | "now" | "later" | "today" | "tomorrow" | "tonight"
            | "example" | "instance" | "sure" | "real" | "once" | "all" | "each"
            | "every" | "some" | "any" | "more" | "less" | "good" | "better"
            | "worse" | "work" | "school" | "dinner" | "lunch" | "breakfast"
            | "meeting" | "everyone" | "somebody" | "someone" | "anyone"
            | "anybody" | "both" | "either" | "neither" | "free" | "sale"
            | "what" | "which" | "whom" | "whose" | "why" | "how" | "reference"
            | "context" | "review" | "approval" | "testing" | "production"
            | "monday" | "tuesday" | "wednesday" | "thursday" | "friday"
            | "saturday" | "sunday"
    )
}

fn is_too_next(next: &str) -> bool {
    matches!(
        next,
        "much" | "many" | "late" | "bad" | "far" | "soon" | "long" | "short"
            | "early" | "hard" | "easy" | "close" | "big" | "small" | "often"
            | "fast" | "slow" | "high" | "low" | "old" | "young" | "tired"
            | "busy" | "good" | "well" | "loud" | "quiet" | "hot" | "cold"
            | "expensive" | "cheap" | "heavy" | "light" | "few" | "little"
    )
}

fn is_to_next(next: &str) -> bool {
    matches!(
        next,
        "the" | "a" | "an" | "be" | "do" | "go" | "get" | "make" | "see"
            | "say" | "have" | "know" | "think" | "try" | "find" | "take"
            | "come" | "give" | "keep" | "let" | "put" | "use" | "work" | "me"
            | "you" | "us" | "them" | "him" | "her" | "it" | "this" | "that"
            | "my" | "your" | "our" | "their" | "his" | "who" | "whom" | "which"
            | "what" | "where" | "when" | "why" | "how" | "everyone" | "someone"
            | "anyone" | "anybody" | "somebody" | "everybody" | "check"
            | "confirm" | "ask" | "tell" | "call" | "send" | "write" | "read"
            | "open" | "close" | "start" | "stop" | "run" | "build" | "deploy"
            | "test" | "fix" | "add" | "remove" | "update" | "install" | "launch"
            | "join" | "leave" | "meet" | "eat" | "drink" | "sleep" | "wait"
            | "talk" | "listen" | "look" | "watch" | "play" | "help" | "show"
            | "pick" | "choose" | "decide" | "finish" | "complete"
    )
}

fn capitalize_if_needed(prev_orig: Option<&str>, word: &str) -> String {
    let start = match prev_orig {
        None => true,
        Some(p) => {
            let t = p.trim_end_matches(|c: char| matches!(c, '"' | '\'' | ')' | ']'));
            t.ends_with('.') || t.ends_with('!') || t.ends_with('?')
        }
    };
    if !start {
        return word.to_string();
    }
    let mut chars = word.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => word.to_string(),
    }
}

fn fix_numeral_homophones(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return text.to_string();
    }
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    for i in 0..words.len() {
        let w = words[i];
        let (lead, bare, trail) = split_word_punct(w);
        let next = next_bare_lower(&words, i);
        let prev = out
            .last()
            .map(|p| split_word_punct(p).1.to_ascii_lowercase())
            .unwrap_or_default();

        let mapped = if prev == "no" && bare == "1" {
            Some("one")
        } else if matches!(bare, "1" | "2" | "4")
            && !is_quantity_prev(&prev)
            && !is_unit_or_quantity_next(&next)
        {
            match bare {
                "4" if is_for_next(&next) => Some("for"),
                "2" if is_too_next(&next) => Some("too"),
                "2" if is_to_next(&next) => Some("to"),
                "1" if next == "of" => Some("one"),
                _ => None,
            }
        } else {
            None
        };

        if let Some(word) = mapped {
            let cased = capitalize_if_needed(out.last().map(|s| s.as_str()), word);
            out.push(format!("{lead}{cased}{trail}"));
        } else {
            out.push(w.to_string());
        }
    }
    out.join(" ")
}

fn is_its_contraction_next(next: &str) -> bool {
    matches!(
        next,
        "a" | "an" | "the" | "not" | "been" | "going" | "gonna" | "ok"
            | "okay" | "just" | "really" | "already" | "always" | "never"
            | "still" | "also" | "only" | "actually" | "currently" | "probably"
    )
}

fn is_youre_next(next: &str) -> bool {
    matches!(
        next,
        "going" | "gonna" | "not" | "welcome" | "being" | "doing" | "getting"
            | "looking" | "trying" | "having" | "making" | "coming"
    )
}

fn is_theyre_next(next: &str) -> bool {
    matches!(
        next,
        "going" | "gonna" | "not" | "being" | "doing" | "getting" | "looking"
            | "trying" | "here" | "there"
    )
}

fn of_after_modal_ok(after: &str) -> bool {
    !matches!(
        after,
        "course" | "the" | "a" | "an" | "this" | "that" | "it" | "his" | "her"
            | "our" | "my" | "your" | "their"
    ) && !after.is_empty()
}

fn fix_common_homophones(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return text.to_string();
    }
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        let (lead, bare, trail) = split_word_punct(w);
        let lower = bare.to_ascii_lowercase();
        let next = next_bare_lower(&words, i);
        let prev = out
            .last()
            .map(|p| split_word_punct(p).1.to_ascii_lowercase())
            .unwrap_or_default();

        if matches!(prev.as_str(), "could" | "would" | "should" | "must")
            && lower == "of"
            && of_after_modal_ok(&next)
        {
            let modal = out.pop().unwrap();
            let (m_lead, m_bare, m_trail) = split_word_punct(&modal);
            let repl = match prev.as_str() {
                "could" => "could've",
                "would" => "would've",
                "should" => "should've",
                _ => "must've",
            };
            out.push(format!(
                "{m_lead}{}{m_trail}",
                copy_casing(m_bare, repl)
            ));
            i += 1;
            continue;
        }

        if lower == "then"
            && matches!(
                prev.as_str(),
                "better" | "worse" | "rather" | "less" | "more" | "other"
            )
        {
            out.push(format!("{lead}{}{trail}", copy_casing(bare, "than")));
            i += 1;
            continue;
        }

        if lower == "to" && is_too_next(&next) {
            out.push(format!("{lead}{}{trail}", copy_casing(bare, "too")));
            i += 1;
            continue;
        }

        if lower == "its" && is_its_contraction_next(&next) {
            out.push(format!("{lead}{}{trail}", copy_casing(bare, "it's")));
            i += 1;
            continue;
        }

        if lower == "your" && is_youre_next(&next) {
            out.push(format!("{lead}{}{trail}", copy_casing(bare, "you're")));
            i += 1;
            continue;
        }

        if (lower == "their" || lower == "there") && is_theyre_next(&next) {
            out.push(format!("{lead}{}{trail}", copy_casing(bare, "they're")));
            i += 1;
            continue;
        }

        out.push(w.to_string());
        i += 1;
    }
    out.join(" ")
}

fn fix_spoken_percent_word(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 2 {
        return text.to_string();
    }
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        let bare = w.trim_matches(|c: char| matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '"' | '\''));
        let is_num = bare.chars().all(|c| c.is_ascii_digit()) && !bare.is_empty();
        let next_is_pct = words
            .get(i + 1)
            .map(|n| {
                let nb = n.trim_matches(|c: char| {
                    matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '"' | '\'')
                });
                nb.eq_ignore_ascii_case("percent")
                    || nb.eq_ignore_ascii_case("percentage")
                    || nb.eq_ignore_ascii_case("percents")
            })
            .unwrap_or(false);
        if is_num && next_is_pct {
            let pct_tok = words[i + 1];
            let trailing: String = pct_tok
                .chars()
                .rev()
                .take_while(|c| matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '"' | '\''))
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            out.push(format!("{bare}%{trailing}"));
            i += 2;
            continue;
        }
        out.push(w.to_string());
        i += 1;
    }
    out.join(" ")
}

fn fix_percent_heard_as_times(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 2 {
        return text.to_string();
    }

    let mult_next = [
        "as", "more", "faster", "slower", "larger", "bigger", "harder", "louder",
        "cheaper", "better", "worse", "higher", "lower", "greater", "smaller",
        "again", "over", "before", "after", "until", "the", "a", "an",
    ];
    let percent_prev = [
        "at", "by", "of", "about", "around", "roughly", "exactly", "only", "to",
        "from", "discount", "off", "plus", "minus", "rate", "tax", "tip",
        "interest", "fee", "up", "down", "nearly", "almost", "over", "under",
        "above", "below", "was", "is", "be", "been",
    ];

    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        let bare = w.trim_matches(|c: char| matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '"' | '\''));
        let is_num = bare.chars().all(|c| c.is_ascii_digit()) && !bare.is_empty();
        let next_is_times = words
            .get(i + 1)
            .map(|n| {
                let nb = n.trim_matches(|c: char| matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '"' | '\''));
                nb.eq_ignore_ascii_case("times") || nb.eq_ignore_ascii_case("time")
            })
            .unwrap_or(false);

        if is_num && next_is_times {
            let prev = out
                .last()
                .map(|p| {
                    p.trim_matches(|c: char| matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '"' | '\''))
                        .to_ascii_lowercase()
                })
                .unwrap_or_default();
            let after = words.get(i + 2).map(|n| {
                n.trim_matches(|c: char| matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '"' | '\''))
                    .to_ascii_lowercase()
            });
            let after_is_mult = after
                .as_deref()
                .map(|a| mult_next.contains(&a))
                .unwrap_or(false);
            let prev_suggests_pct = percent_prev.iter().any(|p| *p == prev.as_str());
            // Bare "10 times" → percent; "at 10 times" → percent; "10 times faster" stays.
            let bare_utterance = words.len() <= 2;
            if !after_is_mult && (prev_suggests_pct || bare_utterance || prev.is_empty()) {
                let times_tok = words[i + 1];
                let trailing: String = times_tok
                    .chars()
                    .rev()
                    .take_while(|c| matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '"' | '\''))
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
                out.push(format!("{bare}%{trailing}"));
                i += 2;
                continue;
            }
        }
        out.push(w.to_string());
        i += 1;
    }
    out.join(" ")
}

fn local_self_correct_markers(text: &str) -> String {
    let lower = text.to_lowercase();
    let Some((idx, content_start, kind)) = find_last_correction_marker(&lower) else {
        return text.to_string();
    };

    // ASCII markers / punctuation skips — indices are byte-safe on the original.
    if !text.is_char_boundary(idx) || !text.is_char_boundary(content_start) {
        return text.to_string();
    }

    let before = text[..idx].trim_end();
    let mut correction = text[content_start..].trim();
    correction = correction.trim_start_matches(|c: char| matches!(c, ',' | ':' | ';' | '.'));
    correction = correction.trim();
    correction = correction.trim_end_matches(|c: char| matches!(c, '.' | '!' | '?' | ','));
    correction = correction.trim();

    if before.is_empty() || correction.is_empty() {
        return text.to_string();
    }

    // "I mean it can slip" is discourse, not a word swap — leave the sentence alone.
    if kind == CorrectionMarkerKind::IMean && looks_like_discourse_filler(correction) {
        return text.to_string();
    }

    // "I met …" is often ASR for "I meant …". Only treat it as a correction when
    // the tail clearly restates / renames something from the preceding clause.
    if kind == CorrectionMarkerKind::IMet && !looks_like_restatement(before, correction) {
        return text.to_string();
    }

    let (stem, trailing_punct) = strip_trailing_punct(before);
    let mut words: Vec<String> = stem.split_whitespace().map(|w| w.to_string()).collect();
    if words.is_empty() {
        return format!("{correction}{trailing_punct}");
    }

    let corr_words: Vec<&str> = correction.split_whitespace().collect();

    // Weekday → name/restatement → last-N fallback.
    if !apply_weekday_correction(&mut words, &corr_words)
        && !apply_single_name_correction(&mut words, &corr_words)
        && !apply_suffix_restatement(&mut words, &corr_words)
    {
        let n = corr_words.len().min(words.len());
        words.truncate(words.len() - n);
        for w in corr_words {
            words.push(w.to_string());
        }
    }

    let mut out = words.join(" ");
    out.push_str(trailing_punct);
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CorrectionMarkerKind {
    Generic,
    IMean,
    IMet,
}

/// Find the last spoken-correction marker. Allows Deepgram punctuation such as
/// "I meant, Samuel…" (comma after the marker) and mid-sentence "…out. I meant…".
fn find_last_correction_marker(lower: &str) -> Option<(usize, usize, CorrectionMarkerKind)> {
    // Phrases only — trailing comma/space is handled after the match.
    let markers: &[(&str, CorrectionMarkerKind)] = &[
        ("oh no i meant", CorrectionMarkerKind::Generic),
        ("oh no, i meant", CorrectionMarkerKind::Generic),
        ("oh wait i meant", CorrectionMarkerKind::Generic),
        ("oh wait, i meant", CorrectionMarkerKind::Generic),
        ("no wait i meant", CorrectionMarkerKind::Generic),
        ("no, i meant", CorrectionMarkerKind::Generic),
        ("no i meant", CorrectionMarkerKind::Generic),
        ("wait i meant", CorrectionMarkerKind::Generic),
        ("wait, i meant", CorrectionMarkerKind::Generic),
        ("actually i meant", CorrectionMarkerKind::Generic),
        ("actually, i meant", CorrectionMarkerKind::Generic),
        ("sorry i meant", CorrectionMarkerKind::Generic),
        ("sorry, i meant", CorrectionMarkerKind::Generic),
        ("scratch that i meant", CorrectionMarkerKind::Generic),
        ("correction:", CorrectionMarkerKind::Generic),
        ("correction", CorrectionMarkerKind::Generic),
        ("i meant", CorrectionMarkerKind::Generic),
        ("i mean", CorrectionMarkerKind::IMean),
        ("i met", CorrectionMarkerKind::IMet),
        ("or rather", CorrectionMarkerKind::Generic),
    ];

    let mut best: Option<(usize, usize, CorrectionMarkerKind, usize)> = None;
    // tuple: (marker_start, content_start, kind, marker_phrase_len)

    for &(phrase, kind) in markers {
        let mut search_from = 0;
        while let Some(rel) = lower[search_from..].find(phrase) {
            let idx = search_from + rel;
            let after_phrase = idx + phrase.len();

            // Word boundary before the phrase (avoid "semi meant").
            if idx > 0 {
                let prev = lower[..idx].chars().next_back().unwrap_or('\0');
                if prev.is_alphanumeric() {
                    search_from = idx + 1;
                    continue;
                }
            }

            // After the phrase: optional comma/colon, then whitespace, then content.
            let Some(content_start) = marker_content_start(lower, after_phrase, phrase) else {
                search_from = idx + 1;
                continue;
            };

            match best {
                None => best = Some((idx, content_start, kind, phrase.len())),
                Some((bi, bcontent, _, blen)) => {
                    // Prefer the match whose correction content starts latest; on a
                    // tie, prefer the longer phrase ("oh no i meant" over "i meant").
                    if content_start > bcontent
                        || (content_start == bcontent && phrase.len() > blen)
                        || (content_start == bcontent && phrase.len() == blen && idx < bi)
                    {
                        best = Some((idx, content_start, kind, phrase.len()));
                    }
                }
            }
            search_from = idx + 1;
        }
    }

    // Bare "… no wait Friday" / "… wait no Friday" (no "I meant").
    let alt = [" no wait ", " wait no ", " wait actually "];
    for m in alt {
        if let Some(idx) = lower.rfind(m) {
            let content_start = idx + m.len();
            if content_start >= lower.len() {
                continue;
            }
            match best {
                None => {
                    best = Some((idx, content_start, CorrectionMarkerKind::Generic, m.len()))
                }
                Some((_, bcontent, _, blen)) => {
                    if content_start > bcontent
                        || (content_start == bcontent && m.len() > blen)
                    {
                        best = Some((idx, content_start, CorrectionMarkerKind::Generic, m.len()));
                    }
                }
            }
        }
    }

    best.map(|(idx, content_start, kind, _)| (idx, content_start, kind))
}

fn marker_content_start(lower: &str, after_phrase: usize, phrase: &str) -> Option<usize> {
    if after_phrase > lower.len() {
        return None;
    }
    let rest = &lower[after_phrase..];
    if rest.is_empty() {
        return None;
    }

    // "correction:" already includes the colon in the phrase.
    if phrase.ends_with(':') {
        let trimmed = rest.trim_start();
        if trimmed.is_empty() {
            return None;
        }
        return Some(after_phrase + (rest.len() - trimmed.len()));
    }

    // Require a break after the phrase so "meantimes" / "meetup" don't match.
    let first = rest.chars().next()?;
    if first.is_alphanumeric() {
        return None;
    }

    // Skip punctuation Deepgram inserts after markers ("I meant, Samuel").
    let mut i = 0;
    for c in rest.chars() {
        if matches!(c, ',' | ':' | ';' | '.' | '!' | '?' | '"' | '\'') || c.is_whitespace() {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    if i == 0 || i >= rest.len() {
        return None;
    }
    // Must have consumed at least one whitespace (possibly after a comma).
    let consumed = &rest[..i];
    if !consumed.chars().any(|c| c.is_whitespace()) {
        return None;
    }
    Some(after_phrase + i)
}

fn looks_like_name_token(word: &str) -> bool {
    let bare: String = word
        .chars()
        .filter(|c| c.is_alphabetic() || *c == '-')
        .collect();
    if bare.len() < 2 || bare.len() > 40 {
        return false;
    }
    let mut chars = bare.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => chars.all(|c| c.is_alphabetic() || c == '-'),
        _ => false,
    }
}

/// True when `correction` restates / renames something in `before` (shared tail,
/// single name swap, or near-equal length with few diffs).
fn looks_like_restatement(before: &str, correction: &str) -> bool {
    let a: Vec<String> = before
        .split_whitespace()
        .map(bare_alpha)
        .filter(|w| !w.is_empty())
        .collect();
    let b: Vec<String> = correction
        .split_whitespace()
        .map(bare_alpha)
        .filter(|w| !w.is_empty())
        .collect();
    if a.is_empty() || b.is_empty() {
        return false;
    }

    let mut overlap = 0usize;
    while overlap < a.len()
        && overlap < b.len()
        && a[a.len() - 1 - overlap] == b[b.len() - 1 - overlap]
    {
        overlap += 1;
    }
    if overlap >= 1 && (a.len() > overlap || b.len() > overlap) {
        return true;
    }

    // Single-name fix after ASR "I met": require a proper name in a multi-word
    // clause so "Yesterday I met Samuel" is not treated as a correction.
    if b.len() == 1 {
        let rep = &b[0];
        let before_words: Vec<&str> = before.split_whitespace().collect();
        if before_words.len() >= 2 {
            let has_non_initial_name = before_words.iter().skip(1).any(|w| {
                looks_like_name_token(w) && !bare_alpha(w).eq_ignore_ascii_case(rep)
            });
            let has_leading_name_clause = looks_like_name_token(before_words[0])
                && before_words.len() >= 3
                && !bare_alpha(before_words[0]).eq_ignore_ascii_case(rep);
            if has_non_initial_name || has_leading_name_clause {
                return true;
            }
        }
    }

    if a.len() == b.len() && a.len() >= 2 {
        let diffs = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
        return (1..=2).contains(&diffs);
    }

    false
}

/// "Daniel walked out. I meant Samuel" → replace the name token, keep the clause.
fn apply_single_name_correction(words: &mut Vec<String>, corr_words: &[&str]) -> bool {
    if corr_words.len() != 1 {
        return false;
    }
    let rep = corr_words[0];
    let rep_bare = bare_alpha(rep);
    if rep_bare.is_empty() {
        return false;
    }

    let Some(i) = words.iter().rposition(|w| {
        looks_like_name_token(w) && bare_alpha(w) != rep_bare
    }) else {
        return false;
    };

    words[i] = rep.to_string();
    true
}

/// "Daniel walked out" / "Samuel walked out" → keep the shared tail, take the fix.
fn apply_suffix_restatement(words: &mut Vec<String>, corr_words: &[&str]) -> bool {
    if corr_words.is_empty() {
        return false;
    }

    let mut overlap = 0usize;
    while overlap < words.len() && overlap < corr_words.len() {
        if bare_alpha(&words[words.len() - 1 - overlap])
            == bare_alpha(corr_words[corr_words.len() - 1 - overlap])
        {
            overlap += 1;
        } else {
            break;
        }
    }

    if overlap == 0 {
        return false;
    }

    let a_head = words.len() - overlap;
    let b_head = corr_words.len() - overlap;
    if a_head == 0 && b_head == 0 {
        return false;
    }

    // Shared content ("walked out") plus a short changed head → take the restatement.
    if overlap >= 2 || (overlap >= 1 && a_head <= 2 && b_head <= 2) {
        words.clear();
        for w in corr_words {
            words.push((*w).to_string());
        }
        return true;
    }

    false
}

fn strip_trailing_punct(s: &str) -> (&str, &str) {
    let trimmed = s.trim_end();
    let bytes = trimmed.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        let c = bytes[i - 1] as char;
        if matches!(c, '.' | '!' | '?' | ',' | ';' | ':') {
            i -= 1;
        } else {
            break;
        }
    }
    if i == bytes.len() {
        (trimmed, "")
    } else {
        (&trimmed[..i], &trimmed[i..])
    }
}

fn llm_key() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    secrets::resolve_llm_api_key().ok_or_else(|| "LLM API key not set".into())
}

async fn apply_tone_ex(
    text: &str,
    tone: &str,
    multilingual: bool,
    dict_terms: &[String],
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = llm_key()?;
    let system = with_dictionary(system_prompt_for_tone(tone, multilingual), dict_terms);
    call_llm(&api_key, &system, text, 1024).await
}

pub async fn rewrite_with_llm(
    text: &str,
    instruction: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = llm_key()?;

    let system = format!(
        "You are a Grammarly-like dictation assistant. Rewrite the text per the instruction. \
         Instruction: {instruction}. Only return the rewritten text, nothing else.\n\n\
         {GRAMMAR_RULES}\n\n{ASR_CORRECTION_RULES}\n\n{SELF_CORRECTION_RULES}"
    );
    call_llm(&api_key, &system, text, 1024).await
}

async fn enhance_long_dictation_ex(
    text: &str,
    tone: &str,
    multilingual: bool,
    dict_terms: &[String],
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = llm_key()?;

    let style = system_prompt_for_tone(tone, multilingual);
    let system = with_dictionary(
        format!(
            "{style}\n\nThis is a longer dictation. Apply Grammarly-style grammar, punctuation, \
             and clarity fixes throughout. Remove filler (um, uh, like). Break into clear paragraphs \
             when natural. Apply self-correction rules carefully. Do not summarize — return the full \
             cleaned transcript only."
        ),
        dict_terms,
    );
    call_llm(&api_key, &system, text, 4096).await
}

async fn cleanup_self_corrections_ex(
    text: &str,
    multilingual: bool,
    dict_terms: &[String],
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = llm_key()?;

    let multi = if multilingual {
        format!(" {MULTILINGUAL_RULES}")
    } else {
        String::new()
    };
    let system = with_dictionary(
        format!(
            "You are a Grammarly-like cleanup pass for spoken dictation. \
             {GRAMMAR_RULES} {ASR_CORRECTION_RULES} {SELF_CORRECTION_RULES}{multi} \
             Only return the cleaned text, nothing else."
        ),
        dict_terms,
    );
    call_llm(&api_key, &system, text, 2048).await
}

/// Enhance path used by the dictation pipeline (preserves code-switched scripts).
pub async fn enhance_dictation_ex(
    text: &str,
    tone: &str,
    long: bool,
    multilingual: bool,
    dict_terms: &[String],
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Local English self-correction can mangle mixed-script text — skip it when
    // the transcript already contains non-Latin characters.
    let local = if multilingual || has_non_latin_script(text) {
        text.to_string()
    } else {
        local_self_correct(text)
    };
    let multi = multilingual || has_non_latin_script(&local);
    if long {
        enhance_long_dictation_ex(&local, tone, multi, dict_terms).await
    } else if tone == "default" {
        cleanup_self_corrections_ex(&local, multi, dict_terms).await
    } else {
        apply_tone_ex(&local, tone, multi, dict_terms).await
    }
}

async fn call_llm(
    api_key: &str,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": "gpt-4o-mini",
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ],
        "max_tokens": max_tokens,
        "temperature": 0.1
    });

    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let json: serde_json::Value = resp.json().await?;

    if !status.is_success() {
        let err = json["error"]["message"]
            .as_str()
            .unwrap_or("OpenAI request failed");
        log::error!("LLM error ({status}): {err}");
        return Err(err.into());
    }

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("Empty LLM response")?
        .trim()
        .trim_matches('"')
        .to_string();

    if content.is_empty() {
        return Err("Empty LLM response".into());
    }

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::{local_asr_cleanup, local_self_correct, normalize_terminal_punctuation};

    #[test]
    fn ten_times_becomes_percent() {
        assert_eq!(local_asr_cleanup("10 times"), "10%");
        assert_eq!(local_asr_cleanup("at 10 times."), "at 10%.");
        assert_eq!(local_asr_cleanup("10 percent"), "10%");
        // Multiplication / repetition sense stays.
        assert_eq!(local_asr_cleanup("10 times faster"), "10 times faster");
        assert_eq!(local_asr_cleanup("do it 10 times"), "do it 10 times");
        assert_eq!(local_asr_cleanup("do it 10 times again"), "do it 10 times again");
    }

    #[test]
    fn restores_missing_contractions() {
        assert_eq!(local_asr_cleanup("dont worry"), "don't worry");
        assert_eq!(local_asr_cleanup("thats fine"), "that's fine");
        assert_eq!(local_asr_cleanup("im ready now"), "I'm ready now");
        assert_eq!(local_asr_cleanup("lets go"), "let's go");
        assert_eq!(local_asr_cleanup("lets the user in"), "lets the user in");
        assert_eq!(local_asr_cleanup("id like coffee"), "I'd like coffee");
        assert_eq!(local_asr_cleanup("user id is 7"), "user id is 7");
        assert_eq!(local_asr_cleanup("Doesnt work"), "Doesn't work");
    }

    #[test]
    fn reverses_numeral_homophones() {
        assert_eq!(local_asr_cleanup("thanks 4 the update"), "thanks for the update");
        assert_eq!(local_asr_cleanup("I need 2 go"), "I need to go");
        assert_eq!(local_asr_cleanup("2 much work"), "Too much work");
        assert_eq!(local_asr_cleanup("1 of us"), "One of us");
        assert_eq!(local_asr_cleanup("no 1 else"), "no one else");
        assert_eq!(local_asr_cleanup("4 the meeting"), "For the meeting");
        // Real quantities / times stay digits.
        assert_eq!(local_asr_cleanup("meet at 4pm"), "meet at 4pm");
        assert_eq!(local_asr_cleanup("room 2"), "room 2");
        assert_eq!(local_asr_cleanup("version 2"), "version 2");
        assert_eq!(local_asr_cleanup("I have 2 apples"), "I have 2 apples");
    }

    #[test]
    fn fixes_common_english_homophones() {
        assert_eq!(local_asr_cleanup("its a bug"), "it's a bug");
        assert_eq!(local_asr_cleanup("its own place"), "its own place");
        assert_eq!(local_asr_cleanup("your going to love this"), "you're going to love this");
        assert_eq!(local_asr_cleanup("your laptop"), "your laptop");
        assert_eq!(local_asr_cleanup("to much work"), "too much work");
        assert_eq!(local_asr_cleanup("better then that"), "better than that");
        assert_eq!(local_asr_cleanup("could of been worse"), "could've been worse");
        assert_eq!(local_asr_cleanup("could of course"), "could of course");
        assert_eq!(local_asr_cleanup("their going home"), "they're going home");
        assert_eq!(local_asr_cleanup("and then we left"), "and then we left");
    }

    #[test]
    fn trailing_comma_becomes_period() {
        assert_eq!(
            normalize_terminal_punctuation("This is a finished sentence,", "default"),
            "This is a finished sentence."
        );
        assert_eq!(
            normalize_terminal_punctuation("Also done;", "default"),
            "Also done."
        );
    }

    #[test]
    fn keeps_question_and_exclamation() {
        assert_eq!(
            normalize_terminal_punctuation("Are you free tomorrow?", "default"),
            "Are you free tomorrow?"
        );
        assert_eq!(
            normalize_terminal_punctuation("That was amazing!", "default"),
            "That was amazing!"
        );
    }

    #[test]
    fn adds_period_to_complete_statement() {
        assert_eq!(
            normalize_terminal_punctuation("Please send the report today", "default"),
            "Please send the report today."
        );
    }

    #[test]
    fn casual_tone_does_not_force_period() {
        assert_eq!(
            normalize_terminal_punctuation("hey can you check this later", "casual"),
            "hey can you check this later"
        );
        // Weak ASR endings still get cleaned even in casual.
        assert_eq!(
            normalize_terminal_punctuation("hey can you check this later,", "casual"),
            "hey can you check this later."
        );
    }

    #[test]
    fn short_fragment_stays_unpunctuated() {
        assert_eq!(normalize_terminal_punctuation("ok", "default"), "ok");
        assert_eq!(
            normalize_terminal_punctuation("got it", "default"),
            "got it"
        );
    }

    #[test]
    fn corrects_tuesday_to_monday() {
        let out = local_self_correct(
            "Would you like to go on a trip on Tuesday? Oh no I meant Monday",
        );
        assert!(out.to_lowercase().contains("monday"), "{out}");
        assert!(!out.to_lowercase().contains("tuesday"), "{out}");
        assert!(!out.to_lowercase().contains("meant"), "{out}");
    }

    #[test]
    fn corrects_early_tuesday_to_monday() {
        let out = local_self_correct(
            "for Tuesday would you like to go on a trip? Oh no I meant Monday",
        );
        assert!(out.to_lowercase().contains("monday"), "{out}");
        assert!(!out.to_lowercase().contains("tuesday"), "{out}");
        assert!(!out.to_lowercase().contains("meant"), "{out}");
    }

    #[test]
    fn corrects_thursday_to_friday() {
        let out = local_self_correct("Let's meet through Thursday I meant Friday");
        assert!(out.to_lowercase().contains("friday"), "{out}");
        assert!(!out.to_lowercase().contains("thursday"), "{out}");
        assert!(out.to_lowercase().contains("through"), "{out}");
    }

    #[test]
    fn keeps_through_tuesday_when_i_mean_is_discourse() {
        let out =
            local_self_correct("The deadline is through Tuesday I mean it can slip");
        assert!(out.to_lowercase().contains("through tuesday"), "{out}");
        assert!(out.to_lowercase().contains("i mean"), "{out}");
    }

    #[test]
    fn corrects_i_mean() {
        let out = local_self_correct("Send it to Sarah I mean Sandra");
        assert!(out.to_lowercase().contains("sandra"), "{out}");
        assert!(!out.to_lowercase().contains("sarah"), "{out}");
    }

    #[test]
    fn corrects_full_sentence_restatement_after_period() {
        let out = local_self_correct("Daniel walked out. I meant Samuel walked out");
        let lower = out.to_lowercase();
        assert!(lower.contains("samuel"), "{out}");
        assert!(lower.contains("walked out"), "{out}");
        assert!(!lower.contains("daniel"), "{out}");
        assert!(!lower.contains("meant"), "{out}");
    }

    #[test]
    fn corrects_full_sentence_restatement_without_period() {
        let out = local_self_correct("Daniel walked out I meant Samuel walked out");
        let lower = out.to_lowercase();
        assert!(lower.contains("samuel"), "{out}");
        assert!(!lower.contains("daniel"), "{out}");
        assert!(!lower.contains("meant"), "{out}");
    }

    #[test]
    fn corrects_i_meant_with_comma_after_marker() {
        // Deepgram punctuate often inserts a comma after transitional phrases.
        let out = local_self_correct("Daniel walked out. I meant, Samuel walked out");
        let lower = out.to_lowercase();
        assert!(lower.contains("samuel"), "{out}");
        assert!(!lower.contains("daniel"), "{out}");
        assert!(!lower.contains("meant"), "{out}");
    }

    #[test]
    fn corrects_prefixed_restatement() {
        let out = local_self_correct("So Daniel walked out. I meant Samuel walked out");
        let lower = out.to_lowercase();
        assert!(lower.contains("samuel"), "{out}");
        assert!(lower.contains("walked out"), "{out}");
        assert!(!lower.contains("daniel"), "{out}");
        assert!(!lower.contains("meant"), "{out}");
    }

    #[test]
    fn corrects_single_name_after_i_meant() {
        let out = local_self_correct("Daniel walked out. I meant Samuel");
        let lower = out.to_lowercase();
        assert!(lower.contains("samuel"), "{out}");
        assert!(lower.contains("walked out"), "{out}");
        assert!(!lower.contains("daniel"), "{out}");
        assert!(!lower.contains("meant"), "{out}");
    }

    #[test]
    fn corrects_asr_i_met_when_restating() {
        let out = local_self_correct("Daniel walked out. I met Samuel walked out");
        let lower = out.to_lowercase();
        assert!(lower.contains("samuel"), "{out}");
        assert!(!lower.contains("daniel"), "{out}");
        // Genuine meeting chatter should not leave "i met" if we applied a fix.
        assert!(!lower.contains("i met"), "{out}");
    }

    #[test]
    fn keeps_genuine_i_met_meeting() {
        let out = local_self_correct("Yesterday I met Samuel at noon");
        let lower = out.to_lowercase();
        assert!(lower.contains("yesterday"), "{out}");
        assert!(lower.contains("i met"), "{out}");
        assert!(lower.contains("samuel"), "{out}");
    }
}
