# macOS notes

## Permissions

MaxSpeech needs:

1. **Microphone** — speech capture (`NSMicrophoneUsageDescription` in `src-tauri/Info.plist`)
2. **Accessibility** — inject dictated text into other apps (enigo / paste)
3. **Automation (System Events)** — optional; used to detect the frontmost app for history labels

Grant these under **System Settings → Privacy & Security**.

## Build

```bash
npm ci
npm run tauri build
```

Produces a `.dmg` under `src-tauri/target/release/bundle/dmg/`.

Entitlements for mic / Apple Events: `src-tauri/Entitlements.plist`.

## Signing / notarization

Release CI builds unsigned Apple Silicon binaries unless you add Apple signing secrets. For distribution outside Gatekeeper exceptions, configure Developer ID signing and notarization separately (not automated in this repo yet).

## Default hotkey

**Ctrl + Shift + Space** (hold to dictate). Modifier-only combos like Ctrl+Win are Windows-only.
