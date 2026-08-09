#!/usr/bin/env bash
set -euo pipefail
echo "== packages =="
dpkg -l 2>/dev/null | grep -E 'libwebkit2gtk|libgtk-3-dev|libayatana|patchelf|libxdo|libasound|librsvg' || true
echo "== pkg-config webkit =="
pkg-config --exists webkit2gtk-4.1 && echo webkit4.1_ok || echo webkit4.1_missing
pkg-config --exists webkit2gtk-4.0 && echo webkit4.0_ok || echo webkit4.0_missing
pkg-config --exists gtk+-3.0 && echo gtk3_ok || echo gtk3_missing
echo "== tauri =="
command -v cargo-tauri || true
ls "$HOME/.cargo/bin" 2>/dev/null || true
