#!/usr/bin/env bash
# Runs inside Docker (Ubuntu 22.04). Invoked by scripts/build-linux.sh.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
export APPIMAGE_EXTRACT_AND_RUN=1
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"

echo "== apt =="
apt-get update -qq
apt-get install -y -qq \
  curl wget file build-essential pkg-config ca-certificates \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf libssl-dev libxdo-dev libasound2-dev libdbus-1-dev \
  xdg-utils fuse libfuse2 python3

# Node 22
if ! command -v node >/dev/null 2>&1 || [[ "$(node -v | cut -d. -f1 | tr -d v)" -lt 20 ]]; then
  echo "== node =="
  curl -fsSL https://deb.nodesource.com/setup_22.x | bash -
  apt-get install -y -qq nodejs
fi
echo "node $(node -v) npm $(npm -v)"

# Rust
if ! command -v cargo >/dev/null 2>&1; then
  echo "== rustup =="
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
fi
# shellcheck disable=SC1091
source "${CARGO_HOME:-$HOME/.cargo}/env"
rustc --version
cargo --version

if ! command -v cargo-tauri >/dev/null 2>&1; then
  echo "== cargo-tauri =="
  cargo install tauri-cli --version "^2" --locked
fi

cd /app
VER="$(node -p "require('./src-tauri/tauri.conf.json').version")"
echo "== building MaxSpeech $VER =="

npm ci
# Bundles often succeed even when updater signing fails (missing key).
set +e
npm run tauri -- build
TAURI_EXIT=$?
set -e

# Resolve where cargo put release artifacts (CARGO_TARGET_DIR or default).
TARGET_BASE="${CARGO_TARGET_DIR:-/app/src-tauri/target}"
BUNDLE="$TARGET_BASE/release/bundle"

echo "== looking for artifacts under $BUNDLE (tauri exit=$TAURI_EXIT) =="
find "$BUNDLE" -type f \( -name '*.AppImage' -o -name '*.deb' \) -ls 2>/dev/null || true

