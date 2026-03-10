# poly-web-devtools-mcp — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-10

---

## CLI Preference (IMPORTANT — Updated 2026-03-10)

> **Prefer CLI over MCP whenever possible.**

All devtools functionality is available as CLI subcommands — no JSON-RPC overhead:

```bash
cargo run --bin poly-web-devtools-mcp -- status
cargo run --bin poly-web-devtools-mcp -- launch
cargo run --bin poly-web-devtools-mcp -- screenshot --save devtools-screenshots/snap.png
cargo run --bin poly-web-devtools-mcp -- snapshot
cargo run --bin poly-web-devtools-mcp -- eval "document.title"
cargo run --bin poly-web-devtools-mcp -- click "#my-button"
cargo run --bin poly-web-devtools-mcp -- fill "#input" "value"
cargo run --bin poly-web-devtools-mcp -- generation
cargo run --bin poly-web-devtools-mcp -- help

# Headless: add --headless before the subcommand
cargo run --bin poly-web-devtools-mcp -- --headless screenshot --save snap.png
```

VS Code CLI tasks are available under **"CLI: web — *"** in `.vscode/tasks.json`.

Use MCP mode (via `.vscode/mcp.json`) only when orchestrating multi-step sequences through Copilot agent mode.

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
    │ loads http://127.0.0.1:3000
    ▼
dx serve --platform web --port 3000 (apps/web/)
    └── Serves the Poly web app (WASM, no server component)
