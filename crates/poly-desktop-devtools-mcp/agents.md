# poly-desktop-devtools-mcp — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

---

## Purpose

`poly-desktop-devtools-mcp` is the **desktop MCP server** for Poly. It implements the
`DevtoolsBackend` trait from `poly-devtools-protocol` using an HTTP eval-bridge
to communicate with the running `poly-desktop-devtools` app on port 9223.

This is how you verify UI changes in the **desktop (Wry/WebKit)** build.

For the **web (Chrome)** build, use `poly-web-devtools-mcp` instead.

---

## Architecture

```
VS Code Copilot / MCP Client
    │ JSON-RPC stdio
    ▼
poly-desktop-devtools-mcp (this crate)
    │ HTTP requests to 127.0.0.1:9223
    ▼
poly-desktop-devtools (apps/desktop-devtools/)
    ├── Embedded axum HTTP server (port 9223)
    ├── Bridges HTTP → dioxus eval() via use_coroutine + mpsc channel
    └── Renders the Poly UI in a Wry/WebKit webview
```

### Why HTTP, not Chrome CDP?

WebKit2GTK's inspector (port 9222 via `WEBKIT_INSPECTOR_SERVER`) uses a
**proprietary binary protocol**, NOT Chrome CDP. You cannot connect with
standard CDP/WebSocket libraries. The HTTP eval-bridge via dioxus `eval()` is
the only reliable path for the desktop build.

---

## How to Use (Every Session)

### 1. Launch the DevTools App

```
launch_app { workspace: "/home/laragana/workspcacemsg" }
```

This uses `dx build --platform desktop` (NOT `cargo build`) to get proper
`asset!()` processing, then launches the output binary.

Wait ~3 seconds for the app to start, then:

### 2. Connect

```
connect_cdp {}
```

Verifies the HTTP eval-bridge at `http://127.0.0.1:9223/status` is reachable.

### 3. Take a Screenshot

```
screenshot {}
```

Returns a PNG image captured via SVG foreignObject → Canvas → data URL.
**Note:** This method may not capture external images or iframes perfectly.
For pixel-perfect screenshots, use the `poly-web-devtools-mcp` backend (Chrome CDP).

### 4. Inspect the DOM / CSS

```
get_dom {}
js_eval { expression: "getComputedStyle(document.body).backgroundColor" }
```

### 5. Reset to Setup Wizard

```
reset_app {}
```

Kills the app, removes `~/.local/share/poly` data directory.
Call `launch_app` again to restart at the setup wizard.

---

## Build Notes

- **MUST use `dx build`** — `cargo build` leaves `asset!()` placeholder URLs intact 
- Binary output: `target/dx/poly-desktop-devtools/debug/linux/app/poly-desktop-devtools`
- CSS asset: `target/dx/poly-desktop-devtools/debug/linux/app/assets/tailwind-*.css`

## Debugging CSS Not Loading

If the app looks unstyled (white/transparent background):

```javascript
js_eval { expression: "document.querySelector('link[rel=stylesheet]').href" }
```

If the href contains "This should be replaced by dx", the app was built with
`cargo build` instead of `dx build`. Re-build with dx.

## Key Files

| File | Purpose |
|---|---|
| `src/main.rs` | `DesktopHttpBackend` impl + entry point |
| `Cargo.toml` | Dependencies (uses poly-devtools-protocol) |
