#!/usr/bin/env bash
# Build MaxSpeech .dmg on macOS and copy it into website/downloads/.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export CARGO_TERM_COLOR=always

VER="$(node -p "require('./src-tauri/tauri.conf.json').version")"
ARCH="$(uname -m)"
case "$ARCH" in
  arm64|aarch64) ARCH_LABEL="aarch64" ;;
  x86_64) ARCH_LABEL="x64" ;;
  *) ARCH_LABEL="$ARCH" ;;
esac

echo "Building MaxSpeech $VER .dmg (arch=$ARCH_LABEL)..."

# Bundles often succeed even when updater signing fails (missing TAURI_SIGNING_PRIVATE_KEY).
set +e
npm run tauri -- build --target "${ARCH_LABEL}-apple-darwin"
TAURI_EXIT=$?
set -e

# Prefer cross-target output, then default release bundle.
CANDIDATES=(
  "$ROOT/src-tauri/target/${ARCH_LABEL}-apple-darwin/release/bundle/dmg/"*.dmg
  "$ROOT/src-tauri/target/release/bundle/dmg/"*.dmg
)

DMG_SRC=""
for pattern in "${CANDIDATES[@]}"; do
  for f in $pattern; do
    if [[ -f "$f" ]]; then
      DMG_SRC="$f"
      break 2
    fi
  done
done

if [[ -z "$DMG_SRC" ]]; then
  echo "error: .dmg not found under src-tauri/target/.../bundle/dmg/ (tauri exit=$TAURI_EXIT)" >&2
  exit 1
fi

DEST_NAME="MaxSpeech_${VER}_${ARCH_LABEL}.dmg"
WEB_DL="$ROOT/website/downloads/$DEST_NAME"
mkdir -p "$(dirname "$WEB_DL")"
cp -f "$DMG_SRC" "$WEB_DL"

echo ""
echo "DMG ready:"
echo "  $DMG_SRC"
echo "  $WEB_DL"
echo ""
echo "Next: update website/mac.html links to /downloads/$DEST_NAME and deploy website/."
if [[ "$TAURI_EXIT" -ne 0 ]]; then
  echo "Note: tauri exited $TAURI_EXIT (often updater signing). DMG above is still usable." >&2
fi