```

**Port**: `WEB_SERVER_PORT = 3000` (NOT 8080 — desktop dx serve claims 8080; we use 3000 to avoid conflict).

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

The MCP now automatically:
- **Kills any stale `dx serve`** on port 8080 (wrong port)
- **Kills any `dx serve --hotpatch`** (breaks WASM)
- **Starts the correct `dx serve --platform web --port 3000`**
- **Launches Chrome with CDP**
- **Starts the auto-restart watchdog**

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

## `get_generation` — Rebuild Detection Counters

The `get_generation` extension tool returns a JSON object with three fields:

```json
{ "generation": 4, "build_id": 2, "dx_serve_pid": 2898097 }
```

**All three fields are always included in every response** — they're not separate, they come together in one JSON object.

| Field | Semantics |
|---|---|
| `generation` | Increments on each successful `connect_cdp` call. Starts at 0 before first connect, 1 after. |
| `build_id` | **⭐ PRIMARY INDICATOR**: Increments on each `rebuild_app` call. Reads `/tmp/poly-devtools-web-rebuild-counter`. 0 = no rebuild this session. |
| `dx_serve_pid` | OS PID of the managed `dx serve` process (null if not started by this MCP). |

### ⭐ Complete Decision Table — Check All Three Together

To verify nothing changed, all three must be identical from the previous poll:

| `generation` | `build_id` | `dx_serve_pid` | Meaning |
|---|---|---|---|
| **Same** | **Same** | **Same** | ✅ No changes (no rebuild, no reconnect, no process restart) |
| Changed | Same | Same | 🔨 Hot-patch occurred (window alive, component remounted — rare under hotpatch) |
| **Changed** | **Changed** | **Same** | 🔨 **Rebuild triggered + reconnected** (most common case) |
| Changed | Changed | Changed | 🔄 `dx serve` restarted (full process restart) |
| Any changed | Any changed | Any changed | ⚠️ Something changed — investigate which field(s) |

**Key insight:** Check `build_id` first to know if a rebuild happened. If `build_id` is the same, no rebuild occurred — even if other fields changed.

### ⭐ **ALWAYS USE `build_id` TO DETECT REBUILDS**

**`build_id` is the universal, platform-independent way to know if a rebuild happened.**

For visual/screenshot testing: after each `rebuild_app()`, check `build_id` increased.
Do NOT rely on `generation` — it may not change if you haven't called `connect_cdp` yet.

### Important: web `generation` vs desktop `generation`

Web `generation` **correctly increments on every `connect_cdp` call** because each WASM
rebuild causes a full page reload, which drops the CDP WebSocket — requiring explicit
reconnection. You MUST call `connect_cdp` after each rebuild for `generation` to advance.

Desktop `generation` may stay at 1 across hot-patches because the Dioxus component state
is preserved (no page reload). This is the key behavioural difference between the two MCPs.

In both cases, **`build_id` is the reliable indicator** of "did a rebuild happen?"

### Counter file

`/tmp/poly-devtools-web-rebuild-counter` — plain text U64, separate from the desktop
counter (`/tmp/poly-devtools-rebuild-counter`) to avoid cross-contamination when running
both MCPs simultaneously.

### Visual verification (2026-03-02)

Three-rebuild test confirmed all counters work correctly:

| Step | Banner | `generation` | `build_id` | Notes |
|---|---|---|---|---|
| Baseline (launch + connect×2) | 🔴 Alpha | 2 | 0 | No rebuild yet |
| After rebuild + `connect_cdp` | 🟡 Beta | 3 | 1 | ✅ `build_id` increased immediately |
| After rebuild + `connect_cdp` | 🟢 Gamma | 4 | 2 | ✅ `build_id` increased immediately |

Decision table validated: **`build_id` advances immediately on `rebuild_app`** (before `connect_cdp`);
`generation` advances only after `connect_cdp`.

## Dioxus Rebuild Toast Warning (2026-03-08)

The browser may temporarily display Dioxus dev-runtime text like
**"Your app is being rebuilt"** during a rebuild.

Agents must **not** use that text as the primary signal for success/failure because:

- it is a transient Dioxus overlay, not Poly application state
- it can appear in screenshots during a healthy rebuild cycle
- a page can already be healthy again shortly after while a previous screenshot still captured it

Use this order instead:

1. `get_generation()` → confirm `build_id` changed if a rebuild was requested
2. `connect_cdp()` after the reload completes
3. Take a fresh snapshot/screenshot
4. Verify real app markers on the expected screen
5. **Note:** The toast DOM element may still appear in the snapshot even after successful rebuild —
   its presence does not indicate failure

Avoid `wait_for` on rebuild-toast strings. Wait for stable app content instead.

---

### NEVER use `--hotpatch` for web/WASM (DECISION)

`dx serve --hotpatch` is **explicitly prohibited** for the web platform.

When `--hotpatch` is enabled for WASM builds:
1. dx serve does a normal initial WASM build (shows progress bar at 100%)
2. It then immediately triggers a second "non-hot-reloadable" rebuild
3. The browser shows "Your app is being rebuilt" and gets stuck — it never
   resolves because the second build never sends a completion signal

This is a known limitation of Dioxus 0.7.3 experimental hotpatch mode with WASM.
Standard hot-reload (file-watcher → full WASM recompile → page refresh via
hot-reload WebSocket) works correctly without `--hotpatch`.

**`rebuild_app` strategy**: touch `crates/core/src/lib.rs` ONLY — do NOT also
send `r` to dx serve stdin. Sending both signals causes a double-rebuild loop.

### ⚠️ CRITICAL: Stale WASM Cache — When `rebuild_app` Fails (DECISION 2026-03-08)

**Symptom**: You call `rebuild_app`, the browser shows "Oops! build failed" or the app
shows the old code. `get_generation` confirms `build_id` changed but the UI hasn't updated.

**Root cause**: `dx serve` uses the `wasm-dev` incremental Cargo profile. After calling
`rebuild_app` (which touches `lib.rs`), `dx serve` receives the file-watch event and invokes
Cargo — but Cargo may consider all targets "fresh" (fingerprints match) and skip recompilation.
The WASM binary timestamp stays old. `dx serve` then serves the stale binary to the browser.

**Fix — use `force_rebuild`**:

```
force_rebuild {}
```

This tool runs `dx build --platform web` directly in `apps/web/`, completely bypassing
`dx serve`'s file-watcher and incremental cache detection. Cargo is forced to re-evaluate
all target freshness from scratch and writes a new WASM binary, which `dx serve` then serves.

After `force_rebuild` completes:
1. Call `page_reload { ignoreCache: true }`
2. Call `connect_cdp`
3. Verify with `get_generation` that both `build_id` and `generation` increased

**When to use `force_rebuild` vs `rebuild_app`:**

| Scenario | Tool to use |
|---|---|
| Normal code change during development | `rebuild_app` (fast, touches lib.rs) |
| `rebuild_app` result doesn't appear in browser | `force_rebuild` (forces full WASM build) |
| "Oops! build failed" toast in Poly | `force_rebuild` |
| WASM timestamp is older than source file | `force_rebuild` |

Note: `force_rebuild` takes 30–90 seconds (full compile). Use `rebuild_app` first, only
upgrade to `force_rebuild` if the browser doesn't show the updated code.

### Port 3000, NOT 8080

`dx serve --platform desktop` binds port 8080 for its hot-reload asset server.
This web MCP uses port `3000` (`WEB_SERVER_PORT = 3000`) to avoid the conflict.
Both desktop and web MCPs can run simultaneously.

---

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.

---

## Troubleshooting

See **`docs/web-devtools-setup.md`** for:
- Common issues and fixes
- Port reference
- Port conflict resolution
- When to use MCP vs manual `dx serve`
- Full cleanup scripts
