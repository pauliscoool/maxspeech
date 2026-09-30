# Faithful dictation verification

Implemented on `fix/faithful-dictation`, based on `android-app` at `ddea1fd`.

## Behavior and changed files

- Desktop: `pipeline/tone.rs`, new `pipeline/faithful.rs`, `pipeline/mod.rs`, `pipeline/vocab.rs`, `pipeline/commands.rs`, and `main.rs`. Removed the automatic learner `pipeline/learn_substitutions.rs`. Stored dictionary/substitution rows remain intact; dictation no longer reads substitution rows or infers new dictionary entries from edits.
- Android: `EnhanceClient.kt`, `DictationController.kt`, `DeepgramClient.kt`, new `FaithfulDictation.kt`, `DictationSession.kt`, and `TranscriptAccumulator.kt`; added snippet reads in `AppDatabase.kt` and JVM-test/assets configuration in `build.gradle.kts`.
- Shared: `shared/dictation/prompt.txt` and 58 golden cases in `shared/dictation/golden.json`, consumed by both platforms. Kotlin tests live in `FaithfulDictationTest.kt`; Rust tests live in `tone.rs`, `vocab.rs`, and `commands.rs`.
- Settings descriptions now describe faithful cleanup and latency instead of rewriting depth.

Every enhancement speed uses the existing `gpt-4o-mini` endpoint/key and temperature zero. Speed changes timeout and short-session skipping only. Response budgets depend on input size. Remote output cannot change locally rendered words or formatting: any unexplained word change or different punctuation/layout falls back to the local result. A case-only response also returns the local casing.

Local cleanup retains grammar, slang, contractions, homophones, numbers and product-name guesses. It recognizes conservative corrections, hesitation/stutter patterns, spoken formatting and sequential lists. Quoted/mentioned commands and hesitation words are preserved. Isolated formatting commands retain desktop compatibility and work on Android. Ordinary “make it…” and “change to…” utterances no longer invoke the desktop rewrite path; the explicit “rewrite as…” command and selected-text editing remain separate actions.

Android now calls the previously unused enhance client only for eligible sessions with the existing configured key. The request is cancellable and shares the old 420 ms processing budget. Streaming transcript accumulation prefers final words while retaining an unfinalized tail; session IDs reject old socket messages. Confirmation and automatic insertion can claim a session only once.

These rules mirror observable [Wispr filler removal, backtracking, punctuation and list formatting](https://wisprflow.ai/features). Wispr also documents [cleanup levels that permit clarity/conciseness rewriting](https://docs.wisprflow.ai/articles/4283510616-Auto-Cleanup%3A-control-how-much-Flow-edits-your-dictation); MaxSpeech's word-preservation policy is stricter.

## Commands and exact result output

Ran from `src-tauri/`, exit code **0** for both commands:

```text
cargo test
test result: ok. 54 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.16s

cargo check
warning: `maxspeech` (bin "maxspeech") generated 6 warnings (run `cargo fix --bin "maxspeech" -p maxspeech` to apply 1 suggestion)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.72s
```

The ignored tests are existing live-service probes: `agent_live_rewrite` and `retry_live_from_saved_recording`. Warnings include existing unreachable/unused code and storage methods now unused after retiring automatic learning; they were not changed outside this task.

Ran from `android/`, exit code **0**:

```powershell
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:JAVA_TOOL_OPTIONS = '-Djdk.net.unixdomain.tmpdir=C:\maxspeech-nonexistent-socket-dir'
.\gradlew.bat --no-daemon testDebugUnitTest assembleDebug
```

```text
> Task :app:assembleDebug
> Task :app:testDebugUnitTest
Picked up JAVA_TOOL_OPTIONS: -Djdk.net.unixdomain.tmpdir=C:\maxspeech-nonexistent-socket-dir

BUILD SUCCESSFUL in 54s
45 actionable tasks: 12 executed, 33 up-to-date
```

JUnit XML reports **9 tests, 0 failures, 0 errors, 0 skipped**. These include the shared corpus, strict guard, prompt/model settings, error/truncation fallback, real socket cancellation inside the processing budget, session ownership and STT accumulation.

The initial plain Gradle attempt failed before compilation with:

```text
java.io.IOException: Unable to establish loopback connection
Caused by: java.net.SocketException: Invalid argument: connect
```

The process-only JVM property above forces Java's pipe implementation to fall back from the failing Unix-domain socket path to TCP. No repository or persistent environment setting was added. Android also reports an SDK XML version warning.

Ran `npm run build` from the repository root, exit code **0**:

```text
✓ built in 7.67s
```

Full command output is retained locally:

- [Rust tests](<C:/Users/Paul Dimov/AppData/Local/Temp/maxspeech-faithful-cargo-test-final.log>)
- [Rust check](<C:/Users/Paul Dimov/AppData/Local/Temp/maxspeech-faithful-cargo-check.log>)
- [Android checks](<C:/Users/Paul Dimov/AppData/Local/Temp/maxspeech-faithful-android-tcp-final.log>)
- [Frontend build](<C:/Users/Paul Dimov/AppData/Local/Temp/maxspeech-faithful-frontend.log>)
- [Built Android debug APK](<C:/Users/Paul Dimov/Documents/max speech project/android/app/build/outputs/apk/debug/app-debug.apk>)

## Latency evidence and remaining limits

A temporary optimized Rust harness compared the baseline local self-correction/punctuation code with the new local cleanup on the same 500-word input, 1,000 iterations, while other builds ran:

```text
500-word cleanup, 1000 iterations: baseline_ms=2248 faithful_ms=1474
```

This is a local CPU benchmark, not an end-to-end release-to-paste measurement. Desktop request timeouts, short-session thresholds, streaming and paste-epoch checks remain intact; the HTTP client is reused. Android's synthetic timeout test verifies cancellation inside its processing budget. Timing and fallback logs contain no transcript content or keys.

Controlled live audio/STT/enhancement comparisons and hardware release-to-paste measurements were **not performed**. Available history/logs do not retain paired raw STT and enhanced text, and no recording with a known expected transcript was supplied. Both platforms still use Deepgram smart formatting/numerals; an error already in its transcript cannot be repaired by the drift guard. The 58 text fixtures establish cleanup parity, not equal acoustic recognition across microphones/devices.

Conservative rules intentionally leave ambiguous corrections untouched. Exact output parity makes local rendering authoritative even when remote formatting differs. Existing polluted dictionary entries lack provenance and are retained; no new automatic learning occurs.

No version bump, installer, installation, production deployment, push, key/provider addition, or protected-branch change was performed. Existing untracked `.serena/` and `docs/PROMPT_faithful_dictation.md` were preserved.
