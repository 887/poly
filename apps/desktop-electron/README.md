# apps/desktop-electron

**Poly** Electron desktop entry point — a Dioxus **fullstack** app that serves
the WASM bundle + host-bridge routes on port 3001. The `apps/desktop-electron-web`
Electron thin shell connects to this dev server.

## Purpose

An alternative desktop distribution using Electron's Chromium webview instead of
the OS-native webview. Ships Chromium, so behaviour is identical to the web app
regardless of the host OS's native webview.

> **Note:** `apps/desktop` (Wry) is the recommended desktop target — it is
> lighter (no bundled Chromium). This entry point exists for compatibility and
> user preference.

## How It Works

1. `dx serve --platform web --fullstack` compiles the WASM client + native server
2. The server serves both the WASM bundle and `/host/*` routes on port 3001
3. `apps/desktop-electron-web` (Electron shell) loads from `http://127.0.0.1:3001/`
4. On code changes, only the WASM reloads — the Electron window stays alive
5. CDP remote debugging on port 9224

## Running

```bash
# Development (fullstack with host-bridge)
cd apps/desktop-electron
dx serve --platform web --fullstack \
  @client --no-default-features --features "dev-plugins,web" \
  @server --platform server --no-default-features --features "dev-plugins,server"

# Then launch the Electron shell (done automatically by poly-electron-devtools-mcp)
```

The `@server --platform server` flag is required — without it dx tries to build
the server half for wasm32 and fails.

## Structure

```
apps/desktop-electron/
├── src/main.rs           # Dioxus fullstack entry point (WASM + server)
├── Cargo.toml            # Features: dev-plugins, production, web, server
├── Dioxus.toml           # Web target build config (port 3001)
└── electron/
    ├── package.json      # Electron dependency
    └── node_modules/     # Electron binary lives here
```

The thin Electron shell lives in `apps/desktop-electron-web/electron/`.

## Host-Bridge Routes (port 3001)

Same routes as `apps/desktop` — see root README or
`docs/1-architecture/1.2-host-bridge.md`.

## Requirements

- Node.js + npm (for the Electron shell in `apps/desktop-electron-web`)
- `electron` npm package

## Key Implementation Notes

- **`ELECTRON_RUN_AS_NODE`**: The MCP strips this env var when spawning Electron.
  If set (e.g. by VS Code terminals), Electron runs as plain Node.js and
  `require('electron')` fails.
- **Frameless window**: The Electron shell uses `frame: false` — do NOT combine
  with `titleBarStyle` or `titleBarOverlay` on Linux.

## License

MIT / Apache-2.0
