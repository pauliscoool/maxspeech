use crate::store::Store;

/// Expand macro triggers in the text. If a macro matches, return its expansion.
/// Also apply dictionary casing / whole-word corrections for known terms.
pub fn expand_macros(text: &str, store: &Store) -> String {
    let mut macros = store.get_macros().unwrap_or_default();
    // Longest triggers first so "sign off" wins over "sign".
    macros.sort_by(|a, b| b.trigger.len().cmp(&a.trigger.len()));
    let lower = text.to_lowercase();

    // Check for full macro matches first
    for m in &macros {
        if lower.trim() == m.trigger.to_lowercase() {
            return expand_template_vars(&m.expansion);
        }
    }

    // Inline expansions: whole-word / phrase boundaries, all occurrences, UTF-8 safe.
    let mut result = text.to_string();
    for m in &macros {
        let expansion = expand_template_vars(&m.expansion);
        result = replace_phrase_ci(&result, &m.trigger, &expansion);
    }

    // Apply dictionary corrections as whole-word replacements (case-insensitive).
    // Putting "cloud" in the dictionary forces "Cloud"/"CLOUD" → preferred casing,
    // and also helps when ASR returns a differently-cased product name.
    let dict = store.get_dictionary().unwrap_or_default();
    let dict_words: Vec<String> = dict
        .iter()
        .map(|w| w.word.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect();
    for want in &dict_words {
        result = replace_whole_word_ci(&result, want);
    }
    result = apply_learned_possessives(&result, &dict_words);
    result = fix_common_asr(&result);
    // User-learned pairs win over builtins so a correction sticks next time.
    super::learn_substitutions::apply(&result, store)
}

/// Expand `{clipboard}`, `{date}`, `{time}` in snippet templates.
fn expand_template_vars(expansion: &str) -> String {
    let mut out = expansion.to_string();
    if out.contains("{clipboard}") {
        let clip = arboard::Clipboard::new()
            .ok()
            .and_then(|mut cb| cb.get_text().ok())
            .unwrap_or_default();
        out = out.replace("{clipboard}", &clip);
    }
    if out.contains("{date}") {
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        out = out.replace("{date}", &date);
    }
    if out.contains("{time}") {
        let time = chrono::Local::now().format("%H:%M").to_string();
        out = out.replace("{time}", &time);
    }
    out
}

/// Persist name-like corrections so future ASR prefers the fixed spelling.
/// Learns tokens that look like proper names / jargon when the user (or
/// self-correction) changes one word into another.
pub fn learn_name_corrections(before: &str, after: &str, store: &Store) {
    for w in name_corrections(before, after) {
        if store.add_dict_word(&w, 1.0).is_ok() {
            log::info!("Learned name correction: {w}");
        }
    }
}

/// Terms worth adding to the dictionary after `before` became `after`.
fn name_corrections(before: &str, after: &str) -> Vec<String> {
    let before_t = before.trim();
    let after_t = after.trim();
    if before_t.is_empty() || after_t.is_empty() || before_t == after_t {
        return Vec::new();
    }

    let strip = |s: &str| -> Vec<String> {
        s.split_whitespace()
            .map(|w| {
                w.trim_matches(|c: char| matches!(c, ',' | '.' | '!' | '?' | ';' | ':' | '"' | '\''))
                    .to_string()
            })
            .filter(|w| !w.is_empty())
            .collect()
    };

    let a = strip(before_t);
    let b = strip(after_t);
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    // ASR capitalizes the first word of every sentence; that says nothing about a name.
    let mut sentence_start: Vec<bool> = Vec::with_capacity(b.len());
    let mut at_start = true;
    for raw in after_t.split_whitespace() {
        let core = raw.trim_matches(|c: char| {
            matches!(c, ',' | '.' | '!' | '?' | ';' | ':' | '"' | '\'')
        });
        if !core.is_empty() {
            sentence_start.push(at_start);
        }
        at_start = raw.ends_with(['.', '!', '?']);
    }
    let is_name = |idx: usize, w: &str| {
        looks_like_name_term(w)
            && (!sentence_start.get(idx).copied().unwrap_or(false) || has_strong_name_shape(w))
    };

    // Align from the end: self-corrections usually replace the last N words.
    let mut i = a.len();
    let mut j = b.len();
    while i > 0 && j > 0 {
        if a[i - 1].eq_ignore_ascii_case(&b[j - 1]) {
            i -= 1;
            j -= 1;
        } else {
            break;
        }
    }
    // Tokens that differ at the end of `after` are the corrections.
    let changed: Vec<String> = b[j..]
        .iter()
        .enumerate()
        .filter(|(k, w)| is_name(j + k, w))
        .map(|(_, w)| w.clone())
        .collect();
    if !changed.is_empty() {
        return changed;
    }

    // Also pick up mid-sentence single-token diffs of equal length.
    if a.len() == b.len() {
        return a
            .iter()
            .zip(b.iter())
            .enumerate()
            .filter(|(k, (wa, wb))| !wa.eq_ignore_ascii_case(wb) && is_name(*k, wb))
            .map(|(_, (_, wb))| wb.clone())
            .collect();
    }
    Vec::new()
}

fn looks_like_learnable_term(word: &str) -> bool {
    let w = word.trim();
    if w.len() < 2 || w.len() > 40 {
        return false;
    }
    let letters = w.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 2 {
        return false;
    }
    let lower = w.to_ascii_lowercase();
    !is_stop_word(&lower)
}

fn looks_like_name_term(word: &str) -> bool {
    let w = word.trim();
    if !looks_like_learnable_term(w) {
        return false;
    }
    // Contractions and possessives ("It's", "Look's") are grammar, not names.
    if w.contains('\'') || w.contains('\u{2019}') {
        return false;
    }
    let first = w.chars().next().unwrap();
    has_strong_name_shape(w) || first.is_uppercase()
}

/// Everyday English. Boosting or force-casing one of these (or learning a bare rule
/// that rewrites it) changes every dictation, not just the one that taught the app.
const COMMON_WORDS: &str = "\
a about above across actually add after afterwards again against ago all almost alone along already also \
although always am among an and another any anyone anything anyway anywhere are around as ask at away back \
be became because become been before behind being below beside besides best better between beyond both but \
by call came can cannot care case change check clear click close come comes could day days did do does doing \
done down drop during each either else end enough especially even ever every everyone everything everywhere \
except few find first five for four from further get gets getting give given go goes going gone good got great \
had has have having he hello help hence her here hers herself hey hi him himself his how however hundred i if \
in indeed instead into is it its itself just keep keeps kept kind know last later least leave less let like \
likely little long look looks made make makes many may maybe me mean might mind mine more moreover most mostly \
move much must my myself near need needs neither never new next nine no nobody none nor not note nothing now \
nowhere number of off often oh ok okay old on once one only onto open or other others otherwise our ours \
ourselves out over own part past people per perhaps person place please point press problem put quite rather \
read really remove right run said same say says see seem seems send set seven several shall she should show \
side since six so some someone something sometimes somewhere sorry start still stop such sure take tell ten \
than thank thanks that the their theirs them themselves then there therefore these they thing things think \
this those though three through throughout thus till time to today together tomorrow too took toward towards \
try turn two under underneath until up upon us use used using very want wants was way we week well went were \
what whatever when whenever where whereas whether which while who whoever whole whom whose why will with within \
without word words work would write yes yesterday yet you your yours yourself yourselves \
activity app apps answer bug button code company copy cut data delete desktop download edit email example \
file files fix folder home house idea issue item items line lines list menu message messages name names \
office option options page paste plan project question reason release result results save screen section \
seconds settings step steps system task tasks team test tests text undo update upload user users version \
window yeah yep \
monday tuesday wednesday thursday friday saturday sunday um uh";

pub(super) fn is_common_word(lower: &str) -> bool {
    static SET: std::sync::OnceLock<std::collections::HashSet<&'static str>> =
        std::sync::OnceLock::new();
    SET.get_or_init(|| COMMON_WORDS.split_whitespace().collect())
        .contains(lower)
}

fn is_stop_word(lower: &str) -> bool {
    is_common_word(lower)
}

/// Shapes that carry a name/term signal beyond a leading capital: inner capitals
/// (WhisperFlow), acronyms (VPS), hyphens, digits. Sentence-initial "If"/"So" have none.
fn has_strong_name_shape(w: &str) -> bool {
    let inner_upper = w.chars().skip(1).any(|c| c.is_uppercase());
    let acronym = (2..=6).contains(&w.len()) && w.chars().all(|c| c.is_ascii_uppercase());
    let mixed_digit =
        w.chars().any(|c| c.is_ascii_digit()) && w.chars().any(|c| c.is_alphabetic());
    inner_upper || acronym || mixed_digit || w.contains('-')
}

/// Restore `Name's` when the dictionary has `Name` and ASR emitted `Names`.
fn apply_learned_possessives(text: &str, names: &[String]) -> String {
    let mut result = text.to_string();
    let mut candidates: Vec<&str> = names
        .iter()
        .map(|n| n.trim())
        .filter(|n| {
            looks_like_name_term(n)
                && !n.contains('\'')
                && !n.ends_with('s')
                && !n.ends_with('S')
        })
        .collect();
    candidates.sort_by(|a, b| b.len().cmp(&a.len()));
    for name in candidates {
        let from = format!("{name}s");
        let to = format!("{name}'s");
        result = replace_phrase_ci(&result, &from, &to);
    }
    result
}

/// Deterministic fixes for frequent English ASR near-homophones (esp. Git ↔ get).
fn fix_common_asr(text: &str) -> String {
    let mut result = text.to_string();

    // Phrase-level first (longest matches).
    for (from, to) in [
        ("get hub", "GitHub"),
        ("git hub", "GitHub"),
        ("place it to get", "place it to Git"),
        ("placed it to get", "placed it to Git"),
        ("push it to get", "push it to Git"),
        ("pushed it to get", "pushed it to Git"),
        ("push to get", "push to Git"),
        ("pushed to get", "pushed to Git"),
        ("pull from get", "pull from Git"),
        ("pulled from get", "pulled from Git"),
        ("commit to get", "commit to Git"),
        ("committed to get", "committed to Git"),
        ("clone from get", "clone from Git"),
        ("cloned from get", "cloned from Git"),
        ("merge into get", "merge into Git"),
        ("merged into get", "merged into Git"),
        ("branch on get", "branch on Git"),
        ("repo on get", "repo on Git"),
        ("repository on get", "repository on Git"),
        ("type script", "TypeScript"),
        ("java script", "JavaScript"),
        ("node js", "Node.js"),
        ("next js", "Next.js"),
        ("postgres ql", "PostgreSQL"),
        ("post grass", "Postgres"),
        ("verse cell", "Vercel"),
        ("super base", "Supabase"),
        ("cloud flare", "Cloudflare"),
        ("clout flare", "Cloudflare"),
        ("clout storage", "cloud storage"),
        ("on the clout", "on the cloud"),
        ("deep grammar", "Deepgram"),
        ("deep gram", "Deepgram"),
        ("chat gpt", "ChatGPT"),
        ("chat gbt", "ChatGPT"),
        ("open ai", "OpenAI"),
        ("git lab", "GitLab"),
        ("vs code", "VS Code"),
        ("curse forge", "CurseForge"),
        ("covenant court", "Covenant Core"),
        ("covenant corner", "Covenant Core"),
        ("covenant core", "Covenant Core"),
        ("covenantcore", "Covenant Core"),
        ("tale scale", "Tailscale"),
        ("tale-scale", "Tailscale"),
        ("tail scale", "Tailscale"),
        ("tail-scale", "Tailscale"),
        ("tailscale", "Tailscale"),
        ("graph ql", "GraphQL"),
        ("mongo db", "MongoDB"),
        ("a ws", "AWS"),
        ("o clock", "o'clock"),
        ("oclock", "o'clock"),
        ("bit bucket", "Bitbucket"),
        ("fig ma", "Figma"),
        ("my sequel", "MySQL"),
        ("my sql", "MySQL"),
        ("kubernettes", "Kubernetes"),
        ("cooper netes", "Kubernetes"),
        ("c sharp", "C#"),
        ("c plus plus", "C++"),
        ("dot net", ".NET"),
        ("j query", "jQuery"),
        ("elastic search", "Elasticsearch"),
        ("weather or not", "whether or not"),
        ("per say", "per se"),
        ("for all intensive purposes", "for all intents and purposes"),
        ("case and point", "case in point"),
        ("nip it in the butt", "nip it in the bud"),
        ("escape goat", "scapegoat"),
    ] {
        result = replace_phrase_ci(&result, from, to);
    }

    // Contextual bare "get" → "Git" after VC verbs / prepositions.
    result = fix_get_as_git(&result);
    result
}

fn fix_get_as_git(text: &str) -> String {
    // Match "... (to|into|from|on|with) get ..." when the preceding clause
    // looks like version-control speech (place/push/pull/commit/…).
    let lower = text.to_lowercase();
    let vc_hint = [
        "place", "placed", "push", "pushed", "pull", "pulled", "commit",
        "committed", "clone", "cloned", "merge", "merged", "rebase", "rebased",
        "fork", "forked", "branch", "checkout", "repo", "repository", "remote",
        "github", "gitlab", "bitbucket", "pr ", " pull request",
    ]
    .iter()
    .any(|h| lower.contains(h));
    if !vc_hint {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Whole-word "get"
        if matches_word_at(&chars, i, "get") {
            let before = preceding_token(&chars, i);
            let after = following_token(&chars, i + 3);
            let before_l = before.to_lowercase();
            let after_l = after.to_lowercase();
            let prep = matches!(
                before_l.as_str(),
                "to" | "into" | "from" | "on" | "with" | "via" | "onto"
            );
            // "get commit", "get push", "get pull", "get repo", "get branch"
            let git_noun = matches!(
                after_l.as_str(),
                "commit" | "commits" | "push" | "pull" | "repo" | "repository"
                    | "branch" | "branches" | "clone" | "merge" | "rebase"
                    | "remote" | "status" | "diff" | "log" | "ignore"
            );
            if prep || git_noun {
                out.push_str("Git");
                i += 3;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn matches_word_at(chars: &[char], i: usize, word: &str) -> bool {
    let w: Vec<char> = word.chars().collect();
    if i + w.len() > chars.len() {
        return false;
    }
    let before_ok = i == 0 || !chars[i - 1].is_alphanumeric();
    let after_i = i + w.len();
    let after_ok = after_i >= chars.len() || !chars[after_i].is_alphanumeric();
    if !before_ok || !after_ok {
        return false;
    }
    chars[i..i + w.len()]
        .iter()
        .zip(w.iter())
        .all(|(a, b)| a.to_ascii_lowercase() == *b)
}

fn preceding_token(chars: &[char], at: usize) -> String {
    if at == 0 {
        return String::new();
    }
    let mut end = at;
    while end > 0 && chars[end - 1].is_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && chars[start - 1].is_alphanumeric() {
        start -= 1;
    }
    chars[start..end].iter().collect()
}

fn following_token(chars: &[char], at: usize) -> String {
    let mut start = at;
    while start < chars.len() && chars[start].is_whitespace() {
        start += 1;
    }
    let mut end = start;
    while end < chars.len() && chars[end].is_alphanumeric() {
        end += 1;
    }
    chars[start..end].iter().collect()
}

fn replace_phrase_ci(text: &str, from: &str, to: &str) -> String {
    let from_lower = from.to_lowercase();
    if from_lower.is_empty() || !text.to_lowercase().contains(&from_lower) {
        return text.to_string();
    }
    let from_chars: Vec<char> = from_lower.chars().collect();
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let remaining = chars.len() - i;
        if remaining >= from_chars.len() {
            let slice: String = chars[i..i + from_chars.len()].iter().collect();
            if slice.to_lowercase() == from_lower {
                let before_ok = i == 0 || !chars[i - 1].is_alphanumeric();
                let after_i = i + from_chars.len();
                let after_ok = after_i >= chars.len() || !chars[after_i].is_alphanumeric();
                if before_ok && after_ok {
                    out.push_str(to);
                    i = after_i;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Replace whole-word matches of `want` (case-insensitive) with the exact `want` spelling.
fn replace_whole_word_ci(text: &str, want: &str) -> String {
    let want_lower = want.to_lowercase();
    if want_lower.is_empty() || !text.to_lowercase().contains(&want_lower) {
        return text.to_string();
    }
    let want_chars: Vec<char> = want_lower.chars().collect();
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let remaining = chars.len() - i;
        if remaining >= want_chars.len() {
            let slice: String = chars[i..i + want_chars.len()].iter().collect();
            if slice.to_lowercase() == want_lower {
                let before_ok = i == 0 || !chars[i - 1].is_alphanumeric();
                let after_i = i + want_chars.len();
                let after_ok = after_i >= chars.len() || !chars[after_i].is_alphanumeric();
                if before_ok && after_ok {
                    out.push_str(want);
                    i = after_i;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{apply_learned_possessives, fix_common_asr, replace_whole_word_ci};

    #[test]
    fn everyday_words_are_never_learned_as_names() {
        // Real junk that ended up in a user dictionary, biasing ASR and force-casing text.
        for w in [
            "If", "So", "Then", "Than", "Also", "Do", "What", "When", "Which", "Who", "Why", "Not",
            "Next", "Look", "Make", "Keep", "Sure", "Right", "Instead", "Press", "Side", "Drop",
            "Delete", "It's", "That's", "There's", "Look's", "Everything's", "Where's",
        ] {
            assert!(!super::looks_like_name_term(w), "{w} must not look like a name");
        }
    }

    #[test]
    fn real_names_and_terms_are_still_learned() {
        for w in ["Sarah", "Supabase", "WhisperFlow", "OpenCloud", "VPS", "AI", "Vercel", "Covenant"] {
            assert!(super::looks_like_name_term(w), "{w} should look like a name");
        }
    }

    #[test]
    fn local_cleanup_and_sentence_starts_teach_nothing() {
        // Contraction fix (local_asr_cleanup) changes the text but teaches no names.
        assert!(super::name_corrections("its a good day", "It's a good day").is_empty());
        assert!(super::name_corrections("look whats here", "Look what's here").is_empty());
        // Capitalized only because a sentence starts there.
        assert!(super::name_corrections("we go. instead we stay", "We go. Instead we stay").is_empty());
        assert!(super::name_corrections("send it then", "Send it. Then").is_empty());
    }

    #[test]
    fn spoken_name_correction_still_learns_the_name() {
        assert_eq!(
            super::name_corrections("send it to Sarah I mean Sandra", "send it to Sandra"),
            vec!["Sandra".to_string()]
        );
        assert_eq!(
            super::name_corrections("send it to Sarah now", "send it to Sandra now"),
            vec!["Sandra".to_string()]
        );
    }

    #[test]
    fn replaces_whole_word_casing() {
        assert_eq!(replace_whole_word_ci("use Cloud storage", "cloud"), "use cloud storage");
        assert_eq!(replace_whole_word_ci("cloudy day", "cloud"), "cloudy day");
        assert_eq!(replace_whole_word_ci("MaxSpeech rocks", "MaxSpeech"), "MaxSpeech rocks");
    }

    #[test]
    fn fixes_git_vs_get() {
        assert_eq!(
            fix_common_asr("Did you place it to get?"),
            "Did you place it to Git?"
        );
        assert_eq!(
            fix_common_asr("Did you push it to get"),
            "Did you push it to Git"
        );
        assert_eq!(fix_common_asr("open get hub"), "open GitHub");
        // Ordinary English should stay put.
        assert_eq!(
            fix_common_asr("I want to get coffee"),
            "I want to get coffee"
        );
    }

    #[test]
    fn fixes_split_product_names() {
        assert_eq!(fix_common_asr("write it in type script"), "write it in TypeScript");
        assert_eq!(fix_common_asr("deploy on verse cell"), "deploy on Vercel");
        assert_eq!(fix_common_asr("open super base"), "open Supabase");
        assert_eq!(fix_common_asr("ask chat gpt"), "ask ChatGPT");
        assert_eq!(fix_common_asr("install curse forge"), "install CurseForge");
        assert_eq!(fix_common_asr("edit in vs code"), "edit in VS Code");
        assert_eq!(fix_common_asr("on the clout"), "on the cloud");
        assert_eq!(fix_common_asr("install CurseForge"), "install CurseForge");
        assert_eq!(fix_common_asr("meet at 3 o clock"), "meet at 3 o'clock");
        assert_eq!(fix_common_asr("open fig ma"), "open Figma");
        assert_eq!(fix_common_asr("write it in c sharp"), "write it in C#");
        assert_eq!(fix_common_asr("weather or not we go"), "whether or not we go");
    }

    #[test]
    fn fixes_covenant_core_mishears() {
        assert_eq!(fix_common_asr("open Covenant court"), "open Covenant Core");
        assert_eq!(fix_common_asr("open covenant court"), "open Covenant Core");
        assert_eq!(fix_common_asr("Covenant Court"), "Covenant Core");
        assert_eq!(fix_common_asr("try Covenant corner"), "try Covenant Core");
        assert_eq!(fix_common_asr("Covenant Corner"), "Covenant Core");
        assert_eq!(fix_common_asr("covenant core"), "Covenant Core");
        assert_eq!(fix_common_asr("Covenant Core"), "Covenant Core");
        assert_eq!(fix_common_asr("CovenantCore"), "Covenant Core");
        // Unrelated "court" / "corner" stay put.
        assert_eq!(fix_common_asr("see you in court"), "see you in court");
        assert_eq!(fix_common_asr("around the corner"), "around the corner");
    }

    #[test]
    fn fixes_tailscale_mishears() {
        assert_eq!(fix_common_asr("open tail scale"), "open Tailscale");
        assert_eq!(fix_common_asr("open Tail Scale"), "open Tailscale");
        assert_eq!(fix_common_asr("connect via tailscale"), "connect via Tailscale");
        assert_eq!(fix_common_asr("connect via Tailscale"), "connect via Tailscale");
        assert_eq!(fix_common_asr("use tale scale"), "use Tailscale");
        assert_eq!(fix_common_asr("Tale Scale VPN"), "Tailscale VPN");
        assert_eq!(fix_common_asr("tail-scale funnel"), "Tailscale funnel");
        // Unrelated "tail" / "scale" stay put.
        assert_eq!(fix_common_asr("the dog wagged its tail"), "the dog wagged its tail");
        assert_eq!(fix_common_asr("scale the image"), "scale the image");
    }

    #[test]
    fn restores_possessive_from_learned_name() {
        let names = vec!["Sandra".to_string(), "Paul".to_string()];
        assert_eq!(
            apply_learned_possessives("Send it to Sandras desk", &names),
            "Send it to Sandra's desk"
        );
        assert_eq!(
            apply_learned_possessives("Pauls laptop is here", &names),
            "Paul's laptop is here"
        );
        assert_eq!(
            apply_learned_possessives("the reports are ready", &names),
            "the reports are ready"
        );
    }
}
