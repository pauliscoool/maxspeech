//! Evaluation harness: end-to-end golden cases plus a synthetic-typo precision/recall report.
//! Run `cargo test --release --bin maxspeech autocorrect::eval -- --nocapture` to see the numbers.
use std::collections::HashSet;
use std::sync::OnceLock;

use super::engine::{Context, Decision, Engine};
use super::tracker::{Event, Tracker};
use super::{bundled_engine, evaluate};

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(bundled_engine)
}

/// Type `typed` on a fresh line and apply only the automatic fixes, like the live hook would.
fn simulate(engine: &Engine, typed: &str) -> String {
    let mut tracker = Tracker::new();
    tracker.feed(Event::Enter);
    let mut out: Vec<char> = Vec::new();
    for c in typed.chars() {
        let boundary = tracker.feed(Event::Char(c));
        let Some(b) = boundary else {
            out.push(c);
            continue;
        };
        let keep = out.len() - (b.chars_to_delete() - 1);
        out.truncate(keep);
        match evaluate(engine, &b, &|_| false) {
            Some(fix) if fix.auto => {
                out.extend(b.leading.chars());
                out.extend(fix.replacement.chars());
            }
            _ => {
                out.extend(b.leading.chars());
                out.extend(b.core.chars());
            }
        }
        out.extend(b.trailing.chars());
        out.push(c);
    }
    out.into_iter().collect()
}

#[derive(serde::Deserialize)]
struct Case {
    typed: String,
    expected: String,
}

#[test]
fn golden_cases_match_exactly() {
    let raw = include_str!("../../../shared/dictation/autocorrect_golden.json");
    let cases: Vec<Case> = serde_json::from_str(raw).expect("golden json");
    let e = engine();
    let mut failures = Vec::new();
    for case in &cases {
        let got = simulate(e, &case.typed);
        if got != case.expected {
            failures.push(format!("typed   {:?}\nexpected {:?}\ngot      {:?}", case.typed, case.expected, got));
        }
    }
    assert!(failures.is_empty(), "{} golden failures:\n{}", failures.len(), failures.join("\n\n"));
}

/// Timing is meaningless while other tests hog the CPU: run alone with
/// `cargo test --release --bin maxspeech latency_report -- --ignored --nocapture`.
#[test]
#[ignore = "benchmark: run alone"]
fn latency_report() {
    let build = std::time::Instant::now();
    let e = bundled_engine();
    let build_ms = build.elapsed().as_millis();
    let words = ["recieve", "wprd", "tomorow", "becuse", "thier", "definately", "xqzvkj", "helllo", "adress", "meeeting"];
    let ctx = Context { prev: Some("the") };
    let mut times: Vec<u128> = Vec::new();
    for _ in 0..200 {
        for w in words {
            let t = std::time::Instant::now();
            let _ = e.decide(w, &ctx, &|_| false);
            times.push(t.elapsed().as_micros());
        }
    }
    times.sort_unstable();
    let p = |q: f64| times[((times.len() - 1) as f64 * q) as usize];
    println!("index build {build_ms} ms; decide p50 {} us, p95 {} us, max {} us", p(0.5), p(0.95), times[times.len() - 1]);
    assert!(p(0.95) < 5_000, "p95 decision latency above 5 ms");
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn neighbors(c: char) -> &'static str {
    match c {
        'q' => "wa", 'w' => "qes", 'e' => "wrd", 'r' => "etf", 't' => "ryg", 'y' => "tuh",
        'u' => "yij", 'i' => "uok", 'o' => "ipl", 'p' => "o", 'a' => "qsz", 's' => "awdx",
        'd' => "sefc", 'f' => "drgv", 'g' => "fthb", 'h' => "gyjn", 'j' => "hukm",
        'k' => "jil", 'l' => "ko", 'z' => "ax", 'x' => "zsc", 'c' => "xdv", 'v' => "cfb",
        'b' => "vgn", 'n' => "bhm", 'm' => "nj", _ => "",
    }
}

/// One realistic keyboard slip: adjacent key, swapped pair, dropped letter, or doubled letter.
fn make_typo(word: &str, rng: &mut Lcg) -> Option<String> {
    let chars: Vec<char> = word.chars().collect();
    let n = chars.len();
    let pos = 1 + rng.below(n - 1);
    let mut t = chars.clone();
    match rng.below(4) {
        0 => {
            let near = neighbors(chars[pos]);
            if near.is_empty() {
                return None;
            }
            let nc: Vec<char> = near.chars().collect();
            t[pos] = nc[rng.below(nc.len())];
        }
        1 if pos + 1 < n => t.swap(pos, pos + 1),
        2 => {
            t.remove(pos);
        }
        3 => t.insert(pos, chars[pos]),
        _ => return None,
    }
    let typo: String = t.into_iter().collect();
    (typo != word).then_some(typo)
}

#[test]
fn synthetic_typo_report() {
    let e = engine();
    let words: Vec<String> = include_str!("../../data/autocorrect/freq_en.txt")
        .lines()
        .take(20_000)
        .filter_map(|l| l.split_whitespace().next())
        .filter(|w| w.chars().count() >= 5 && w.chars().all(|c| c.is_ascii_lowercase()))
        .map(str::to_string)
        .collect();
    let mut rng = Lcg(42);
    let (mut total, mut auto_ok, mut auto_bad, mut suggest_ok, mut suggest_bad, mut left) = (0, 0, 0, 0, 0, 0);
    let mut bad_examples: Vec<String> = Vec::new();
    let ctx = Context { prev: None };
    let none = |_: &str| false;
    let mut seen = HashSet::new();
    for word in words.iter().cycle().take(words.len() * 2) {
        let Some(typo) = make_typo(word, &mut rng) else { continue };
        if e.is_known(&typo) || !seen.insert(typo.clone()) {
            continue;
        }
        total += 1;
        match e.decide(&typo, &ctx, &none) {
            Decision::AutoFix(r) if &r == word => auto_ok += 1,
            Decision::AutoFix(r) => {
                auto_bad += 1;
                if bad_examples.len() < 15 {
                    bad_examples.push(format!("{typo} -> {r} (wanted {word})"));
                }
            }
            Decision::Suggest(r) if &r == word => suggest_ok += 1,
            Decision::Suggest(_) => suggest_bad += 1,
            Decision::Leave => left += 1,
        }
    }
    let precision = auto_ok as f64 / (auto_ok + auto_bad).max(1) as f64;
    let helped = (auto_ok + suggest_ok) as f64 / total as f64;
    println!("typos tested        {total}");
    println!("auto-fixed correct  {auto_ok}");
    println!("auto-fixed WRONG    {auto_bad}");
    println!("suggested correct   {suggest_ok}");
    println!("suggested wrong     {suggest_bad}");
    println!("left alone          {left}");
    println!("auto precision      {:.2}%", precision * 100.0);
    println!("fixed or suggested  {:.1}%", helped * 100.0);
    for ex in &bad_examples {
        println!("  wrong: {ex}");
    }
    assert!(total > 1000, "too few typos generated");
    assert!(precision >= 0.97, "auto-fix precision {:.2}% is below the floor", precision * 100.0);
}
