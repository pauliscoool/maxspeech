#!/usr/bin/env bash
set -euo pipefail

PROJECT="/mnt/c/Users/Paul Dimov/Projects/maxspeech"
cd "$PROJECT"

echo "== apt deps =="
sudo DEBIAN_FRONTEND=noninteractive apt-get update -qq
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev \
  patchelf libssl-dev libxdo-dev libasound2-dev libdbus-1-dev pkg-config \
  build-essential curl wget file

echo "== rust/cargo =="
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env" 2>/dev/null || true
export PATH="$HOME/.cargo/bin:$PATH"

echo "== tauri-cli =="
if ! command -v cargo-tauri >/dev/null 2>&1; then
  cargo install tauri-cli --version "^2" --locked
fi

echo "== npm ci =="
npm ci

echo "== tauri build =="
npm run tauri build

echo "== artifacts =="
find src-tauri/target/release/bundle -type f \( -name '*.AppImage' -o -name '*.deb' \) -ls || true
