# poly-electron-devtools-mcp — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-12

## MCP Preference (IMPORTANT — Updated 2026-03-12)

> **Prefer MCP mode over CLI subcommands.**

The MCP server is the primary integration point:
- Proper error handling and timeout protection baked into all calls
- Integrated with VS Code's built-in Copilot agent workflow
- Single shared `ElectronCdpBackend` instance (Electron window stays open, reuses CDP session)
- Non-blocking background builds — Electron launches when ready, pages reload on rebuild
- Full Chrome DevTools Protocol (CDP) support over Electron renderer

### When to use CLI (rare)

CLI subcommands are available for testing or scripting when MCP server is not needed:

```bash
cargo run --bin poly-electron-devtools-mcp -- status
cargo run --bin poly-electron-devtools-mcp -- launch  # polls background build
cargo run --bin poly-electron-devtools-mcp -- screenshot
cargo run --bin poly-electron-devtools-mcp -- snapshot
cargo run --bin poly-electron-devtools-mcp -- build-status
cargo run --bin poly-electron-devtools-mcp -- build-log
cargo run --bin poly-electron-devtools-mcp -- help
```

Default screenshot policy: **prefer inline screenshot output**. Use `--save ...` only when you explicitly need a file artifact.

VS Code CLI tasks under **"CLI: electron — *"** exist but are not recommended for regular development.

**Always use MCP mode** (VS Code MCP integration or explicit MCP server launch) for production workflows.

## Purpose

MCP server for debugging the Poly Desktop Electron (WASM/Electron) build.

Implements `DevtoolsBackend` via **Chrome DevTools Protocol (CDP)** over
WebSocket — architecturally similar to `poly-web-devtools-mcp` (which targets
Chrome) but adapted for the Electron runtime.

## Timeout Behaviour (2026-03-10)

The shared MCP protocol now wraps every tool call in a timeout, and this backend adds explicit
timeouts to CDP send/response waits.

If the Electron renderer freezes, MCP commands should now fail with timeout errors instead of hanging forever.
Treat those timeout errors as a real signal that the page/renderer is wedged.

If the renderer is still responsive enough for JS evaluation, inspect `window.__polyCrashState` to see whether the shared WASM crash handler recorded a panic or browser-side error.

## Architecture (Updated 2026-03-28 — Web-Shell Mode)

- **CDP port:** `9224` (distinct from web-devtools `9222` and desktop HTTP `9223`)
- **dx serve port:** `3001` (Electron loads WASM from this dev server)
- **Rebuild counter file:** `/tmp/poly-devtools-electron-rebuild-counter`
- **Backend struct:** `ElectronCdpBackend`
- **Thin shell:** `apps/desktop-electron-web/electron/` — stays alive across rebuilds
- **Build mode:** `dx serve --platform web --port 3001` (live dev server, NOT one-shot)
- **MCP name:** `"poly-electron"`

### Web-Shell Mode (Default)

The Electron MCP now uses a **web-shell architecture** where:
1. `dx serve --platform web --port 3001` runs in `apps/desktop-electron/`
2. The thin shell (`apps/desktop-electron-web/electron/`) loads from `http://127.0.0.1:3001/`
3. On rebuild, only the WASM page reloads — the Electron window stays alive
4. This matches how `poly-web-devtools-mcp` works with Chrome

### ELECTRON_RUN_AS_NODE

The MCP strips `ELECTRON_RUN_AS_NODE` and `ELECTRON_NO_ATTACH_CONSOLE` from the
environment when spawning Electron. If these are set (common in VS Code/Claude Code
terminals), Electron runs as plain Node.js and `require('electron')` fails.

### Orphan Process Cleanup

`launch_app`, `kill_app`, and `hard_kill` all run `pkill -f "poly-desktop-electron-web"`
to catch orphaned Electron child processes (main, GPU, network, renderer) from
previous MCP sessions — not just renderer processes matched by CDP port.

## Launch Sequence (NON-BLOCKING)

`launch_app(workspace)` is **non-blocking** — it returns in ~1 s. The actual build runs in
the background. **Required workflow:**

1. Call `launch_app { workspace }` → returns "Build started in background"
2. Loop: call `get_last_build_status` every 5-10 s until `state` ≠ `"Running"`
   - `"Succeeded"` → wait ~5 s, then call `connect_cdp`
   - `"Failed"` → call `get_last_build_log` to see the Cargo/Dioxus error
3. Do **NOT** call `connect_cdp` immediately after `launch_app` — the build will still be in progress

Background steps performed by `launch_app`:
1. Kill any existing Electron devtools process and stale CDP listeners via `pkill` (~1.3 s, sync)
2. Record `state = Running`, return immediately to caller
3. (background) Run `dx build --platform web` in `apps/desktop-electron/`
   (cold build: 2+ min; warm cache: 30-90 s)
