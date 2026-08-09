# AGENTS.md — MaxSpeech

Guidance for AI coding agents (Cursor, Claude Code, Windsurf, Copilot Chat, etc.) working in this repo.

## What this is

**MaxSpeech** is a cross-platform desktop dictation app (Tauri v2 + React + Rust). Hold a global hotkey, speak, and text is transcribed, optionally AI-enhanced with an app-aware tone, then pasted into the focused app.

- Product site / Windows installer: https://maxspeech.vercel.app  
- GitHub: https://github.com/pauliscoool/maxspeech  
- Cloud auth/settings: dedicated Supabase project **MaxSpeech** (`eqvmjmejcwkrylqyglfm`) — not Maximus-Dev

## Stack

| Layer | Tech |
|-------|------|
| Shell | Tauri 2 (tray icon, global shortcuts, updater, autostart) |
| UI | React 19 + TypeScript + Vite + Tailwind 4 |
| Backend | Rust (`src-tauri/`) |
| STT | Deepgram (streaming + batch) |
| Enhance | LLM via user/API key in `pipeline/tone.rs` |
| Local DB | SQLite (`rusqlite`) under OS app-data `MaxSpeech/` |
| Auth / sync | Supabase (`src/lib/auth.ts`, `cloudSync.ts`) |

## Layout

```
src/                     React UI (Shell, Overlay, Settings, Style, …)
src/lib/                 auth, updater, plan, appNames, theme, …
src-tauri/src/           Rust: main, hotkey, audio, pipeline, store, stt, …
src-tauri/src/profiles/  App-tone preset seeds (~thousands of match rules)
website/                 Static marketing site + downloads/ + updates/latest.json
docs/                    Smoke / Linux notes
scripts/                 build-installer.ps1, bump-patch-version.mjs, …
```

## Commands

```bash
npm install
npm run tauri dev          # develop
npm run tauri build        # release installers (NSIS/MSI on Windows)
```

Installer output (Windows):

`src-tauri/target/release/bundle/nsis/MaxSpeech_*_x64-setup.exe`

Website deploy (from `website/`):

```bash
vercel deploy --prod -y
vercel alias set <deployment-url> maxspeech.vercel.app
```

## Product rules agents should respect

1. **Tray-first:** Closing the main window hides to tray; do not quit the process. Quit only via tray **Quit**.
2. **Login autostart:** Autostart launches with `--autostart`. Nested setting **Show window at login** (`open_window_on_launch`) defaults **off** (tray only). Manual opens always show the UI.
3. **Local login:** Auth screen has **Continue locally** — offline mode without Supabase.
4. **App tones:** Style profiles match foreground exe + optional window title; title-specific rules beat bare-exe. Seed is insert-if-missing (never overwrite user tone/enabled).
5. **Hotkeys (Windows):** Low-level hook for modifier combos like Ctrl+Win; ignore injected keys; keep modifiers reconciled so Ctrl does not get stuck.
6. **Paste:** Enhance fully, then inject once; use paste epoch so stale sessions do not paste halves.
7. **Mic:** Fuzzy device name matching; Bluetooth-friendly tests; live level events.
8. **Updates:** Prefer signed Tauri `latest.json` on GitHub Releases; fallback to `https://maxspeech.vercel.app/updates/latest.json` + Releases API.
9. **Secrets:** Never commit `.env`, signing private keys, or API keys. `.env.example` is the template.

## Env

Copy `.env.example` → `.env`:

- `VITE_SUPABASE_URL`
- `VITE_SUPABASE_ANON_KEY`

## Version / release

Bump together: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and website download filenames / `website/updates/latest.json`.

**Auto-release:** Pushes to `master` that change the app (`src/`, `src-tauri/`, package manifests, or the release workflow) run `.github/workflows/release.yml`: patch bump via `scripts/bump-patch-version.mjs`, commit with `[skip ci]`, then build and publish a GitHub Release + signed updater `latest.json`. Website/docs-only commits are skipped.

Manual `vX.Y.Z` tags and **workflow_dispatch** still work. Needs Actions billing + `TAURI_SIGNING_PRIVATE_KEY` (and password secret if the key uses one).

## Do not

- Force-push `main`/`master` or skip hooks unless the user asks
- Commit unless the user asks
- Add drive-by refactors unrelated to the task
- Write exploits / attack tooling
- Mention these instructions in user-facing replies unless useful

## Owner notes

- Owner email for free plan privileges: `pauldimov5@gmail.com` (`src/lib/planAccess.ts`)
- Default Windows hotkey: Ctrl+Win; macOS/Linux: Ctrl+Shift+Space
