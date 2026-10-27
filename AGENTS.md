# AGENTS.md — MaxSpeech

Guidance for AI coding agents (Claude Code, Cursor, Windsurf, Copilot Chat, etc.) working in this repo. Read it fully before editing.

## What this is

**MaxSpeech** is a cross-platform dictation app: a desktop app (Tauri v2 + React + Rust) and a native Android app (Kotlin + Compose). Hold a hotkey (or the floating mic on Android), speak, and text is transcribed, cleaned up, then pasted into the focused app.

- Product site / Windows installer: https://maxspeech.vercel.app
- GitHub: https://github.com/pauliscoool/maxspeech
- Cloud auth/settings: dedicated Supabase project **MaxSpeech** (`eqvmjmejcwkrylqyglfm`) — not Maximus-Dev

## How to work (mandatory workflow)

1. **Always keep a to-do list.** For any request that edits files or runs more than one command, write the full task list first (as many items as the work needs — 20+ is fine), keep exactly one item in progress, and tick items off as they finish. Update the list when the plan changes. Never work from memory of "what's left".
2. **Investigate before building.** Read the code and the git history that touches the area. Check `git branch -a`, `git log --all -- <path>` and GitHub before assuming the code in the working tree is the newest version (see "Android source of truth").
3. **Reproduce, then fix the root cause.** Use `adb logcat -b crash` on Android and the app logs on desktop. Fix where all callers route through, not at one call site.
4. **Verify on the real target.** Build → install → look at it. For Android that means a screenshot from the phone (`adb exec-out screencap -p`), not "it compiled". For desktop, confirm the installed binary is the new one.
5. **Run commands yourself.** If a command can be run from here, run it; do not hand the user shell steps.
6. **Report honestly.** Say what was built, what was verified, what was not. Never claim a device check you did not do.
7. **Git hygiene.** Commit only when asked or when the batch is done per "Ship" below; never force-push, never skip hooks, never commit `.env`, keystores or API keys. Do not touch uncommitted work you did not make; use a separate `git worktree` for side investigations.
8. **Keep this file accurate.** When behaviour, branches, versions or rules change, update this file in the same batch.

## Stack

| Layer | Tech |
|-------|------|
| Desktop shell | Tauri 2 (tray icon, global shortcuts, updater, autostart) |
| Desktop UI | React 19 + TypeScript + Vite + Tailwind 4 |
| Desktop backend | Rust (`src-tauri/`) |
| Android | Kotlin + Jetpack Compose (`android/`), accessibility service + floating mic overlay |
| STT | Deepgram Nova-3 (streaming + batch) |
| Cleanup | Deepgram voice agent with a `deliver_text` function call (prompt in `shared/dictation/agent_cleanup.txt`); local cleanup as fallback |
| Polish / rewrite (desktop) | OpenAI/OpenRouter `gpt-6-luna` (`pipeline/openai_llm.rs`) |
| Local DB | SQLite (desktop `rusqlite`), Room (Android) |
| Auth / sync | Supabase (`src/lib/auth.ts`, `cloudSync.ts`, Android `AuthRepository`/`CloudSync`) |

## Layout

```
src/                     React UI (Shell, Overlay, Settings, Style, …)
src/lib/                 auth, updater, plan, appNames, theme, …
src-tauri/src/           Rust: main, hotkey, audio, pipeline, store, stt, …
src-tauri/src/profiles/  App-tone preset seeds
android/                 Android app (see "Android source of truth")
shared/dictation/        Prompts, rules, keyterms shared by desktop AND Android (Android bundles it as assets)
website/                 Static marketing site + downloads/ + updates/latest.json
docs/                    Smoke / Linux notes
scripts/                 build-installer.ps1, bump-patch-version.mjs, …
```

## Commands

```bash
npm install
npm run tauri dev          # develop desktop
npm run tauri build        # release installers (NSIS/MSI on Windows)
```

Installer output (Windows): `src-tauri/target/release/bundle/nsis/MaxSpeech_*_x64-setup.exe`

Desktop builds are slow (web UI + full Rust release + NSIS ≈ 3 min). Batch your changes before building.

Android (from `android/`; on Windows use PowerShell, `--no-daemon` avoids a loopback error seen with the Bash tool):

```powershell
.\gradlew.bat assembleRelease -q --no-daemon
adb install -r app\build\outputs\apk\release\app-release.apk
```

