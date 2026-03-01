# poly-web-devtools-mcp — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

---

## Purpose

`poly-web-devtools-mcp` is the **web MCP server** for Poly. It launches real
Chrome/Chromium with a **visible window** (default) or headless (`--headless`),
connects via the Chrome DevTools Protocol (CDP) over WebSocket, and provides
pixel-perfect inspection and interaction.

This is the preferred backend when you want:
- **Pixel-perfect screenshots** (via `Page.captureScreenshot`)
- **Native input events** (via `Input.dispatchMouseEvent`, `Input.insertText`)
- **Full CDP access** for advanced debugging

---

## Architecture

```
VS Code Copilot / MCP Client
    │ JSON-RPC stdio
    ▼
poly-web-devtools-mcp (this crate)
    │ CDP WebSocket to 127.0.0.1:9222
    ▼
Chrome / Chromium
    │ loads http://127.0.0.1:8080
    ▼
dx serve (apps/web/)
    └── Serves the Poly web app (Dioxus fullstack + Axum)
```

### Chrome Window Mode

- **Default:** Visible Chrome window — you can see exactly what the AI is doing
- **`--headless`:** Headless Chrome — for CI / automated testing

### Auto-Restart Watchdog

Chrome is managed by a background watchdog task:
- If Chrome crashes or exits, it is **automatically restarted** after 2 seconds
- The CDP WebSocket connection is cleared — call `connect_cdp` to reconnect
- The watchdog stops only when `kill_app` is called or the MCP server shuts down
- This handles OOM kills, Chrome crashes, accidental window closes, etc.

---

## How to Use (Every Session)

### 1. Launch

```
launch_app { workspace: "/home/laragana/workspcacemsg" }
```

This:
1. Starts `dx serve` on port 8080 (if not already running)
2. Launches Chrome with `--remote-debugging-port=9222`
3. Starts the auto-restart watchdog

Wait ~3 seconds, then:

### 2. Connect

```
connect_cdp {}
```

Discovers the CDP WebSocket URL via `GET http://127.0.0.1:9222/json`,
connects, and enables `Page`, `Runtime`, and `DOM` domains.

### 3. Screenshots (pixel-perfect!)

```
screenshot {}
```

Uses `Page.captureScreenshot` — real browser rendering, not JS canvas hacks.

### 4. Interact

```
click { x: 400, y: 300 }
type_text { text: "hello world" }
```

Uses CDP `Input.dispatchMouseEvent` and `Input.insertText` — real browser
input events, not JS dispatchEvent.

### 5. Extension Tools

```
page_reload { ignoreCache: true }
set_viewport { width: 1440, height: 900 }
```

### 6. Reset

```
reset_app {}
```

Clears localStorage, sessionStorage, IndexedDB, then reloads the page.
App restarts at the setup wizard.

---

## CLI Options

| Flag | Default | Description |
|---|---|---|
| (none) | visible | Launch Chrome with a visible window |
| `--headless` | off | Launch Chrome in headless mode (no window) |

## Chrome Discovery

The binary searches for Chrome/Chromium in this order:
1. `chromium`
2. `chromium-browser`
3. `google-chrome`
4. `google-chrome-stable`
5. Common absolute paths (`/usr/bin/`, `/snap/bin/`)

Falls back to `chromium` if nothing found.

## vs Desktop MCP (poly-desktop-devtools-mcp)

| Feature | Desktop MCP | Web MCP |
|---|---|---|
| Transport | HTTP eval-bridge (port 9223) | Chrome CDP WebSocket (port 9222) |
| Renderer | WebKit2GTK (Wry) | Chromium |
| Screenshots | SVG foreignObject → Canvas | Page.captureScreenshot (pixel-perfect) |
| Input | JS dispatchEvent | CDP Input.dispatch* (native) |
| Auto-restart | No | Yes (watchdog) |
| Headless option | No | Yes (`--headless`) |

## Key Files

| File | Purpose |
|---|---|
| `src/main.rs` | `ChromeCdpBackend` impl + watchdog + entry point |
| `Cargo.toml` | Dependencies (tokio-tungstenite for CDP, poly-devtools-protocol) |

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.
