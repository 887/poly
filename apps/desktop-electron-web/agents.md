# Desktop Electron Web Shell — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.
> **Last Updated:** 2026-03-28


## Purpose

Thin Electron shell for web-shell development mode. Launched automatically by
`poly-electron-devtools-mcp` — you should never need to run this manually.

## How It Works

- Electron BrowserWindow loads Poly WASM from `dx serve` on port 3001
- CDP on port 9224 for `poly-electron-devtools-mcp` remote debugging
- The window **stays alive across WASM rebuilds** — only the page content reloads
- Custom frameless titlebar with drag region, nav buttons, and window controls

## Key Implementation Details

- **Frameless:** `frame: false` only. Do NOT add `titleBarStyle` or `titleBarOverlay` —
  they conflict on Linux and cause pixel offsets at top/bottom.
- **Electron binary resolution:** The MCP uses the production app's electron binary
  (`apps/desktop-electron/electron/node_modules/.bin/electron`) to avoid the npm
  `electron` package shadowing the built-in `require('electron')` module.
- **`ELECTRON_RUN_AS_NODE`:** Must be stripped from env when launching. If present,
  Electron runs as plain Node.js and `require('electron')` fails with
  "Cannot find module 'electron'". The MCP handles this automatically.
- **Sandbox:** The MCP sets `--no-sandbox` and `ELECTRON_DISABLE_SANDBOX=1`
  because `chrome-sandbox` requires setuid root on Linux.

## Shared Shell JS

- `shared/main_process.js` and `shared/preload_bridge.js` are shared with
  `apps/desktop-electron/electron/`
- When changing Electron window controls, preload bridge, or bundle-serving
  logic, update the shared helpers so both shells stay aligned

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.
