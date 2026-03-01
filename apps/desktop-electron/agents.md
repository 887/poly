# Desktop Electron — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

## Purpose

Desktop entry point wrapping the Dioxus web (WASM) build inside an **Electron** shell. For users who prefer the Electron ecosystem or need Electron-specific features.

## How It Works

1. Build the Dioxus web target (WASM + HTML + JS + CSS)
2. Electron `main.js` creates a `BrowserWindow` and loads the built web app
3. All Poly logic runs as WASM inside Electron's Chromium webview

## Structure

```
apps/desktop-electron/
├── src/main.rs           # Dioxus web WASM entry point (same as apps/web but wrapped)
├── electron/
│   ├── package.json      # Electron + dependencies
│   ├── main.js          # Electron main process — creates window, loads WASM app
│   └── preload.js       # Preload script (if needed for node integration)
├── Dioxus.toml           # Web target config
└── agents.md             # This file
```

## Build Process

```bash
# 1. Build the WASM web app
dx build --release --platform web

# 2. Package with Electron
cd electron && npm run build  # Uses electron-builder or electron-packager
```

## Requirements

- Node.js + npm (for Electron toolchain)
- `electron` npm package
- `electron-builder` or `electron-packager` for distribution packaging

## Configuration

- `electron/package.json` — Electron version, build config
- `electron/main.js` — window size, title, menu, native integrations
- `Dioxus.toml` — web target (builds WASM bundle)

## Notes

- This is the heaviest desktop option (ships Chromium)
- Dioxus desktop with Wry is lighter and recommended as primary
- Electron wrapper exists for compatibility / user preference
- May have access to Node.js APIs via preload script for platform-specific features

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.