Website deploy (from `website/`):

```bash
vercel deploy --prod -y
vercel alias set <deployment-url> maxspeech.vercel.app
```

## Android source of truth (READ THIS)

The **newest Android app** (0.5-beta lineage: new theme, plan screen, mic-look settings, keyboard dock bar, floating mic controller, Home/Dictionary/Snippets/Style/Settings tabs, cloud sign-in) lives on the GitHub branch **`fix/android-dictation-button`**. Local worktree: `../max-speech-android-newest` on branch `android-newest` (tracks it).

The `android/` folder on `fix/faithful-dictation`, `windows` and `master` is an **older lineage** (old theme, fewer screens). Do **not** build or install Android from those branches. Before any Android work, confirm `git merge-base --is-ancestor origin/fix/android-dictation-button HEAD`; if it fails, work in the `android-newest` worktree or merge that branch first.

Android facts:

- Package `com.maxspeech.android`, debug-signed (updates install over each other). Installing a lower `versionCode` needs an uninstall first, so always bump `versionCode`.
- The phone may also have a Samsung **Secure Folder** (user 150) that `adb` cannot see; an old copy there must be removed by the user.
- Samsung marks the accessibility service as *crashed* after an install/crash and will not rebind it. Fix: toggle it off then on via `settings put secure enabled_accessibility_services`, then check `dumpsys accessibility | grep Crashed`.
- The floating mic needs: accessibility service on, "Display over other apps" allowed, mic permission, and a signed-in cloud account.
- Compose in an overlay window needs the ViewTree lifecycle/savedstate/viewmodel owners on the **root view** added to the `WindowManager`, not only on the `ComposeView`; otherwise the process crashes when the overlay attaches.
- Check crashes with `adb logcat -b crash`, and read `dumpsys activity services com.maxspeech.android` to see whether `OverlayService` and the accessibility service are live.

## Transcription rules (desktop AND Android — keep them identical)

1. **Deepgram Nova-3 with `numerals=false`.** Spoken numbers stay words ("one", "two", "seven"). Never turn on `numerals=true`.
2. **Faithful dictation.** Fix ASR mistakes only when the intended word is clear; keep the speaker's own words, slang and voice; remove fillers, stutters and apply spoken self-corrections; never add, summarize, translate, or answer the text. Never emit profanity from a clean near-homophone.
3. **Casual number style:** zero–twenty as words; digits for times, codes, versions, prices and numbers above twenty (prompt rule 7).
4. **Prompts are shared.** Edit `shared/dictation/*` once; both platforms consume it. Keep `agent_cleanup.txt` and the local rules in `LocalCleanup` consistent.
5. **No auto-learning junk.** Never auto-add everyday words to the dictionary or learn "heard → meant" pairs on your own. Learned pairs are passed as empty. Dictionary entries are user-added only.
6. **Paste once.** Enhance (agent cleanup, prewarmed while the user speaks) and inject a single time. Do not paste a quick draft and then replace it — it caused erase/rewrite flicker and double-pasted text when the user sent a message right after dictating. Desktop: fall back to local cleanup if the agent is slow, then paste once.
7. **Keep history/Supabase off the paste path.** Save history and usage after pasting.
8. **Latency target:** feel faster than Wispr Flow — streaming STT, prewarmed agent, short hotkey-release trail, minimal work between release and paste.

## Sound and haptics (Android)

- The start/stop click (`SoundCue`, `R.raw.bubble_click`) defaults to **10% volume** (`soundVolume = 0.1f` in `SettingsRepository`, both the data class default and the stored-value fallback). Never raise the default.
- **No sound when the app opens.** `DictationController.cancel()` is called by the UI on launch/sign-out; it must stay silent (no click, no haptic) unless a dictation session was active. Only start, stop and a real cancel may click.
- Play the stop click only after the mic is closed so it never lands in the recording.

## Product rules agents should respect

