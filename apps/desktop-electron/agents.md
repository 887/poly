# Desktop Electron — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

## Purpose

Desktop entry point wrapping the Dioxus web (WASM) build inside an **Electron** shell. For users who prefer the Electron ecosystem or need Electron-specific features.

## How It Works

1. Build the Dioxus web target (WASM + HTML + JS + CSS)
2. Electron `main.js` creates a `BrowserWindow`, serves the built web app over
	loopback HTTP, and loads that local URL
3. All Poly logic runs as WASM inside Electron's Chromium webview
4. `src/main.rs` must call `poly_core::install_wasm_crash_handler()` before `dioxus::launch(App)`

## WASM Crash Visibility (2026-03-10)

Electron uses the same WASM frontend path as `apps/web`, so it now installs the
shared Poly browser crash handler before launch.

If the renderer hits a Rust panic, `window.onerror`, or `window.unhandledrejection`:

- crash metadata is stored on `window.__polyCrashState`
- a DOM overlay `#poly-wasm-crash-overlay` is injected into the page

When the Electron UI appears frozen, inspect `window.__polyCrashState` before assuming the build is stale.
If Electron MCP commands start timing out, assume the renderer thread may be wedged.

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

## Shared Shell JS (2026-03-07)

- Shared Electron shell logic lives in `electron/shared/`
	- `main_process.js` — asset server, window-state sync, window control IPC
	- `preload_bridge.js` — `window.polyElectron` preload bridge
- `apps/desktop-electron-devtools/electron/` imports these shared helpers instead
	of maintaining a second copy
- When changing Electron window controls, preload bridge shape, or bundle-serving
	logic, update the shared helpers first so the production and devtools shells
	stay behaviorally aligned

## Notes

- This is the heaviest desktop option (ships Chromium)
- Dioxus desktop with Wry is lighter and recommended as primary
- Electron wrapper exists for compatibility / user preference
- May have access to Node.js APIs via preload script for platform-specific features

## IMPORTANT — Bundle Loading

Do **not** load the generated Dioxus bundle with `loadFile(index.html)`.
The generated HTML references absolute `/wasm/...` and `/assets/...` URLs, which
break under `file://` and produce a blank/gray window.

Serve the built bundle directory over `http://127.0.0.1:<port>/` inside the
Electron main process and use `loadURL` instead.

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.
