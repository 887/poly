# apps/desktop-electron

**Poly** desktop app packaged as an [Electron](https://www.electronjs.org/) application. Runs the Dioxus web (WASM) build inside Electron's Chromium webview.

## Purpose

An alternative desktop distribution for users who prefer the Electron ecosystem or require Electron-specific platform integrations (native menus, tray, notifications via Electron APIs). Ships Chromium, so behaviour is identical to the web app regardless of the host OS's native webview.

> **Note:** `apps/desktop` (Wry) is the recommended desktop target — it is lighter (no bundled Chromium). This entry point exists for compatibility and user preference.

## How It Works

1. `src/main.rs` — Dioxus web WASM entry point (same target as `apps/web`)
2. `dx build --platform web` compiles it to WASM + HTML + JS + CSS
3. `electron/main.js` creates an Electron `BrowserWindow` and loads the built web app
4. All Poly logic runs as WASM inside Electron's Chromium webview

## Structure

```
apps/desktop-electron/
├── src/main.rs           # Dioxus WASM entry point
├── electron/
│   ├── package.json      # Electron dependencies + build config
│   ├── main.js           # Electron main process — creates window, loads WASM app
│   └── preload.js        # Preload script for any Node.js bridge
└── Dioxus.toml           # Web target build config
```

## Building

```bash
# 1. Build the WASM web app
dx build --release --platform web

# 2. Package with Electron
cd electron && npm install && npm run build
```

## Requirements

- Node.js + npm (Electron toolchain)
- `electron` npm package
- `electron-builder` or `electron-packager` for distribution

## License

MIT / Apache-2.0
