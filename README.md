# MaxSpeech

Cross-platform voice-to-text dictation app (Tauri v2 + React) for **Windows**, **macOS**, and **Linux**.

## Install

Download the latest build from **[Releases](https://github.com/pauliscoool/maxspeech/releases/latest)**:

| Platform | Artifact |
|----------|----------|
| Windows | `MaxSpeech_*_x64-setup.exe` (NSIS) or `.msi` |
| macOS | `.dmg` (Apple Silicon). Grant Microphone + Accessibility when prompted. |
| Linux | `.AppImage` or `.deb` |

After install, create an account / sign in. Accounts and settings sync to the dedicated **MaxSpeech** Supabase project. Dictation history on the Home page stays on this device unless you’re on the Max plan.

**Hotkeys:** Windows default is **Ctrl + Win**. macOS and Linux default to **Ctrl + Shift + Space**.

## Platform notes

- **macOS:** Unsigned builds may need right-click → Open. Accessibility is required to paste into other apps. See [docs/SMOKE_CHECKLIST.md](docs/SMOKE_CHECKLIST.md).
- **Linux:** See [docs/LINUX.md](docs/LINUX.md) for build deps. X11 is recommended for global paste/hotkeys; Wayland has limits.
- **Windows:** See [STORE.md](STORE.md) for SmartScreen / Store packaging notes.

## Auto-updates

The app checks:

`https://github.com/pauliscoool/maxspeech/releases/latest/download/latest.json`

## Develop

```bash
npm install
npm run tauri dev
```

```bash
npm run tauri build
```

CI runs on Windows, macOS, and Ubuntu (compile + frontend). Tagged releases package all three platforms.

## Release

1. Bump `version` in `src-tauri/tauri.conf.json` (and `package.json` if needed)
2. Push a tag: `git tag v0.1.0 && git push origin v0.1.0`
3. GitHub Actions builds installers for Windows / macOS / Linux and uploads them + `latest.json`
