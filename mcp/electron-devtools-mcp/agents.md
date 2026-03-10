# poly-electron-devtools-mcp — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-10

## CLI Preference (IMPORTANT — Updated 2026-03-10)

> **Prefer CLI over MCP whenever possible.**

All devtools functionality is available as CLI subcommands — no JSON-RPC overhead:

```bash
cargo run --bin poly-electron-devtools-mcp -- status
cargo run --bin poly-electron-devtools-mcp -- launch
cargo run --bin poly-electron-devtools-mcp -- screenshot --save devtools-screenshots/snap.png
cargo run --bin poly-electron-devtools-mcp -- snapshot
cargo run --bin poly-electron-devtools-mcp -- eval "document.title"
cargo run --bin poly-electron-devtools-mcp -- click "#my-button"
cargo run --bin poly-electron-devtools-mcp -- fill "#input" "value"
cargo run --bin poly-electron-devtools-mcp -- generation
cargo run --bin poly-electron-devtools-mcp -- build-status
cargo run --bin poly-electron-devtools-mcp -- build-log
cargo run --bin poly-electron-devtools-mcp -- help
```

VS Code CLI tasks are available under **"CLI: electron — *"** in `.vscode/tasks.json`.

Use MCP mode (via `.vscode/mcp.json`) only when orchestrating multi-step sequences through Copilot agent mode.

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

## Architecture

- **CDP port:** `9224` (distinct from web-devtools `9222` and desktop HTTP `9223`)
- **Rebuild counter file:** `/tmp/poly-devtools-electron-rebuild-counter`
- **Backend struct:** `ElectronCdpBackend`
- **No watchdog:** Electron is a single stable process; user closing it is intentional
- **No `dx serve`:** uses `dx build --platform web` (one-shot builds, not watch mode)
- **MCP name:** `"poly-electron"`

## Launch Sequence

`launch_app(workspace)` performs these steps *in order*:

1. Kill any existing Electron devtools process and stale CDP listeners via `pkill`
2. Run `dx build --platform web` in `apps/desktop-electron/` — **waits until done**
   (cold build: 2+ min; warm cache: 30–90 s)
3. Run `npm install --prefer-offline` in `apps/desktop-electron-devtools/electron/`
4. Launch `node_modules/.bin/electron .` (or `npx electron .` as fallback)
5. Electron auto-configures CDP on port 9224 via `app.commandLine.appendSwitch` in `main.js`
6. Store the Electron PID for later `kill_app` / `hard_kill`

Then wait ~5 seconds and call `connect_cdp`.

## Reliability Notes

- The Electron launcher uses Chromium flags `disable-dev-shm-usage` and
   `no-zygote` to keep renderer startup stable on Linux systems where CDP can
   appear before the renderer is fully usable.
- `take_screenshot` now retries transient `Page.captureScreenshot` failures and
   brings the page to the foreground before capturing.
- `launch_app` performs both graceful and SIGKILL cleanup for stale Electron
   devtools processes before building and launching a fresh instance.

## Rebuild Flow

`rebuild_app(workspace)`:
1. Increment `/tmp/poly-devtools-electron-rebuild-counter`
2. Run `dx build --platform web` again — **waits until done**
3. Send `Page.reload` via CDP
4. Clear WebSocket (reload invalidates the debugger session)

Caller must call `connect_cdp` afterwards to re-establish CDP.

### `force_rebuild` Extension Tool

The Electron MCP's `rebuild_app` already uses `dx build --platform web` (not touch+file-watcher),
so it doesn't suffer the stale-cache issue that affects the web MCP.

However, a `force_rebuild` tool is exposed for consistency with the other MCPs.
It does the same thing as `rebuild_app` — runs `dx build --platform web` in
`apps/desktop-electron/` — but it's explicitly called out as a "full rebuild".

After `force_rebuild`, call `connect_cdp`.

### Background: Web MCP Stale Cache Issue (DECISION 2026-03-08)

The web MCP (`poly-web-devtools-mcp`) uses `dx serve --platform web --port 3000` for
hot-reloading. Its `rebuild_app` touches `lib.rs` to trigger the file-watcher — but
`dx serve`'s `wasm-dev` incremental Cargo cache can become stale: Cargo fingerprints
may consider all targets "fresh" and skip recompilation, leaving the old WASM binary.

The web MCP's `force_rebuild` tool fixes this by calling `dx build --platform web`
directly, bypassing the file-watcher entirely and forcing a fresh compile.

The Electron MCP already uses `dx build` directly (no `dx serve`), so it doesn't
have this issue. But if you ever need to trigger a guaranteed-fresh build for Electron,
use `force_rebuild` the same way.

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

## Key Differences from `web-devtools-mcp`

| Feature | `web-devtools-mcp` | `electron-devtools-mcp` |
|---|---|---|
| CDP port | `9222` | `9224` |
| Build command | `dx serve` (hot-watching) | `dx build` (one-shot) |
| Browser | Chrome / Chromium | Electron |
| URL type | `http://localhost:3000` | `file://…/dist/index.html` |
| Watchdog (restart on crash) | Yes | No |
| Hot-patch | No | No |
| Counter file | `…web-rebuild-counter` | `…electron-rebuild-counter` |

## Discovery Strategy

`discover_ws_url()` queries `http://127.0.0.1:9224/json`:

1. **1st preference:** page targets whose URL starts with `file://` or `app://` (our WASM app)
2. **2nd preference:** any `page`-type target (handles `about:blank` while loading)

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

NEVER add lint suppression attributes. Fix the code instead.  
**Exception:** `#[allow(clippy::unwrap_used)]` / `expect_used` inside `#[cfg(test)]` only.
