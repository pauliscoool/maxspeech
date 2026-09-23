# MaxSpeech for Android

Kotlin + Jetpack Compose app (`com.maxspeech.android`). Hold the glass capsule, speak, and MaxSpeech transcribes (Deepgram), optionally enhances, and pastes into the focused app via Accessibility.

## Requirements

- JDK 17
- Android SDK 35 (`ANDROID_HOME` or `local.properties` `sdk.dir`)
- minSdk 26
- Device with USB debugging authorized (`adb devices` shows `device`)

## Build → install → log check (agent loop)

From repo root (PowerShell):

```powershell
# Build release, install, launch, scan logcat for crashes / overlay failures
.\scripts\android-ship.ps1

# Debug APK instead
.\scripts\android-ship.ps1 -Debug

# Re-install last APK only, still scan logs
.\scripts\android-ship.ps1 -SkipBuild

# Live filtered log stream (keep open while testing on device)
.\scripts\android-watch-logs.ps1 -Clear
```

`android-ship.ps1` writes `android-last-logcat.txt` and exits **1** if it sees a FATAL / floating-mic attach failure or the process died — so you can fix and re-run.

## Build only

```bash
cd android
./gradlew :app:assembleRelease
```

APK: `app/build/outputs/apk/release/app-release.apk` (debug-signed for sideload; replace with a release keystore before Play).

Target size is well under 100 MB (cloud STT, no on-device model).

## Install

See https://maxspeech.vercel.app/android
