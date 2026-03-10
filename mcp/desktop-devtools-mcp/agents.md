# poly-desktop-devtools-mcp — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-11

---

## CLI Preference (IMPORTANT — Updated 2026-03-10)

> **Prefer CLI over MCP whenever possible.**

All devtools functionality is available as CLI subcommands — no JSON-RPC overhead:

```bash
cargo run --bin poly-desktop-devtools-mcp -- status
cargo run --bin poly-desktop-devtools-mcp -- launch
cargo run --bin poly-desktop-devtools-mcp -- screenshot --save devtools-screenshots/snap.png
cargo run --bin poly-desktop-devtools-mcp -- snapshot
cargo run --bin poly-desktop-devtools-mcp -- eval "document.title"
cargo run --bin poly-desktop-devtools-mcp -- click "#my-button"
cargo run --bin poly-desktop-devtools-mcp -- fill "#input" "value"
cargo run --bin poly-desktop-devtools-mcp -- generation
cargo run --bin poly-desktop-devtools-mcp -- build-status
cargo run --bin poly-desktop-devtools-mcp -- build-log
cargo run --bin poly-desktop-devtools-mcp -- help
```

VS Code CLI tasks are available under **"CLI: desktop — *"** in `.vscode/tasks.json`.

Use MCP mode (via `.vscode/mcp.json`) only when orchestrating multi-step sequences through Copilot agent mode.

---

## Purpose

`poly-desktop-devtools-mcp` is the **desktop MCP server** for Poly. It implements the
`DevtoolsBackend` trait from `poly-devtools-protocol` using an HTTP eval-bridge
to communicate with the running `poly-desktop-devtools` app on port 9223.

This is how you verify UI changes in the **desktop (Wry/WebKit)** build.

For the **web (Chrome)** build, use `poly-web-devtools-mcp` instead.

## Timeout Behaviour (2026-03-10)

The shared MCP protocol now wraps every desktop tool call in a timeout budget.
Desktop transport already had HTTP client timeouts; now the outer MCP request will also fail fast instead of hanging forever if the eval bridge or app stops responding.

Treat timeout errors as a strong signal that the app or bridge is wedged.

---

## Critical: App and MCP Isolation (2026-03-01)

**The MCP and desktop app are now isolated.** Killing the app **does NOT kill the MCP.**

- `kill_app()` uses pkill pattern `"poly-desktop-devtools($|[^-])"` to match only the app,
  excluding the `-mcp` variant
- The MCP survives app kill/restart cycles
- Enables hot-reload development: rebuild + kill + relaunch app without MCP downtime

**Pattern explained:**
- `poly-desktop-devtools($|[^-])` ← matches either the exact app path/name end or a following non-dash character
- `poly-desktop-devtools-mcp` ← does NOT match (protected)
- This avoids the old bug where a bare `/path/to/poly-desktop-devtools` process survived because there was no trailing character after the app name.

---

## Architecture

```
VS Code Copilot / MCP Client
    │ JSON-RPC stdio
    ▼
poly-desktop-devtools-mcp (this crate)
    │ HTTP requests to 127.0.0.1:9223
    ├── Runs in its own background process (VSCode task)
    └── Survives app kill/restart
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

### 1. Build & Run the MCP First (in its own terminal)

```
cargo run -p poly-desktop-devtools-mcp
```

Or use the VSCode task:
```
Run: desktop-devtools-mcp (protected)
```

The MCP listens on stdin for JSON-RPC and waits for commands.

### 2. (elsewhere) Build & Launch the Desktop App

```
cd apps/desktop-devtools && dx build --platform desktop
target/dx/poly-desktop-devtools/debug/linux/app/poly-desktop-devtools
```

Or use the VSCode task:
```
Build: desktop-devtools
```

Wait ~3 seconds for the app HTTP server to start.

### 3. Connect from MCP (via Copilot or direct call)

```
connect_cdp {}
```

Verifies the HTTP eval-bridge at `http://127.0.0.1:9223/status` is reachable.

