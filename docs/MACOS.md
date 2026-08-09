# macOS notes

## Download

Apple Silicon installer: [maxspeech.vercel.app/mac](https://maxspeech.vercel.app/mac)  
(`website/downloads/MaxSpeech_*_aarch64.dmg`)

## Permissions

MaxSpeech needs:

1. **Microphone** — speech capture (`NSMicrophoneUsageDescription` in `src-tauri/Info.plist`)
2. **Accessibility** — inject dictated text into other apps (enigo / paste)
3. **Automation (System Events)** — optional; used to detect the frontmost app for history labels

Grant these under **System Settings → Privacy & Security**.

## Build (local Mac)

```bash
npm ci
./scripts/build-mac.sh
```

Or:

```bash
npm run tauri build
```

Produces a `.dmg` under `src-tauri/target/**/release/bundle/dmg/`. The helper script copies it to `website/downloads/MaxSpeech_<version>_aarch64.dmg`.

Entitlements for mic / Apple Events: `src-tauri/Entitlements.plist`.

## Signing / notarization

Release CI and local scripts produce **unsigned** Apple Silicon binaries unless you add Apple signing secrets. First launch may need right-click → **Open**. Developer ID signing and notarization are not automated in this repo yet (follow-up).

## Default hotkey

**Ctrl + Shift + Space** (hold to dictate). Modifier-only combos like Ctrl+Win are Windows-only.
