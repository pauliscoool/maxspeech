//! Noisy-channel spell engine: symmetric-delete candidate lookup, keyboard-aware error cost,
//! unigram + bigram language model, and a conservative auto-fix / suggest / leave decision.
use std::collections::{HashMap, HashSet};

use super::casing::apply_case;
use super::error_model::weighted_distance;

const PREFIX_LEN: usize = 6;
const MAX_EDIT: usize = 2;
const MIN_INDEXED_LEN: usize = 3;
const BACKOFF: f32 = 0.4;

/// Pairs people commonly mix up. Only ever produces a suggestion, never a silent fix.
const CONFUSION: &[&[&str]] = &[
    &["their", "there"], &["to", "too"], &["then", "than"], &["lose", "loose"],
    &["affect", "effect"], &["quiet", "quite"], &["accept", "except"], &["advice", "advise"],
    &["whether", "weather"], &["principal", "principle"], &["of", "off"],
];
const CONFUSION_MIN_COUNT: u64 = 50;
const CONFUSION_RATIO: f64 = 30.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Leave,
    AutoFix(String),
    Suggest(String),
}

/// Tunable constants. Defaults are placeholders until the eval harness (work package 11) fixes them.
#[derive(Debug, Clone)]
pub struct Params {
    pub w_err: f32,
    pub w_uni: f32,
    pub lambda: f32,
    pub auto_margin: f32,
    pub suggest_margin: f32,
    pub auto_cost_max: f32,
    pub suggest_cost_max: f32,
    pub auto_min_len: usize,
    /// A rival this close in edit cost (and not hopelessly less likely) blocks an auto-fix.
    pub rival_cost: f32,
    pub rival_score: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            w_err: 3.0,
            w_uni: 0.5,
            lambda: 0.5,
            auto_margin: 2.0,
            suggest_margin: 0.5,
            auto_cost_max: 1.0,
            suggest_cost_max: 2.0,
            auto_min_len: 4,
            rival_cost: 0.4,
            rival_score: 6.0,
        }
    }
}

pub struct Context<'a> {
    pub prev: Option<&'a str>,
}

pub struct Engine {
    words: Vec<String>,
    freq: Vec<u64>,
    ids: HashMap<String, u32>,
    total: f64,
    deletes: Vec<(u64, u32)>,
    bigrams: HashMap<(u32, u32), u64>,
    params: Params,
}

fn hash_str(s: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn delete_variants(word: &str) -> HashSet<String> {
    let truncated: String = word.chars().take(PREFIX_LEN).collect();
    let mut all = HashSet::new();
    all.insert(truncated.clone());
    let mut frontier = vec![truncated];
    for _ in 0..MAX_EDIT {
        let mut next = Vec::new();
        for w in &frontier {
            let chars: Vec<char> = w.chars().collect();
            if chars.len() <= 1 {
                continue;
            }
            for skip in 0..chars.len() {
                let v: String = chars
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != skip)
                    .map(|(_, c)| *c)
                    .collect();
                if all.insert(v.clone()) {
                    next.push(v);
                }
            }
        }
        frontier = next;
    }
    all
}

