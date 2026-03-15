# poly-web-devtools-mcp — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-12

---

## MCP Preference (IMPORTANT — Updated 2026-03-12)

> **Prefer MCP mode over CLI subcommands.**

The MCP server is the primary integration point:
- Proper error handling and timeout protection baked into all calls
- Integrated with VS Code's built-in Copilot agent workflow
- Single shared `ChromeCdpBackend` instance (Chrome stays open, reuses session)
- Non-blocking background builds — Chrome auto-launches, pages reload on rebuild
- Full Chrome DevTools Protocol (CDP) support for advanced debugging

### When to use CLI (rare)

CLI subcommands are available for testing or scripting when MCP server is not needed:

```bash
cargo run --bin poly-web-devtools-mcp -- status
cargo run --bin poly-web-devtools-mcp -- launch  # polls background build
cargo run --bin poly-web-devtools-mcp -- screenshot --save devtools-screenshots/snap.png
cargo run --bin poly-web-devtools-mcp -- snapshot
cargo run --bin poly-web-devtools-mcp -- build-status
cargo run --bin poly-web-devtools-mcp -- build-log
cargo run --bin poly-web-devtools-mcp -- --headless screenshot --save snap.png
cargo run --bin poly-web-devtools-mcp -- help
```

VS Code CLI tasks under **"CLI: web — *"** exist but are not recommended for regular development.

**Always use MCP mode** (VS Code MCP integration or explicit MCP server launch) for production workflows.

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

## Timeout Behaviour (2026-03-10)

The shared MCP protocol now times out every tool call, and this backend also applies explicit
timeouts to CDP send/response waits.

So if the Chromium renderer freezes, you should now get a timeout error such as:
- tool timeout from shared MCP dispatch, or
- CDP send/response timeout from this backend

Do **not** treat timeouts as inconclusive. Treat them as evidence that the page or CDP session is wedged.
If the page is still partially responsive, inspect `window.__polyCrashState` to see whether the WASM app reported a panic/error overlay.

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
python3 -m http.server 3000 (static file server)
    └── Serves target/dx/poly-web/debug/web/public/
    (files produced by `dx build --platform web` in apps/web/)
