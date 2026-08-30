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
  file \
  fuse \
  libfuse2
```

## Release packages (recommended)

From a machine with Docker (Windows + Docker Desktop is fine):

```bash
bash scripts/build-linux.sh
```

This builds AppImage + `.deb` for the current version, copies them to
`website/downloads/` as:

- `MaxSpeech_amd64.AppImage` / `MaxSpeech_<ver>_amd64.AppImage`
- `maxspeech_amd64.deb` / `maxspeech_<ver>_amd64.deb`

Then deploy `website/` to Vercel.

Native build (WSL/Ubuntu):

```bash
npm ci
npm run tauri build
```

Artifacts: AppImage and `.deb` under `src-tauri/target/release/bundle/`.

**Runtime notes**

- Prefer an X11 session for reliable global hotkeys and paste.
- On Wayland, dictation may work but injecting into other apps can fail.
- A Secret Service provider (GNOME Keyring / KWallet) is needed for stored API credentials.
- If AppImage FUSE is unavailable: `./MaxSpeech_amd64.AppImage --appimage-extract-and-run`
- `.deb` installs pull WebKit/GTK via `apt-get install -f` when depends are missing.
