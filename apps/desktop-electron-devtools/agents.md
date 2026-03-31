# Desktop Electron DevTools — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-07


## Purpose

Pure JavaScript/Electron directory used exclusively for **debugging** the Poly
Desktop Electron (WASM) build via Chrome DevTools Protocol (CDP).

This is **not a production build** — security settings are relaxed so CDP has
full access for developer tooling.

## Structure

```
apps/desktop-electron-devtools/
├── electron/
│   ├── package.json      # Electron dependency (^33.0.0)
│   └── main.js           # Electron main with CDP on port 9224
└── agents.md              # This file
```

## NOT a Rust Crate

This directory does **NOT** contain a `Cargo.toml`. It is pure Node.js and is
**NOT** a member of the Cargo workspace. The Rust devtools logic lives in
`mcp/electron-devtools-mcp/`.

## How It Works

1. The MCP server (`mcp/electron-devtools-mcp/`) first builds the WASM:
   ```bash
   dx build --platform web   # run in apps/desktop-electron/
   ```
2. Then launches this Electron app from `apps/desktop-electron-devtools/electron/`
3. Electron serves the WASM bundle directory over a loopback HTTP server and
   loads it via `http://127.0.0.1:<port>/` (not `file://`)
4. CDP is enabled on port **9224** via `app.commandLine.appendSwitch('remote-debugging-port', '9224')` in `main.js`
5. The MCP connects via CDP WebSocket for screenshots, JS eval, and interaction

## Key Configuration

| Setting | Value |
|---|---|
| CDP port | **9224** (distinct from web-devtools 9222, desktop HTTP 9223) |
| WASM source | `../../../target/dx/poly-desktop-electron/debug/web/public/` served over loopback HTTP |
| Window size | 1440×900 |
| DevTools | Closed by default; set `POLY_DEV_DEVTOOLS=1` to auto-open |
| Linux stability flags | `disable-dev-shm-usage` + `no-zygote` |
| contextIsolation | `false` (devtools build only — **never** for production) |

## Manual Usage (for testing without MCP)

```bash
# 1. Build the WASM bundle
cd apps/desktop-electron && dx build --platform web

# 2. Install npm deps (first time only)
cd ../desktop-electron-devtools/electron && npm install

# 3. Launch Electron (CDP always on :9224)
./node_modules/.bin/electron .
```

## Constraints

- **NEVER** add this directory to the Cargo workspace (it is pure JS)
- **NEVER** use this as a production build — security is relaxed for devtools
- **Always** run `dx build --platform web` in `apps/desktop-electron/` before launching
- The manager for this app is `mcp/electron-devtools-mcp/` — use that for automation

## Linux Note — Renderer Startup

On this project/workstation, Electron renderer startup was unreliable until the
launcher added:

- `app.commandLine.appendSwitch('disable-dev-shm-usage')`
- `app.commandLine.appendSwitch('no-zygote')`

Without those flags, CDP could come up while renderer-level commands like
`Runtime.evaluate` and `Page.captureScreenshot` failed or hung.

## IMPORTANT — Do NOT load the Dioxus bundle via `file://`

The generated Dioxus `index.html` uses absolute `/wasm/...` and `/assets/...`
paths. Under `file://`, those resolve against the filesystem root and the app
appears as a blank/gray window even though Electron itself launched.

For devtools, always serve the built bundle directory over a local HTTP server
and point `BrowserWindow` at `http://127.0.0.1:<port>/`.