### 4. Use Devtools Functions

All functions now work with the MCP and app isolated:

```
screenshot {}              → PNG screenshot of desktop app
get_dom {}                 → HTML of current UI
js_eval { expression: "..." }  → evaluate JavaScript in the app
click { x: 100, y: 200 }   → simulate mouse click
type_text { text: "hello" } → simulate text input
kill_app {}                → kill ONLY the app, NOT the MCP
launch_app { workspace: "..." } → relaunch the app
reset_app {}               → kill app + wipe data + docs for setup wizard
```

---

## Implementation Details

### kill_app() — MCP-Safe Pattern

```rust
// Uses pattern that matches the exact app name/path but NOT the MCP server
tokio::process::Command::new("pkill")
    .args(["-f", "poly-desktop-devtools($|[^-])"])
    .status()
    .await?;
```

This pattern:
- `poly-desktop-devtools($|[^-])` — match either the exact executable path end or a following non-dash
- Will match: `/path/to/poly-desktop-devtools` (the app)
- Will NOT match: `poly-desktop-devtools-mcp` (has a dash after)

If `launch_app` times out and `get_last_build_status` mentions port 9223 still being occupied while `/status` is dead,
assume a stale desktop app blocked the bridge bind instead of assuming the build itself is broken.

### launch_app() — Builds and Launches

1. Kill any existing app instance (pkill MCP-safe pattern)
2. Run **`dx build --platform desktop`** in `apps/desktop-devtools/` — **blocks synchronously** until the build finishes  
   - Exit code gives **immediate pass/fail** — no polling, no ambiguity  
   - Stdout/stderr captured and available via `get_last_build_log`
3. Launch the binary from `target/dx/poly-desktop-devtools/debug/linux/app/poly-desktop-devtools`
4. Spawn a background log reader for app output
5. Wait up to 30 s for the HTTP eval bridge on port 9223 to become reachable

Call `connect_cdp()` after launch_app returns to verify the bridge is ready.


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

## Build Approach — `dx build` everywhere (DECISION, 2026-03-11)

**All desktop rebuilds use `dx build --platform desktop` — never `dx serve --hotpatch`.**

- `launch_app`: runs `dx build --platform desktop` (blocking, sync) then launches the binary
- `rebuild_app`: calls `launch_app` (kills old binary, rebuilds, relaunches)
- `force_rebuild`: delegates to `rebuild_app`

**Why `dx build` instead of `dx serve --hotpatch`:**
- Exit code is immediate — any build failure is surfaced at once with exact Cargo error
- No background dx serve process to manage or pkill
- Simpler lifecycle: one binary PID to track, not a dx serve process + binary PID
- No stale port / pkill-regex edge cases

After `rebuild_app` or `force_rebuild` completes, call `connect_cdp` to verify the bridge.

## Debugging CSS Not Loading

If the app looks unstyled (white/transparent background):

```javascript
js_eval { expression: "document.querySelector('link[rel=stylesheet]').href" }
```

If the href contains "This should be replaced by dx", the app was built with
`cargo build` instead of `dx build`. Re-build with dx.

## Rebuild Detection — Extension Tools (2026-03-03)

Two extension tools help detect rebuilds and hot-patches:

### `get_generation()`

Returns a JSON object with three counters: `{generation, build_id, pid}`.

**All three fields are always included in the response:**

```json
{ "generation": 1, "build_id": 3, "pid": 2890763 }
```

| Field | Meaning |
|---|---|
| `generation` | Starts at 1 on launch. **Increments on each hot-patch (component remount).** Resets to 1 only on full process restart (PID change). |
| `build_id` | **⭐ PRIMARY INDICATOR**: Increments on each `rebuild_app` call (reads `/tmp/poly-devtools-rebuild-counter` at runtime). 0 = no rebuild this session. |
| `pid` | OS process ID. Stable across hot-patches; changes only on full restart. |

