#!/usr/bin/env bash
# Build script for the Poly Desktop (Electron) app.
#
# Usage:
#   ./build.sh            # debug build + launch dev electron
#   ./build.sh --release  # release WASM build + electron-builder package
#
# Requirements:
#   - dx (Dioxus CLI): cargo install dioxus-cli
#   - Node.js + npm: https://nodejs.org
#   - electron npm package (installed via npm install)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

RELEASE="${1:-}"

echo "==> Building Poly WASM (desktop-electron target)..."
if [[ "$RELEASE" == "--release" ]]; then
    dx build --release --platform web
else
    dx build --platform web
fi

echo "==> Installing Electron npm dependencies..."
cd electron
npm install --prefer-offline 2>/dev/null || npm install

if [[ "$RELEASE" == "--release" ]]; then
    echo "==> Packaging with electron-builder..."
    npm run dist
    echo ""
    echo "Packaged app is in electron/dist-electron/"
else
    echo "==> Launching Electron in dev mode..."
    POLY_DEV=1 npx electron .
fi
