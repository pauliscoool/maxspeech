# MaxSpeech

Windows voice-to-text dictation app (Tauri v2 + React).

## Install

Download the latest Windows installer from **[Releases](https://github.com/pauliscoool/maxspeech/releases/latest)**:

- `MaxSpeech_*_x64-setup.exe` — recommended (NSIS)
- `MaxSpeech_*_x64_en-US.msi` — MSI alternative

After install, create an account / sign in. Accounts and settings sync to the dedicated **MaxSpeech** Supabase project (separate from other Maximus apps). Dictation history on the Home page stays on your PC unless you’re on the Max plan.

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

## Release

1. Bump `version` in `src-tauri/tauri.conf.json` (and `package.json` if needed)
2. Push a tag: `git tag v0.1.0 && git push origin v0.1.0`
3. GitHub Actions builds Windows installers and uploads them + `latest.json`
