# apps/desktop-devtools

A special **Poly** desktop build with an embedded HTTP eval-bridge on port 9223. Used exclusively for dogfooding — inspecting, testing, and automating the Poly UI via the MCP devtools toolchain.

## Purpose

The normal `apps/desktop` build provides no programmatic access to the running UI. This build adds an Axum HTTP server that exposes:

| Endpoint | Method | Description |
|---|---|---|
| `/status` | GET | Health check — returns `"ok"` when ready |
| `/eval` | POST | Evaluate arbitrary JavaScript in the WebKit context |
| `/screenshot` | GET | Capture the current viewport as a PNG (using `SnapshotRegion::Visible`) |
| `/dom` | GET | Return `document.documentElement.outerHTML` |
| `/console` | GET | Return buffered `console.*` output (last 200 messages) |

The `poly-desktop-devtools-mcp` MCP server talks to this HTTP bridge and exposes it as MCP tools for GitHub Copilot.

## Architecture

```
GitHub Copilot
     │ MCP tools
     ▼
poly-desktop-devtools-mcp  (crates/poly-desktop-devtools-mcp)
     │ HTTP  :9223
     ▼
poly-desktop-devtools  (this app)
     │ dioxus eval() / WebKit2GTK snapshot
     ▼
WebKit webview running poly-core UI
```

## Building & Running

```bash
# Build the devtools binary
dx build --platform desktop

# Or run with live output (development)
dx serve --platform desktop
```

> **Important:** Use `dx build` + run the binary directly in production. `dx serve` adds a hot-reload proxy that interferes with the eval-bridge.

## Key Details

- Screenshot uses `webkit2gtk::SnapshotRegion::Visible` — coordinates are 1:1 with CSS pixels
- The `/eval` handler retries up to 5× on `EvalError::Finished` (transient after Dioxus navigations)
- The eval coroutine is driven by a `mpsc` channel from the HTTP handler into the Dioxus runtime

## License

MIT / Apache-2.0
