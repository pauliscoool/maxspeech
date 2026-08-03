# Linux build dependencies

Required to compile MaxSpeech on Debian/Ubuntu (CI uses Ubuntu 22.04):

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf \
  libssl-dev \
  libxdo-dev \
  libasound2-dev \
  libdbus-1-dev \
  pkg-config \
  build-essential \
  curl \
  wget \
  file
```

Then:

```bash
npm ci
npm run tauri build
```

Artifacts: AppImage and `.deb` under `src-tauri/target/release/bundle/`.

**Runtime notes**

- Prefer an X11 session for reliable global hotkeys and paste.
- On Wayland, dictation may work but injecting into other apps can fail.
- A Secret Service provider (GNOME Keyring / KWallet) is needed for stored API credentials.
