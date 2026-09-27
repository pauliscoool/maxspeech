#!/usr/bin/env bash
# Build MaxSpeech Linux AppImage + .deb (via Docker) and copy into website/downloads/.
# Run from Windows (Git Bash / WSL) or native Linux with Docker available.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VER="$(node -p "require('./src-tauri/tauri.conf.json').version")"
IMAGE="${MAXSPEECH_LINUX_IMAGE:-ubuntu:22.04}"

echo "Building MaxSpeech $VER Linux packages in Docker ($IMAGE)..."

if ! command -v docker >/dev/null 2>&1; then
  echo "error: docker is required" >&2
  exit 1
fi

# Keep cargo/npm caches across runs for faster rebuilds.
docker volume create maxspeech-cargo-registry >/dev/null
docker volume create maxspeech-cargo-git >/dev/null
docker volume create maxspeech-cargo-target >/dev/null
docker volume create maxspeech-npm-cache >/dev/null

docker run --rm \
  --network host \
  -e DEBIAN_FRONTEND=noninteractive \
  -e APPIMAGE_EXTRACT_AND_RUN=1 \
  -e CARGO_HOME=/cargo \
  -e CARGO_TARGET_DIR=/target \
  -e npm_config_cache=/npm-cache \
  -e TAURI_SIGNING_PRIVATE_KEY="${TAURI_SIGNING_PRIVATE_KEY:-}" \
  -e TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" \
  -v "$ROOT:/app" \
  -v maxspeech-cargo-registry:/cargo/registry \
  -v maxspeech-cargo-git:/cargo/git \
  -v maxspeech-cargo-target:/target \
  -v maxspeech-npm-cache:/npm-cache \
  -w /app \
  "$IMAGE" \
  bash /app/scripts/build-linux-docker.sh

# Artifacts land under the host-mounted repo via CARGO_TARGET_DIR symlink or copy step.
BUNDLE_ROOT="$ROOT/src-tauri/target/release/bundle"
# Prefer the docker volume target (copied out by the docker script).
if [[ ! -d "$BUNDLE_ROOT" ]]; then
  BUNDLE_ROOT="$ROOT/src-tauri/target-linux/release/bundle"
fi

APPIMAGE=""
DEB=""
shopt -s nullglob
for f in "$ROOT"/src-tauri/target/release/bundle/appimage/*.AppImage \
         "$ROOT"/website/downloads-staging/*.AppImage; do
  [[ -f "$f" ]] && APPIMAGE="$f" && break
done
for f in "$ROOT"/src-tauri/target/release/bundle/deb/*.deb \
         "$ROOT"/website/downloads-staging/*.deb; do
  [[ -f "$f" ]] && DEB="$f" && break
done
shopt -u nullglob

# The docker script copies finalized files into website/downloads/ itself.
APPIMAGE_STABLE="$ROOT/website/downloads/MaxSpeech_amd64.AppImage"
APPIMAGE_VER="$ROOT/website/downloads/MaxSpeech_${VER}_amd64.AppImage"
DEB_STABLE="$ROOT/website/downloads/maxspeech_amd64.deb"
DEB_VER="$ROOT/website/downloads/maxspeech_${VER}_amd64.deb"

if [[ ! -f "$APPIMAGE_VER" && ! -f "$APPIMAGE_STABLE" ]]; then
  echo "error: AppImage not found after build" >&2
  find "$ROOT/src-tauri/target" -name '*.AppImage' 2>/dev/null | head -20 || true
  exit 1
fi

echo ""
echo "Linux packages ready:"
ls -lh "$APPIMAGE_VER" "$APPIMAGE_STABLE" "$DEB_VER" "$DEB_STABLE" 2>/dev/null || \
  ls -lh "$ROOT/website/downloads/"*AppImage "$ROOT/website/downloads/"*.deb 2>/dev/null
echo ""
echo "Next: deploy website/ (vercel deploy --prod -y && alias maxspeech.vercel.app)."
