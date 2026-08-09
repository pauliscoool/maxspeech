#!/usr/bin/env bash
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq \
  curl wget file build-essential pkg-config \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf libssl-dev libxdo-dev libasound2-dev libdbus-1-dev \
  ca-certificates xdg-utils fuse libfuse2

# Node 20
if ! command -v node >/dev/null 2>&1; then
  curl -fsSL https://deb.nodesource.com/setup_20.x | bash -
  apt-get install -y -qq nodejs
fi

# Rust
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v cargo-tauri >/dev/null 2>&1; then
  cargo install tauri-cli --version "^2" --locked
fi

cd /app
# Frontend already built; skip if dist exists
if [ ! -d dist ]; then
  npm ci
  npm run build
fi

# Bundle only (binary already compiled) — still run full tauri build; should be incremental
npm run tauri build

echo "== artifacts =="
find src-tauri/target/release/bundle -type f \( -name '*.AppImage' -o -name '*.deb' \) -ls
