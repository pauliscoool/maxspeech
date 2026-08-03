# Smoke / CI report — Linux & macOS port

Date: 2026-08-03  
Branch: [`port/linux-macos`](https://github.com/pauliscoool/maxspeech/tree/port/linux-macos)  
PR: https://github.com/pauliscoool/maxspeech/pull/1

## Local verification (Windows)

| Check | Result |
|-------|--------|
| `cargo check` (src-tauri) | Pass |
| `npx tsc --noEmit` | Pass |

## GitHub Actions

Workflows added:

- [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) — windows / macos / linux jobs (`cargo check` + frontend build)
- [`.github/workflows/release.yml`](../.github/workflows/release.yml) — multi-OS release packaging

**CI status:** every run ends in `startup_failure` with **zero jobs created** (including pushes to `master`). This affects both the new CI workflow and historical tag/release runs on this private repo. That points to an **account / Actions entitlement** problem (billing/minutes/policy), not a MaxSpeech compile error.

Until Actions runners start successfully:

1. Open https://github.com/settings/billing and confirm Actions minutes for private repos.
2. Confirm Actions is enabled under repo **Settings → Actions → General**.
3. Re-run: `gh workflow run CI --ref port/linux-macos`

## Manual smoke (needs Mac / Linux hardware)

Follow [SMOKE_CHECKLIST.md](SMOKE_CHECKLIST.md).

### Known platform limits

- **Wayland:** global hotkey / paste / foreground-app detection may fail; prefer X11.
- **macOS:** Microphone + Accessibility required; unsigned builds need Gatekeeper override. Notarization not automated.
- **Hotkeys:** Windows keeps `Ctrl+Win`; macOS/Linux default to `Ctrl+Shift+Space`. Modifier-only combos stay Windows-only.

## Port surface covered in code

- Clipboard via `arboard`; paste Meta+V (macOS) / Ctrl+V (elsewhere)
- Keyring: apple-native + windows-native + sync-secret-service
- Data dir via `dirs` (Application Support / XDG / LocalAppData)
- Foreground app: Win32 / osascript / xprop (Wayland → None)
- Bundles: nsis, msi, dmg, appimage, deb + Info.plist / Entitlements
