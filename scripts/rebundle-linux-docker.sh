#!/usr/bin/env bash
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq xdg-utils fuse libfuse2

# shellcheck disable=SC1091
source "$HOME/.cargo/env"
export PATH="$HOME/.cargo/bin:$PATH"

cd /app
npm run tauri build

echo "== artifacts =="
find src-tauri/target/release/bundle -type f \( -name '*.AppImage' -o -name '*.deb' \) -ls