### ⭐ **ALWAYS USE `build_id` TO DETECT REBUILDS**

**`build_id` is the universal, platform-independent way to know if a rebuild happened.**

For visual/screenshot testing: after each `rebuild_app()`, check `build_id` increased.
Do NOT rely on `generation` — it may not change if hot-patch succeeded (hot-patches preserve state).

## Build Diagnostics — REQUIRED when generation is ambiguous (2026-03-10)

The desktop MCP now captures Dioxus CLI output and exposes two new tools/CLI commands:

- `get_last_build_status` / `build-status`
- `get_last_build_log` / `build-log`

Use them immediately when:
- `get_generation()` does not change as expected
- `build_id` changed but the UI did not update
- the eval bridge never came back after `rebuild_app`
- `force_rebuild` succeeds/fails and you need the exact Dioxus output

`get_last_build_status` is the fast structured summary.
`get_last_build_log` is the raw stdout/stderr transcript from the most recent desktop Dioxus build/hotpatch attempt.

### ⭐ Complete Decision Table — Check All Three Together

To verify nothing changed, all three must be identical from the previous poll:

| `generation` | `build_id` | `pid` | Meaning |
|---|---|---|---|
| **Same** | **Same** | **Same** | ✅ No changes (no rebuild, no hot-patch, no process restart) |
| Changed | Same | Same | 🔨 Hot-patch occurred (window alive, component remounted — rare) |
| **Changed** | **Changed** | **Same** | 🔨 **Rebuild triggered** (most common case — window stayed alive) |
| Changed | Changed | Changed | 🔄 Full process restart |
| Any changed | **Any changed** | Any changed | ⚠️ **Something rebuilding** — check `build_id` to confirm |

**Key insight:** `build_id` is the universal rebuild indicator. Even if `generation`/`pid` stay the same, if `build_id` changed, a rebuild was triggered.  

### Counter File

`/tmp/poly-devtools-rebuild-counter` — plain text U64, incremented by `rebuild_app()`.

**Important:** Web MCP uses a separate counter file `/tmp/poly-devtools-web-rebuild-counter` to avoid cross-contamination when both MCPs run simultaneously.

### Platform Difference

**Desktop `generation`** may NOT change on every rebuild (hot-patches preserve state).
Always check **`build_id`** to know if a rebuild happened.

**Web `generation`** increments on EVERY `connect_cdp` call (because WASM rebuilds drop the CDP WebSocket).

**In both cases, `build_id` is the reliable indicator** of "did a rebuild happen?"

## Dioxus Rebuild Toast Warning (2026-03-08)

Dioxus dev-runtime may show a visible toast/overlay like **"Your app is being rebuilt"**.

This text is **not** a reliable readiness or failure signal for agents:

- it is injected by the Dioxus dev runtime, not by Poly
- it may still be visible in a screenshot even though the app underneath already updated
- it does **not** prove the rebuild is stuck

For MCP automation and visual verification:

1. Check `get_generation()` and compare **all three** fields
2. Use `build_id` as the primary rebuild indicator
3. Take a fresh snapshot/screenshot after the bridge is reachable again
4. Verify real Poly UI markers instead of the rebuild-toast text
5. **Note:** The toast DOM element may persist in the snapshot/screenshot even after a successful
   rebuild — its presence does not prove the app is stuck

**Avoid:** `wait_for(["Your app is being rebuilt"])`

**Prefer:** account list, setup wizard text, channel title, settings headings, composer placeholder,
or other route-specific content that proves the intended screen is actually ready.

See the tool descriptions in `src/main.rs:extension_tools()` for full cross-platform semantics.

---

| File | Purpose |
|---|---|
| `src/main.rs` | `DesktopHttpBackend` impl + entry point |
| `Cargo.toml` | Dependencies (uses poly-devtools-protocol) |

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.
