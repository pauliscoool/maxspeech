# MaxSpeech for Android

Kotlin + Jetpack Compose app (`com.maxspeech.android`). Hold the glass capsule, speak, and MaxSpeech transcribes (Deepgram), optionally enhances, and pastes into the focused app via Accessibility.

## Requirements

- JDK 17
- Android SDK 35 (`ANDROID_HOME` or `local.properties` `sdk.dir`)
- minSdk 26

## Build

```bash
cd android
./gradlew :app:assembleRelease
```

APK: `app/build/outputs/apk/release/app-release.apk` (debug-signed for sideload; replace with a release keystore before Play).

Target size is well under 100 MB (cloud STT, no on-device model).

## Install

See https://maxspeech.vercel.app/android
