# apps/desktop-web

**Thin native Wry shell** for Poly Desktop web-shell development mode.

## Purpose

A lightweight native window (Wry/tao) that loads the Poly WASM app from a `dx serve` dev server. The shell **never** gets recompiled during development — only the WASM page reloads when `dx serve` finishes rebuilding.

This binary is launched automatically by `poly-desktop-devtools-mcp` in web-shell mode (the default). You should not need to run it manually.

## How It Works

1. Opens a native Wry/tao window (1440x900)
2. Loads `http://127.0.0.1:${POLY_DEV_URL:-3002}/` in the webview
3. Starts an HTTP eval-bridge on **port 9223** so `poly-desktop-devtools-mcp` can drive the app
4. On each page load, injects a JS bootstrap that bridges `window.ipc` back to the eval bridge

## Eval-Bridge Architecture

```
tokio HTTP server → UserEvent::EvalRequest → EventLoopProxy
    → tao event loop → webview.evaluate_script()
    → JS calls window.ipc.postMessage(JSON)
    → Wry IPC handler → oneshot channel → HTTP response
```

### HTTP Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/status` | GET | Health check (returns `"ok"`) |
| `/generation` | GET | Build generation + PID info |
| `/eval` | POST | Evaluate JS in the webview (with retries) |
| `/screenshot` | GET | Capture PNG via WebKit2GTK snapshot API |
| `/dom` | GET | Return `document.documentElement.outerHTML` |
| `/console` | GET | Return buffered console log entries |
| `/reload` | POST | Navigate webview back to the dev URL |

## Screenshot

Screenshots use WebKit2GTK's native `snapshot()` API, triggered via `UserEvent::ScreenshotRequest` on the GTK main thread.

## Key Implementation Notes

- Uses `window.default_vbox()` (not `gtk_window()`) for `build_gtk` — required for proper GTK size allocation
- Console log interceptor buffers last 200 entries for `/console` endpoint
- Eval requests have 15s timeout with up to 5 retries on transient errors

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `POLY_DEV_URL` | `http://127.0.0.1:3002` | URL of the dx serve dev server |

## License

MIT / Apache-2.0