impl Engine {
    /// Build from "word count" lines (any order). Keeps the `max_words` most frequent words.
    pub fn from_freq_text(text: &str, max_words: usize) -> Self {
        let mut entries: Vec<(String, u64)> = text
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let word = parts.next()?;
                let count: u64 = parts.next()?.parse().ok()?;
                if !word.chars().all(|c| c.is_alphabetic() || c == '\'') {
                    return None;
                }
                Some((word.to_lowercase(), count))
            })
            .collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        entries.truncate(max_words);

        let mut engine = Engine {
            words: Vec::new(),
            freq: Vec::new(),
            ids: HashMap::new(),
            total: 0.0,
            deletes: Vec::new(),
            bigrams: HashMap::new(),
            params: Params::default(),
        };
        for (word, count) in entries {
            if engine.ids.contains_key(&word) {
                continue;
            }
            let id = engine.words.len() as u32;
            engine.total += count as f64;
            engine.ids.insert(word.clone(), id);
            engine.words.push(word);
            engine.freq.push(count);
        }
        engine.total = engine.total.max(1.0);

        let mut deletes = Vec::new();
        for (id, word) in engine.words.iter().enumerate() {
            if word.chars().count() < MIN_INDEXED_LEN {
                continue;
            }
            for variant in delete_variants(word) {
                deletes.push((hash_str(&variant), id as u32));
            }
        }
        deletes.sort_unstable();
        deletes.dedup();
        engine.deletes = deletes;
        engine
    }

    /// Add "word1 word2 count" lines. Pairs with an unknown word are ignored.
    pub fn add_bigrams(&mut self, text: &str) {
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let (Some(a), Some(b), Some(c)) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            let Ok(count) = c.parse::<u64>() else { continue };
            if let (Some(&ia), Some(&ib)) = (
                self.ids.get(&a.to_lowercase()),
                self.ids.get(&b.to_lowercase()),
            ) {
                self.bigrams.insert((ia, ib), count);
            }
        }
    }

    pub fn set_params(&mut self, params: Params) {
        self.params = params;
    }

    pub fn is_known(&self, word: &str) -> bool {
        self.ids.contains_key(&word.to_lowercase())
    }

    fn candidates(&self, lower: &str) -> Vec<u32> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for variant in delete_variants(lower) {
            let h = hash_str(&variant);
            let start = self.deletes.partition_point(|e| e.0 < h);
            for entry in &self.deletes[start..] {
                if entry.0 != h {
                    break;
                }
                if seen.insert(entry.1) {
                    out.push(entry.1);
                }
            }
        }
        out
    }

    fn score(&self, cost: f32, id: u32, ctx: &Context) -> f32 {
        let p = self.freq[id as usize] as f64 / self.total;
        let ln_p = p.ln() as f32;
        let prev_id = ctx.prev.and_then(|w| self.ids.get(&w.to_lowercase()));
        let ctx_term = match prev_id {
            Some(&pid) => match self.bigrams.get(&(pid, id)) {
                Some(&c) => {
                    let denom = self.freq[pid as usize].max(1) as f64;
                    (c as f64 / denom).min(1.0).ln() as f32
                }
                None => BACKOFF.ln() + ln_p,
            },
            None => ln_p,
        };
        -self.params.w_err * cost + self.params.w_uni * ln_p + self.params.lambda * ctx_term
    }

    fn confusion(&self, word: &str, lower: &str, ctx: &Context) -> Decision {
        let Some(prev) = ctx.prev else { return Decision::Leave };
        let (Some(&pid), Some(&wid)) = (self.ids.get(&prev.to_lowercase()), self.ids.get(lower))
        else {
            return Decision::Leave;
        };
        let own = self.bigrams.get(&(pid, wid)).copied().unwrap_or(0).max(1) as f64;
        for set in CONFUSION.iter().filter(|s| s.contains(&lower)) {
            for alt in set.iter().filter(|a| **a != lower) {
                let Some(&aid) = self.ids.get(*alt) else { continue };
                let alt_count = self.bigrams.get(&(pid, aid)).copied().unwrap_or(0);
                if alt_count >= CONFUSION_MIN_COUNT && alt_count as f64 >= own * CONFUSION_RATIO {
                    return Decision::Suggest(apply_case(word, alt));
                }
            }
        }
        Decision::Leave
    }

    /// Ranked candidates as (word, edit cost, score), best first. For tuning and debugging.
    pub fn explain(&self, typo: &str, ctx: &Context) -> Vec<(String, f32, f32)> {
        let lower = typo.to_lowercase();
        let len = lower.chars().count();
        let mut out: Vec<(String, f32, f32)> = self
            .candidates(&lower)
            .into_iter()
            .filter_map(|id| {
                let cand = &self.words[id as usize];
                let cost = weighted_distance(&lower, cand);
                (cand.chars().count().abs_diff(len) <= MAX_EDIT && cost <= self.params.suggest_cost_max)
                    .then(|| (cand.clone(), cost, self.score(cost, id, ctx)))
            })
            .collect();
        out.sort_by(|a, b| b.2.total_cmp(&a.2));
        out
    }

    /// Decide what to do with a finished, unprotected word.
    pub fn decide(&self, word: &str, ctx: &Context, user_known: &dyn Fn(&str) -> bool) -> Decision {
        let lower = word.to_lowercase();
        if user_known(&lower) {
            return Decision::Leave;
        }
        if self.ids.contains_key(&lower) {
            return self.confusion(word, &lower, ctx);
        }
        let len = lower.chars().count();
        if len < 3 {
            return Decision::Leave;
        }

        let mut scored: Vec<(f32, f32, u32)> = Vec::new();
        for id in self.candidates(&lower) {
            let cand = &self.words[id as usize];
            if cand.chars().count().abs_diff(len) > MAX_EDIT {
                continue;
            }
            let cost = weighted_distance(&lower, cand);
            if cost > self.params.suggest_cost_max {
                continue;
            }
            scored.push((self.score(cost, id, ctx), cost, id));
        }
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));

        let Some(&(top_score, top_cost, top_id)) = scored.first() else {
            return Decision::Leave;
        };
        let margin = scored.get(1).map_or(f32::INFINITY, |second| top_score - second.0);
        let replacement = apply_case(word, &self.words[top_id as usize]);

        let p = &self.params;
        // value/values, back/backup: equally cheap edits with a likely rival are guesses.
        let has_rival = scored[1..]
            .iter()
            .any(|s| s.1 <= top_cost + p.rival_cost && top_score - s.0 < p.rival_score);
        if top_cost <= p.auto_cost_max && margin >= p.auto_margin && len >= p.auto_min_len && !has_rival {
            Decision::AutoFix(replacement)
        } else if margin >= p.suggest_margin {
            Decision::Suggest(replacement)
        } else {
            Decision::Leave
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: &str = "the 1000000\nword 20000\nworld 40000\nwould 80000\nward 3000\nwork 90000\n\
receive 5000\nseparate 6000\nbecause 70000\ntomorrow 9000\noccurrence 1500\npeople 90000\n\
there 200000\ntheir 210000\nto 900000\ntoo 100000\nthen 150000\nthan 140000\nmore 800000\nand 900000";
    const BIGRAMS: &str = "more than 5000\nmore then 5\nand then 9000";

    fn engine() -> Engine {
        let mut e = Engine::from_freq_text(SEED, 100);
        e.add_bigrams(BIGRAMS);
        e
    }

    fn none(_: &str) -> bool {
        false
    }

    fn ctx(prev: Option<&str>) -> Context<'_> {
        Context { prev }
    }

    #[test]
    fn unambiguous_typo_is_auto_fixed() {
        assert_eq!(engine().decide("separete", &ctx(None), &none), Decision::AutoFix("separate".into()));
    }

    #[test]
    fn typo_with_a_plausible_rival_is_only_suggested() {
        // "work" is more frequent and only one edit-cost unit further than "word".
        assert_eq!(engine().decide("wprd", &ctx(None), &none), Decision::Suggest("word".into()));
    }

    #[test]
    fn doubled_letter_typo_is_auto_fixed_and_keeps_case() {
        let e = engine();
        assert_eq!(e.decide("tomorow", &ctx(None), &none), Decision::AutoFix("tomorrow".into()));
        assert_eq!(e.decide("Tomorow", &ctx(None), &none), Decision::AutoFix("Tomorrow".into()));
    }

    #[test]
    fn known_words_are_never_touched() {
        let e = engine();
        for w in ["word", "world", "people", "ward"] {
            assert_eq!(e.decide(w, &ctx(None), &none), Decision::Leave, "{w}");
        }
    }

    #[test]
    fn user_known_words_are_never_touched() {
        let e = engine();
        let known = |w: &str| w == "wprd";
        assert_eq!(e.decide("wprd", &ctx(None), &known), Decision::Leave);
    }

    #[test]
    fn three_letter_words_only_get_suggestions() {
        assert_eq!(engine().decide("thw", &ctx(None), &none), Decision::Suggest("the".into()));
    }

    #[test]
    fn nonsense_far_from_every_word_is_left_alone() {
        assert_eq!(engine().decide("xqzvkj", &ctx(None), &none), Decision::Leave);
    }

    #[test]
    fn confusion_pair_is_suggested_only_with_strong_context() {
        let e = engine();
        assert_eq!(e.decide("then", &ctx(Some("more")), &none), Decision::Suggest("than".into()));
        assert_eq!(e.decide("then", &ctx(Some("and")), &none), Decision::Leave);
        assert_eq!(e.decide("then", &ctx(None), &none), Decision::Leave);
    }

    #[test]
    fn delete_index_finds_every_word_within_two_edits() {
        let e = engine();
        let ids = e.candidates("recieve");
        assert!(ids.iter().any(|id| e.words[*id as usize] == "receive"));
    }
}
