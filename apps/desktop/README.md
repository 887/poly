# apps/desktop

**Poly** desktop entry point — a Dioxus **fullstack** app that serves the WASM
bundle + host-bridge routes on port 3002. The `apps/desktop-web` Wry shell
connects to this dev server.

## Purpose

Compiles `poly-core`'s UI to WASM and runs a native axum server on port 3002.
The server half mounts `/host/*` (SQLite-backed KV, native HTTP proxy,
subprocess exec) on the same port as the WASM bundle — one process, one port.
The thin Wry shell in `apps/desktop-web` loads from this server.

## How It Works

1. `dx serve --platform web --fullstack` compiles the WASM client + native server
2. The server serves both the WASM bundle and `/host/*` routes on port 3002
3. `apps/desktop-web` (Wry shell) loads from `http://127.0.0.1:3002/`
4. On code changes, only the WASM reloads — the Wry window stays alive

## Running

```bash
# Development (fullstack with host-bridge)
cd apps/desktop
dx serve --platform web --fullstack \
  @client --no-default-features --features "dev-plugins,web" \
  @server --platform server --no-default-features --features "dev-plugins,server"

# Then launch the Wry shell (done automatically by poly-desktop-devtools-mcp):
cargo run -p poly-desktop-web
```

The `@server --platform server` flag is required — without it dx tries to build
the server half for wasm32 and fails.

## Host-Bridge Routes (port 3002)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/host/status` | GET | Liveness ping |
| `/host/kv/get` | POST | App KV read (SQLite backed) |
| `/host/kv/set` | POST | App KV write |
| `/host/kv/delete` | POST | App KV delete |
| `/host/kv/clear` | POST | App KV wipe |
| `/host/exec` | POST | Spawn subprocess |
| `/host/http` | POST | Native HTTP via reqwest |
| `/host` | POST | Legacy tagged-union dispatch |

Storage: `storage.sqlite3` under the OS data dir (see root README).

## Key Dependencies

| Crate | Role |
|---|---|
| `poly-core` | All UI, state, i18n, theming |
| `poly-host` | Host-bridge axum router + SQLite state |
| `dioxus` (`fullstack` + `web` features) | WASM client + native server |

## Platform Notes

- **Linux**: The Wry shell requires WebKit2GTK (`libwebkit2gtk-4.1`).
  Set `WEBKIT_DISABLE_DMABUF_RENDERER=1` on Wayland to avoid DMA-BUF crashes.
- **Windows**: WebView2 runtime (ships with Windows 11+)
- **macOS**: WKWebView (built-in)

## License

MIT / Apache-2.0