```

**Port**: `WEB_SERVER_PORT = 3000` (NOT 8080 — desktop app uses 8080 for its asset server).

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

`launch_app` is **NON-BLOCKING** — it returns immediately (~1 s). The actual `dx build` runs
in the background (takes 30-90 s). You **must** poll `get_last_build_status` to know when
it finishes.

`launch_app` does (sequentially, all very fast):
1. **Guards** against concurrent builds — if one is already running, returns immediately with a message
2. **Kills** any stale Chrome, old static-file-server processes (synchronous, ~600 ms)
3. **Records** build state as `Running` and **returns** immediately
4. In background: runs `dx build --platform web` in `apps/web/`, starts python3 static file server
   on port 3000, launches Chrome with CDP on port 9222, starts auto-restart watchdog

**After calling `launch_app`:** poll `get_last_build_status` (every 5-10 s) until `state` is
`Succeeded` or `Failed`. Do NOT call `connect_cdp` immediately — the build will still be in progress.

```
# Poll loop:
get_last_build_status {}    ← repeat until state ≠ "Running"
```

### 2. Connect (only after Succeeded)

```
connect_cdp {}
```

**Only call `connect_cdp` after `get_last_build_status` reports `state = "Succeeded"`.**
If the build fails, call `get_last_build_log {}` to see the compiler/linker output.

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

For mobile web testing, combine Poly's forced mobile UI route with mobile emulation:

```text
http://127.0.0.1:3000/?mobile=1
```

Recommended phone preset:

```json
set_viewport {
  "width": 393,
  "height": 852,
  "mobile": true,
  "deviceScaleFactor": 3,
  "touch": true
}
```

This does two separate things:
- `?mobile=1` forces Poly's shared UI into mobile layout mode
- `set_viewport(... mobile=true ...)` tells Chromium to emulate mobile device metrics

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
| `build_id` | **⭐ PRIMARY INDICATOR**: Increments on each `rebuild_app` / `force_rebuild` call. Reads `/tmp/poly-devtools-web-rebuild-counter`. 0 = no rebuild this session. |
| `dx_serve_pid` | OS PID of the python3 static file server process (null if not started by this MCP). |

### ⭐ Complete Decision Table — Check All Three Together

To verify nothing changed, all three must be identical from the previous poll:

| `generation` | `build_id` | `dx_serve_pid` | Meaning |
|---|---|---|---|
| **Same** | **Same** | **Same** | ✅ No changes (no rebuild, no reconnect, no process restart) |
| Changed | Same | Same | CDP reconnect happened (no rebuild) |
| **Changed** | **Changed** | **Same** | 🔨 **Rebuild triggered + reconnected** (most common case) |
| Changed | Changed | Changed | 🔄 Static server restarted (full `launch_app` re-run) |
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

## Build Diagnostics — REQUIRED when generation is ambiguous (2026-03-10)

The web MCP now captures Dioxus stdout/stderr and exposes two new tools/CLI commands:

- `get_last_build_status` / `build-status`
- `get_last_build_log` / `build-log`

Use them immediately when:
- `build_id` changes but `generation` does not
- the page appears stale after `rebuild_app` or `force_rebuild`
- `connect_cdp` fails after a rebuild
- you need the exact `dx serve` / `dx build` output explaining a failure

`get_last_build_status` gives the structured summary.
`get_last_build_log` gives the raw Dioxus/Cargo output for the most recent web build attempt.

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
2. if counters look wrong, inspect `get_last_build_status` and `get_last_build_log`
3. `connect_cdp()` after the reload completes
4. Take a fresh snapshot/screenshot
5. Verify real app markers on the expected screen
6. **Note:** The toast DOM element may still appear in the snapshot even after successful rebuild —
   its presence does not indicate failure

Avoid `wait_for` on rebuild-toast strings. Wait for stable app content instead.

### Counter File

`/tmp/poly-devtools-web-rebuild-counter` — plain text U64, separate from the desktop
counter (`/tmp/poly-devtools-rebuild-counter`) to avoid cross-contamination when both MCPs run simultaneously.

---

### Build approach — `dx build` non-blocking (DECISION, 2026-03-12)

**All web builds/rebuilds use `dx build --platform web`, spawned in a background tokio task.**

- `launch_app` kills stale processes, records `state=Running`, **returns immediately**, spawns
  background task: `dx build --platform web` → python3 static server → Chrome watchdog
- `rebuild_app` increments rebuild counter, records `state=Running`, **returns immediately**, spawns
  background task: `dx build --platform web` → Chrome page reload via `Page.reload`
- `force_rebuild` is an extension-tool alias that delegates to `rebuild_app`

**Why non-blocking?**
- `dx build` can take 30-90 s — long enough to timeout VS Code's MCP client connection  
- Background tasks prevent connection drops while still capturing full build output
- Agent polls `get_last_build_status` until `state` transitions: `Running → Succeeded | Failed`
- Exit code + stdout/stderr are still captured and available via `get_last_build_log`

**Why `dx build` instead of `dx serve`:**
- No file watcher, no hotpatch, no ambiguous background state
- No WASM infinite-rebuild-loop (the infamous `dx serve --hotpatch` WASM bug)
- No stale incremental cache issue (full compile every time)

**Workflow after `rebuild_app` or `force_rebuild`:**
1. Poll `get_last_build_status` until `state = "Succeeded"` or `"Failed"`
2. On Succeeded: call `page_reload { ignoreCache: true }`, then `connect_cdp`
3. On Failed: call `get_last_build_log` to see the Cargo/Dioxus error
4. Verify with `get_generation` that `build_id` incremented

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