4. (background) Run `npm install --prefer-offline` in `apps/desktop-electron-devtools/electron/`
5. (background) Launch `node_modules/.bin/electron .` (or `npx electron .` as fallback)
6. (background) Record `state = Succeeded` when Electron PID is up, or `Failed` on any error

## Reliability Notes

- The Electron launcher uses Chromium flags `disable-dev-shm-usage` and
   `no-zygote` to keep renderer startup stable on Linux systems where CDP can
   appear before the renderer is fully usable.
- `take_screenshot` now retries transient `Page.captureScreenshot` failures and
   brings the page to the foreground before capturing.
- `launch_app` performs both graceful and SIGKILL cleanup for stale Electron
   devtools processes before building and launching a fresh instance.

## Screenshot Policy (2026-03-17)

For agent-driven verification, screenshots should be **inline-first**:

- MCP screenshot calls should normally omit a file path so the image appears directly in chat.
- CLI `screenshot` without `--save` is the preferred example.
- Use saved files only for explicit archival evidence, stable path references, or when the user asks for them.

## Rebuild Flow (NON-BLOCKING)

`rebuild_app(workspace)` is **non-blocking** — returns in ~1 s:

1. Increment `/tmp/poly-devtools-electron-rebuild-counter` immediately
2. Record `state = Running`, return immediately
3. (background) Run `dx build --platform web`
4. (background) Send `Page.reload` via CDP on success
5. (background) Record `state = Succeeded` or `Failed`

**Polling workflow:**
```
rebuild_app { workspace }         # returns immediately
get_last_build_status {}          # repeat every 5-10 s
  state = "Running"  → keep polling
  state = "Succeeded" → call connect_cdp {}
  state = "Failed"   → call get_last_build_log {}
```

### `force_rebuild` Extension Tool

Delegates to `rebuild_app`. Same non-blocking behavior. Same polling required.

## get_generation Fields

| Field | Type | Meaning |
|---|---|---|
| `generation` | u64 | Increments on each `connect_cdp` call |
| `build_id` | u64 | Reads `/tmp/poly-devtools-electron-rebuild-counter` |
| `electron_pid` | u32? | PID of the managed Electron process (`null` if not launched by us) |

## Build Diagnostics — REQUIRED when generation is ambiguous (2026-03-10)

The Electron MCP now captures the exact `dx build --platform web` output and exposes:

- `get_last_build_status` / `build-status`
- `get_last_build_log` / `build-log`

Use them immediately when:
- `build_id` changed but the Electron window did not update
- `connect_cdp` fails after `launch_app` or `rebuild_app`
- you need the exact compiler / Dioxus CLI error for a failed Electron WASM build

`get_last_build_status` is the structured JSON summary.
`get_last_build_log` is the raw captured Dioxus/Cargo output.

## Dioxus Rebuild Toast Warning (2026-03-08)

Electron runs the Dioxus web bundle, so a rebuild/reload cycle may also surface temporary
dev-runtime text such as **"Your app is being rebuilt"**.

That text is **not** ground truth for whether the app is actually ready.

Agents should instead:

1. check `get_generation()`
2. confirm `build_id` increased after `launch_app` / `rebuild_app`
3. if counters look wrong, inspect `get_last_build_status` and `get_last_build_log`
4. reconnect with `connect_cdp()` after reload
5. verify the real target UI via snapshot/screenshot
6. **Note:** The toast DOM element may linger in snapshots/screenshots even after successful rebuild

Do not report failure solely because the rebuild toast appeared in a screenshot or DOM snapshot.

Backend behavior update (2026-03-17): when connect/screenshot/JS-eval runs and the real Poly
app root `#main` is already present, the Electron MCP now auto-hides the transient `#__dx-toast`
overlay before inspection. This reduces screenshot/snapshot noise, but agents must still use
build counters and real UI markers as the readiness source of truth.

## Key Differences from `web-devtools-mcp`

| Feature | `web-devtools-mcp` | `electron-devtools-mcp` |
|---|---|---|
| CDP port | `9222` | `9224` |
| dx serve port | `3000` | `3001` |
| Build command | `dx serve --platform web` | `dx serve --platform web --port 3001` |
| Browser | Chrome / Chromium | Electron (thin shell) |
| URL type | `http://localhost:3000` | `http://localhost:3001` |
| Native shell | None (Chrome) | `apps/desktop-electron-web/` |
| Watchdog (restart on crash) | Yes | No |
| Counter file | `…web-rebuild-counter` | `…electron-rebuild-counter` |

## Discovery Strategy

`discover_ws_url()` queries `http://127.0.0.1:9224/json`:

1. **1st preference:** page targets whose URL starts with `http://127.0.0.1:3001` (dev server)
2. **2nd preference:** page targets with `file://` or `app://` URLs (production mode)
3. **3rd preference:** any `page`-type target (handles `about:blank` while loading)

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

NEVER add lint suppression attributes. Fix the code instead.  
**Exception:** `#[allow(clippy::unwrap_used)]` / `expect_used` inside `#[cfg(test)]` only.
