#!/usr/bin/env bash
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq wget file ca-certificates dpkg-dev

cd /tmp
wget -q https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage -O appimagetool.AppImage
chmod +x appimagetool.AppImage
./appimagetool.AppImage --appimage-extract
TOOL=/tmp/squashfs-root/AppRun

APPDIR="/app/src-tauri/target/release/bundle/appimage/MaxSpeech.AppDir"
OUTDIR="/app/src-tauri/target/release/bundle/appimage"
mkdir -p "$OUTDIR"
OUT="$OUTDIR/MaxSpeech_0.1.1_amd64.AppImage"

if [ ! -f "$APPDIR/AppRun" ]; then
  wget -q https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-x86_64 -O "$APPDIR/AppRun"
  chmod +x "$APPDIR/AppRun"
fi

cp -f "$APPDIR/usr/share/applications/MaxSpeech.desktop" "$APPDIR/MaxSpeech.desktop"
ICON="$(find "$APPDIR/usr/share/icons" -name 'maxspeech.png' | head -1)"
cp -f "$ICON" "$APPDIR/maxspeech.png"

# Fix Exec line for AppImage desktop file
sed -i 's|^Exec=.*|Exec=maxspeech|' "$APPDIR/MaxSpeech.desktop"
grep -q '^Icon=' "$APPDIR/MaxSpeech.desktop" || echo 'Icon=maxspeech' >> "$APPDIR/MaxSpeech.desktop"

ARCH=x86_64 "$TOOL" --no-appstream "$APPDIR" "$OUT"
chmod +x "$OUT"
ls -lh "$OUT"

# Build .deb from staged data
DEB_SRC="/app/src-tauri/target/release/bundle/appimage_deb"
WORK=/tmp/maxspeech-deb
rm -rf "$WORK"
mkdir -p "$WORK/DEBIAN"
cp -a "$DEB_SRC/data/." "$WORK/"
SIZE_KB=$(du -sk "$WORK" | awk '{print $1}')
cat > "$WORK/DEBIAN/control" <<EOF
Package: maxspeech
Version: 0.1.1
Section: utils
Priority: optional
Architecture: amd64
Maintainer: MaxSpeech <hello@maxspeech.app>
Installed-Size: $SIZE_KB
Description: AI dictation for the desktop
 Hold a hotkey, speak, and MaxSpeech types into the focused app.
EOF
mkdir -p /app/src-tauri/target/release/bundle/deb
dpkg-deb --build "$WORK" "/app/src-tauri/target/release/bundle/deb/maxspeech_0.1.1_amd64.deb"
ls -lh /app/src-tauri/target/release/bundle/deb/*.deb

echo DONE
