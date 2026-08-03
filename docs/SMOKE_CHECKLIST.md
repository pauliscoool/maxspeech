# MaxSpeech — Mac & Linux smoke checklist

Use after installing a CI or release build. Default hotkey on Mac/Linux is **Ctrl + Shift + Space** (hold to dictate, release to insert). Windows keeps **Ctrl + Win**.

## Shared

- [ ] App launches and shows the shell (Home / Settings)
- [ ] Sign in / create account works
- [ ] Tray icon appears; quit from tray works
- [ ] Overlay bar appears while dictating
- [ ] Mic list populates; Test mic succeeds
- [ ] Hold hotkey → speak → release inserts text into another app
- [ ] Remake works for a recent dictation that still has a recording
- [ ] History shows newest 10 and loads more on scroll
- [ ] Autostart toggle persists across restart

## macOS

- [ ] Grant **Microphone** when prompted (System Settings → Privacy & Security)
- [ ] Grant **Accessibility** so paste/typing into other apps works
- [ ] Frontmost app name shows in history (may need Automation for System Events)
- [ ] Paste uses Cmd+V path (text lands in TextEdit / Notes)
- [ ] DMG opens and app runs (unsigned builds may need right-click → Open)

## Linux

- [ ] AppImage runs (`chmod +x` then execute) or `.deb` installs cleanly
- [ ] Under **X11**: global hotkey + paste into gedit/Kate works
- [ ] Under **Wayland**: note limits — global inject/hotkey/foreground app may fail; document what works
- [ ] ALSA/PipeWire mic enumeration works
- [ ] Secret Service (GNOME Keyring / KWallet) available for saved credentials

## Known limits

- Modifier-only hotkeys (Ctrl+Win) are Windows-only.
- Wayland often blocks global key injection without compositor portals.
- macOS notarization/signing is not automated yet; Gatekeeper may warn on unsigned builds.