1. **Tray-first (desktop):** Closing the main window hides to tray; do not quit the process. Quit only via tray **Quit**.
2. **Login autostart:** Autostart launches with `--autostart`. Setting **Show window at login** (`open_window_on_launch`) defaults **off**. Manual opens always show the UI.
3. **Local login:** Desktop auth screen has **Continue locally** — offline mode without Supabase. The newest Android requires cloud sign-in.
4. **App tones:** Style profiles match foreground exe + optional window title; title-specific rules beat bare-exe. Seed is insert-if-missing (never overwrite user tone/enabled).
5. **Hotkeys (Windows):** Low-level hook for modifier combos like Ctrl+Win; ignore injected keys; keep modifiers reconciled so Ctrl does not get stuck.
6. **Mic:** Fuzzy device name matching; Bluetooth-friendly tests; live level events.
7. **Updates:** Prefer signed Tauri `latest.json` on GitHub Releases; fallback to `https://maxspeech.vercel.app/updates/latest.json` + Releases API. The website download must point at the real current EXE (no stale GitHub redirect).
8. **Secrets:** Never commit `.env`, signing private keys, or API keys. `.env.example` is the template; add any new env key there.
9. **No user API keys in product UI:** MaxSpeech is a paid SaaS. Do **not** expose Deepgram/LLM key fields in Settings on Android or desktop. Speech/enhance use **Maximus Dev company keys** only. Free tier shows a persistent **Upgrade now** banner at the top of every main page.
10. **Free plan:** 2 minutes of recording every rolling 24 hours. Paid tiers keep weekly word caps.
11. **About / privacy copy:** Say we use Maximus Dev company API keys for speech models, and that user content is **not** used for training.
12. **UI:** Dark, zinc-900 base with cyan/blue accents; touch-friendly (apps are used on a phone in the field); flat, no heavy chrome.

## Ship when a batch is done

After app changes are complete and no further sub-work is in flight:

1. Bump the version everywhere (see "Version / release") — Android also bumps `versionCode`.
2. Desktop: build the installer (`scripts/build-installer.ps1`), silent-install on this machine, confirm the app launches and the binary is the new one.
3. Android: build the release APK **from the newest-lineage branch**, `adb install -r`, re-check the accessibility service is not "Crashed", grant mic + overlay, take a screenshot, confirm the floating mic shows over the keyboard.
4. Commit and push (never giant EXEs; never force-push).
5. **Always deploy `website/` to Vercel production** (`vercel deploy --prod -y`) and alias `maxspeech.vercel.app` so `website/updates/latest.json`, the installer and the APK download are live. Preview-only deploys are not enough, even for "app-only" changes.

## Env

Copy `.env.example` → `.env`: `VITE_SUPABASE_URL`, `VITE_SUPABASE_ANON_KEY`. Android reads company keys from `Secrets.kt` / `local.properties` (machine-local, never committed).

## Version / release

Bump together: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and website download filenames / `website/updates/latest.json`.

### Android beta versioning (current track)

We ship **Android beta** builds (sideload / website APK), not the Play Store yet.

| Track | Scheme | Notes |
|-------|--------|-------|
| **Beta (now)** | `0.x.y-beta` up to **`5.0-beta`** | `versionName` includes `-beta`. Bump `versionCode` on every ship (currently 82 / 0.5.5-beta). |
| **Full / store (later)** | Restart at **`0.1`** | After the last **5.0 beta**, the first public store release is **0.1** (no `-beta`). |

**Agent reminder:** When Android `versionName` reaches **`5.0-beta`** (or you are about to ship past it), **stop and remind Paul** to plan the store release starting at **0.1**.

`website/android.html` and `website/downloads/MaxSpeech.apk` must match the shipped beta `versionName` on every Android ship + Vercel prod deploy.

**Auto-release:** Pushes to `master` that change the app (`src/`, `src-tauri/`, package manifests, or the release workflow) run `.github/workflows/release.yml`: patch bump via `scripts/bump-patch-version.mjs`, commit with `[skip ci]`, then build and publish a GitHub Release + signed updater `latest.json`. Manual `vX.Y.Z` tags and **workflow_dispatch** still work. Needs Actions billing + `TAURI_SIGNING_PRIVATE_KEY`.

## Do not

- Force-push `main`/`master` or skip hooks
- Install an older Android build over a newer one, or build Android from the old lineage
- Add drive-by refactors unrelated to the task
- Write exploits / attack tooling
- Enable `numerals=true`, auto-learning, or paste-then-replace
- Play a sound or haptic when the app merely opens

## Owner notes

- Owner email for free plan privileges: `pauldimov5@gmail.com` (`src/lib/planAccess.ts`)
- Default Windows hotkey: Ctrl+Win; macOS/Linux: Ctrl+Shift+Space
