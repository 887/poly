# apps/desktop-electron-web

**Thin Electron shell** for Poly Electron web-shell development mode.

## Purpose

A lightweight Electron window that loads the Poly WASM app from a `dx serve` dev server. The Electron process **stays alive across WASM rebuilds** — only the page reloads. This enables Chrome-like hot-reload development for Electron.

This shell is launched automatically by `poly-electron-devtools-mcp`. You should not need to run it manually.

## How It Works

1. Electron creates a frameless `BrowserWindow` (1280x800, custom CSS titlebar)
2. Loads `http://127.0.0.1:${POLY_DEV_SERVE_PORT:-3001}/` from the dx serve dev server
3. CDP (Chrome DevTools Protocol) enabled on port 9224 for remote debugging
4. The preload bridge exposes `window.polyElectron` for platform detection and window controls
5. The Dioxus fullstack binary bound to port 3001 serves BOTH the WASM
   bundle AND the `/host/*` host-bridge routes. Electron does not run
   its own bridge — it's a pure Chromium renderer pointed at the
   fullstack server. Storage lives in the same SQLite file every other
   Poly shell uses (`storage.sqlite3` under the OS data dir).

### Host-bridge routes (served by `dx serve`, not Electron)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/host/status` | GET | Liveness ping |
| `/host/kv/get` | POST | App KV read (SQLite backed) |
| `/host/kv/set` | POST | App KV write |
| `/host/kv/delete` | POST | App KV delete |
| `/host/kv/clear` | POST | App KV wipe |
| `/host/exec` | POST | Spawn subprocess |
| `/host/http` | POST | Native HTTP via reqwest |
| `/host` | POST | Legacy tagged-union `HostCall` dispatch |

The Rust server reuses `poly_host::router` / `HostState` so the
schema, busy-timeout, and data-dir resolution match every other
shell exactly. Tracked in
`docs/plans/phase-2.21-host-bridge-unification-plan.md`.

## Structure

```
apps/desktop-electron-web/
└── electron/
    ├── main.js               # Electron main process — frameless window, CDP, dev server URL
    ├── package.json          # Electron dependency (devDependencies only)
    ├── preload.js            # Preload script — exposes polyElectron bridge
    └── shared/
        ├── main_process.js   # Window state sync, IPC handlers, asset server
        └── preload_bridge.js # window.polyElectron API (isElectron, version, window controls)
```

## Key Implementation Notes

- **Frameless window:** `frame: false` — the app provides its own CSS titlebar via `.electron-titlebar`
- **No `titleBarStyle`/`titleBarOverlay`:** These properties conflict with `frame: false` on Linux and cause rendering offsets
- **Electron binary:** The MCP uses the `electron` binary from `apps/desktop-electron/electron/node_modules/.bin/electron` (the production app's copy) to avoid `require('electron')` shadowing from a local `node_modules/electron`
- **`electron` in devDependencies:** The npm `electron` package is in devDependencies to prevent it from being installed in `node_modules/` at runtime (which would shadow Electron's built-in module)
- **`ELECTRON_RUN_AS_NODE`:** The MCP strips this env var when spawning Electron — if set, Electron runs as plain Node.js and `require('electron')` fails

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `POLY_DEV_SERVE_PORT` | `3001` | Port of the dx serve dev server |
| `POLY_ELECTRON_REMOTE_DEBUGGING_PORT` | `9224` | CDP remote debugging port |
| `POLY_DEVTOOLS` | unset | Set to `1` to auto-open DevTools |

## Shared Code

The `shared/` directory contains helpers shared with `apps/desktop-electron/electron/`:
- `main_process.js` — asset server, window state listeners, IPC registration
- `preload_bridge.js` — `window.polyElectron` bridge (isElectron, version, minimize/maximize/close)

## License

MIT / Apache-2.0