APPIMAGE=""
DEB=""
shopt -s nullglob
for f in "$BUNDLE"/appimage/*.AppImage; do
  APPIMAGE="$f"
  break
done
for f in "$BUNDLE"/deb/*.deb; do
  DEB="$f"
  break
done
shopt -u nullglob

# If AppImage tooling failed (common without FUSE), rebuild from AppDir.
if [[ -z "$APPIMAGE" ]]; then
  APPDIR="$BUNDLE/appimage/MaxSpeech.AppDir"
  if [[ -d "$APPDIR" ]]; then
    echo "== finishing AppImage from AppDir =="
    apt-get install -y -qq wget file
    cd /tmp
    wget -q https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage \
      -O appimagetool.AppImage
    chmod +x appimagetool.AppImage
    ./appimagetool.AppImage --appimage-extract
    TOOL=/tmp/squashfs-root/AppRun
    mkdir -p "$BUNDLE/appimage"
    OUT="$BUNDLE/appimage/MaxSpeech_${VER}_amd64.AppImage"
    # Ensure desktop/icon at AppDir root for appimagetool
    if [[ -f "$APPDIR/usr/share/applications/MaxSpeech.desktop" ]]; then
      cp -f "$APPDIR/usr/share/applications/MaxSpeech.desktop" "$APPDIR/MaxSpeech.desktop"
      sed -i 's|^Exec=.*|Exec=maxspeech|' "$APPDIR/MaxSpeech.desktop"
      grep -q '^Icon=' "$APPDIR/MaxSpeech.desktop" || echo 'Icon=maxspeech' >> "$APPDIR/MaxSpeech.desktop"
    fi
    ICON="$(find "$APPDIR" -name 'maxspeech.png' | head -1 || true)"
    if [[ -n "$ICON" ]]; then
      cp -f "$ICON" "$APPDIR/maxspeech.png"
    fi
    if [[ ! -f "$APPDIR/AppRun" ]]; then
      wget -q https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-x86_64 \
        -O "$APPDIR/AppRun"
      chmod +x "$APPDIR/AppRun"
    fi
    ARCH=x86_64 "$TOOL" --no-appstream "$APPDIR" "$OUT"
    chmod +x "$OUT"
    APPIMAGE="$OUT"
  fi
fi

if [[ -z "$APPIMAGE" || ! -f "$APPIMAGE" ]]; then
  echo "error: AppImage missing after build" >&2
  exit 1
fi

# Validate the binary links resolve inside the AppImage (best-effort).
echo "== validating AppImage =="
ls -lh "$APPIMAGE"
VALIDATE_DIR=/tmp/ms-appimage-check
rm -rf "$VALIDATE_DIR"
mkdir -p "$VALIDATE_DIR"
cp "$APPIMAGE" "$VALIDATE_DIR/app.AppImage"
chmod +x "$VALIDATE_DIR/app.AppImage"
(cd "$VALIDATE_DIR" && ./app.AppImage --appimage-extract >/dev/null)
BIN="$VALIDATE_DIR/squashfs-root/usr/bin/maxspeech"
if [[ ! -x "$BIN" ]]; then
  echo "error: AppImage missing usr/bin/maxspeech" >&2
  exit 1
fi
# Count bundled .so files — a healthy Tauri AppImage ships dozens, not 1.
SO_COUNT="$(find "$VALIDATE_DIR/squashfs-root" -name '*.so*' | wc -l)"
echo "bundled shared libs: $SO_COUNT"
if [[ "$SO_COUNT" -lt 5 ]]; then
  echo "error: AppImage looks incomplete (only $SO_COUNT shared libs). Refusing to publish." >&2
  find "$VALIDATE_DIR/squashfs-root" -maxdepth 4 -type f | head -40 >&2
  ldd "$BIN" 2>&1 | head -40 >&2 || true
  exit 1
fi
MISSING="$(ldd "$BIN" 2>/dev/null | grep 'not found' || true)"
if [[ -n "$MISSING" ]]; then
  echo "warning: host ldd reports missing libs (may still be OK if bundled under usr/lib):"
  echo "$MISSING"
fi

mkdir -p /app/website/downloads
APPIMAGE_VER="/app/website/downloads/MaxSpeech_${VER}_amd64.AppImage"
APPIMAGE_STABLE="/app/website/downloads/MaxSpeech_amd64.AppImage"
cp -f "$APPIMAGE" "$APPIMAGE_VER"
cp -f "$APPIMAGE" "$APPIMAGE_STABLE"
chmod +x "$APPIMAGE_VER" "$APPIMAGE_STABLE"

if [[ -n "$DEB" && -f "$DEB" ]]; then
  DEB_VER="/app/website/downloads/maxspeech_${VER}_amd64.deb"
  DEB_STABLE="/app/website/downloads/maxspeech_amd64.deb"
  cp -f "$DEB" "$DEB_VER"
  cp -f "$DEB" "$DEB_STABLE"
else
  echo "warning: .deb not produced"
  DEB_VER=""
  DEB_STABLE=""
fi

# Also mirror into the workspace target path for local inspection.
mkdir -p /app/src-tauri/target/release/bundle/appimage
mkdir -p /app/src-tauri/target/release/bundle/deb
cp -f "$APPIMAGE" "/app/src-tauri/target/release/bundle/appimage/MaxSpeech_${VER}_amd64.AppImage"
if [[ -n "$DEB" && -f "$DEB" ]]; then
  cp -f "$DEB" "/app/src-tauri/target/release/bundle/deb/maxspeech_${VER}_amd64.deb"
fi

SIZE="$(stat -c%s "$APPIMAGE_STABLE")"
SHA="$(sha256sum "$APPIMAGE_STABLE" | awk '{print $1}')"

# Refresh platforms.linux in updates/latest.json without clobbering Windows fields.
python3 - <<PY
import json
from pathlib import Path
ver = "${VER}"
path = Path("/app/website/updates/latest.json")
data = json.loads(path.read_text(encoding="utf-8"))
platforms = data.setdefault("platforms", {})
platforms["linux"] = f"https://github.com/pauliscoool/maxspeech/releases/latest/download/MaxSpeech_{ver}_amd64.AppImage"
path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
print("updated website/updates/latest.json platforms.linux")
PY

# Keep build-installer.ps1 linux URL on GitHub Releases (versioned asset name).
python3 - <<PY
from pathlib import Path
import re
ver = "${VER}"
ps1 = Path("/app/scripts/build-installer.ps1")
text = ps1.read_text(encoding="utf-8")
text2 = re.sub(
    r'linux\s*=\s*"[^"]+"',
    f'linux   = "https://github.com/pauliscoool/maxspeech/releases/latest/download/MaxSpeech_{ver}_amd64.AppImage"',
    text,
)
if text2 != text:
    ps1.write_text(text2, encoding="utf-8")
    print("updated scripts/build-installer.ps1 linux URL")
PY

# Patch website/linux.html fallback asset names to this version.
python3 - <<PY
from pathlib import Path
import re
ver = "${VER}"
html = Path("/app/website/linux.html")
text = html.read_text(encoding="utf-8")
text = re.sub(
    r"MaxSpeech_[\d.]+_amd64\.AppImage",
    f"MaxSpeech_{ver}_amd64.AppImage",
    text,
)
text = re.sub(
    r"maxspeech_[\d.]+_amd64\.deb",
    f"maxspeech_{ver}_amd64.deb",
    text,
)
text = re.sub(r"Version [\d.]+", f"Version {ver}", text, count=1)
html.write_text(text, encoding="utf-8")
print("updated website/linux.html fallbacks")
PY

echo ""
echo "DONE AppImage size=${SIZE} sha256=${SHA} tauri_exit=${TAURI_EXIT}"
ls -lh "$APPIMAGE_VER" "$APPIMAGE_STABLE" ${DEB_VER:+$DEB_VER} ${DEB_STABLE:+$DEB_STABLE}

# Hint for host wrapper / operator
cat > /app/website/downloads/.linux-build-meta.json <<EOF
{
  "version": "${VER}",
  "appimage": "MaxSpeech_${VER}_amd64.AppImage",
  "deb": "maxspeech_${VER}_amd64.deb",
  "size": ${SIZE},
  "sha256": "${SHA}",
  "github_appimage": "https://github.com/pauliscoool/maxspeech/releases/latest/download/MaxSpeech_${VER}_amd64.AppImage",
  "github_deb": "https://github.com/pauliscoool/maxspeech/releases/latest/download/maxspeech_${VER}_amd64.deb"
}
EOF

