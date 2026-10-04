# Task: make MaxSpeech dictation faithful, like Wispr Flow (Windows + Android)

Repo: MaxSpeech (Tauri v2 + Rust + React desktop, Kotlin Android). Read `AGENTS.md` first.

## Git setup
1. `git checkout android-app && git pull` (it already contains the merged `windows` work).
2. `git checkout -b fix/faithful-dictation`
3. Commit in small logical steps. Do NOT push, force-push, or touch `master`/`windows`. Never commit `.env` or keys.

## The bug
Dictated text comes out with different wording and phrasing than what I said. The AI enhance pass rewrites my sentences instead of just cleaning them. I want it to behave like Wispr Flow: **my words stay my words**. Wispr Flow only removes filler, fixes punctuation and capitalization, resolves clear self-corrections, and formats obvious lists or numbers. It never paraphrases, "improves clarity", tightens, reorders, or swaps synonyms.

## Root causes I found (verify them yourself)
Desktop, `src-tauri/src/pipeline/tone.rs`:
- `GRAMMAR_RULES` says "Fix ... awkward phrasing", "Improve clarity lightly - tighten run-ons", "Grammarly-style cleanup". That invites rewriting.
- `EnhanceSpeed` (`Fast`/`Thinking`/`Ultra`): `Ultra` uses `gpt-4o` at temperature 0.22 and `Thinking` uses 0.1. Any temperature above 0 adds paraphrase drift.
- The per-app tone presets (casual/formal/prose/code) tell the model to "Rewrite in ...". Rewriting is the wrong mode for dictation.
- `ASR_CORRECTION_RULES` is very aggressive (number spelling, percent guesses, homophone swaps, product-name guesses). Each rule can change what I actually said.
- Pipeline entry: `pipeline/mod.rs` around lines 660-770 (`local_self_correct`, `enhance_dictation_ex`, `quick_skip_secs`, `normalize_terminal_punctuation`).
- Also review `pipeline/agent_llm.rs`, `recovery.rs`, `vocab.rs`, `learn_substitutions.rs`. Auto-learned substitutions already polluted the dictionary once (everyday words like then to Than), so make sure they cannot change normal words.

Android:
- `android/app/src/main/java/com/maxspeech/android/pipeline/EnhanceClient.kt`: prompts say "Rewrite in a casual/formal style", "Rewrite as clean prose", "Grammarly-like". Same problem, plus `Ultra` uses `gpt-4o` at 0.22.
- `DictationController.kt` calls it.

## What to build
1. **Faithful-by-default enhancement.** Rewrite the system prompts on BOTH platforms so the model's job is minimal editing:
   - Keep every content word, the order, and the phrasing exactly as spoken.
   - Allowed: remove filler (um, uh, you know, filler "like"), remove stutters and repeated words, fix punctuation/capitalization, apply explicit spoken self-corrections ("no wait, Friday"), apply spoken formatting commands ("new line", "period").
   - Forbidden: synonyms, paraphrase, reordering, tightening, summarizing, adding or removing meaning, changing tone/formality, translating, adding greetings or sign-offs.
   - Tone presets may change ONLY surface formatting (capitalization, trailing period, paragraphing), never wording.
   - Keep the existing dictionary / custom-vocabulary block.
2. **Determinism.** Temperature 0 on all speeds. Use one model for all speeds (a small fast one, e.g. `gpt-4o-mini` or equivalent already in use), so speed setting only affects latency/timeouts, not rewriting depth. Keep `Ultra` as a name if the UI needs it, but it must not rewrite more.
3. **Safety net against drift.** After the LLM returns, compare it to the local pre-cleaned transcript. If word-level similarity is below a threshold (for example more than ~15-20% of content words changed, ignoring filler, punctuation, and case), discard the LLM output and use the locally cleaned text. Add this guard in Rust and Kotlin. Also fall back to local text on timeout or error, as now.
4. **Prefer local, deterministic cleanup.** Where possible (filler removal, punctuation, capitalization, self-correction, numbers), do it in code, not the LLM. Trim `ASR_CORRECTION_RULES` to only near-certain fixes plus the user's dictionary. Remove guessy rules (percent-from-"times", forced spelling-out of numbers, product-name inference) or make them conservative.
5. **Speed.** This must not add latency. Keep streaming STT and paste-once behavior. Short utterances should skip the LLM (existing `quick_skip_secs`) and use the local path. Do not regress the release-to-paste latency.
6. **Android parity.** Port the same faithful prompt, temperature, single model, drift guard, and local cleanup to Kotlin. Keep behavior identical to desktop for the same input.

## Research (do this before coding)
Look at how Wispr Flow describes its behavior (auto-edits: filler removal, punctuation, self-correction, list formatting, "Flow" style settings) and mirror only the observable behavior. Do not invent features. Note your sources in the PR description.

## Tests and verification
- Rust unit tests in `tone.rs`: golden cases where input words must be preserved (e.g. "so I was thinking we could maybe push it to Git tomorrow" keeps every word except filler), plus self-correction cases, plus a drift-guard test where a paraphrased LLM output is rejected.
- Kotlin unit tests for the same drift guard and prompt content.
- `cargo test` and `cargo check` in `src-tauri/`. Android `./gradlew assembleDebug` (and unit tests if configured). Report exact results; do not claim success without running them.

## Constraints
- No TypeScript `any`. Comments explain WHY only. Match existing style. Targeted diffs, no unrelated refactors.
- Do not commit secrets. Update `.env.example` only if you add env keys.
- Follow AGENTS.md ship steps only if I ask; for now stop after committing on `fix/faithful-dictation` and summarize: files changed, test output, and any risk.
