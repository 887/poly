# poly-desktop-devtools-mcp — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-12


---

## MCP Preference (IMPORTANT — Updated 2026-03-12)

> **Prefer MCP mode over CLI subcommands.**

The MCP server is the primary integration point:
- Proper error handling and timeout protection baked into all calls
- Integrated with VS Code's built-in Copilot agent workflow
- Single shared `DevtoolsBackend` instance (connects once, reuses connection)
- Non-blocking background builds — MCP returns ~600ms while `dx serve` continues compiling

### When to use CLI (rare)

CLI subcommands are available for testing or scripting when MCP server is not needed:

```bash
cargo run --bin poly-desktop-devtools-mcp -- status
cargo run --bin poly-desktop-devtools-mcp -- launch  # polls background build
cargo run --bin poly-desktop-devtools-mcp -- screenshot
cargo run --bin poly-desktop-devtools-mcp -- snapshot
cargo run --bin poly-desktop-devtools-mcp -- build-status
cargo run --bin poly-desktop-devtools-mcp -- build-log
cargo run --bin poly-desktop-devtools-mcp -- help
```

Default screenshot policy: **prefer inline screenshot output**. Use `--save ...` only when you explicitly need a file artifact on disk.

VS Code CLI tasks under **"CLI: desktop — *"** exist but are not recommended for regular development.

**Always use MCP mode** (VS Code MCP integration or explicit MCP server launch) for production workflows.

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

## Architecture (Updated 2026-03-28 — Web-Shell Mode)

### Web-Shell Mode (Default)

```
VS Code Copilot / MCP Client
    │ JSON-RPC stdio
    ▼
poly-desktop-devtools-mcp (this crate)
    │ HTTP requests to 127.0.0.1:9223
    ├── Runs in its own background process
    └── Survives shell kill/restart
    ▼
poly-desktop-web (apps/desktop-web/)
    ├── Thin Wry/tao window (stays alive across rebuilds)
    ├── HTTP eval-bridge on port 9223
    ├── Screenshots via WebKit2GTK snapshot API
    └── Loads WASM from dx serve on port 3002
         ▼
dx serve --platform web --port 3002  (in apps/desktop/)
    └── Compiles Poly as WASM, serves hot-reloading dev server
```

### Legacy Mode (POLY_DESKTOP_LEGACY=1)

```
poly-desktop-devtools-mcp
    │ HTTP requests to 127.0.0.1:9223
    ▼
poly-desktop-devtools (apps/desktop-devtools/)
    ├── Embedded axum HTTP server (port 9223)
    ├── Bridges HTTP → dioxus eval() via use_coroutine + mpsc channel
    └── Renders the Poly UI in a native Wry/WebKit webview
```

### Why HTTP, not Chrome CDP?

WebKit2GTK's inspector (port 9222 via `WEBKIT_INSPECTOR_SERVER`) uses a
**proprietary binary protocol**, NOT Chrome CDP. You cannot connect with
standard CDP/WebSocket libraries. The HTTP eval-bridge is the only reliable path.

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
screenshot {}              → PNG screenshot of desktop app (prefer inline output; save only when needed)
get_dom {}                 → HTML of current UI
js_eval { expression: "..." }  → evaluate JavaScript in the app
click { x: 100, y: 200 }   → simulate mouse click
type_text { text: "hello" } → simulate text input
kill_app {}                → kill ONLY the app, NOT the MCP
launch_app { workspace: "..." } → relaunch the app
reset_app {}               → kill app + wipe data + docs for setup wizard
```

## Screenshot Policy (2026-03-17)

For agent-driven UI verification, screenshots should be **inline-first**:

- MCP screenshot calls should normally be made without a file path so the image is shown directly in chat.
- CLI `screenshot` without `--save` is the preferred terminal example.
- Use saved files only for explicit archival evidence, reproducible file-path references, or when the user asks for a saved image.

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

### launch_app() — NON-BLOCKING build + launch

`launch_app` is **NON-BLOCKING** — it returns immediately (~1 s). The actual
`dx build --platform desktop` runs in the background (takes 30-90 s). You **must** poll
`get_last_build_status` to know when it finishes.

Background sequence:
1. Kill any existing app instance (pkill MCP-safe pattern)
2. Record build state as `Running` and **return immediately** to the caller
3. Run **`dx build --platform desktop`** in `apps/desktop-devtools/` (background)
4. Launch the binary from `target/dx/poly-desktop-devtools/debug/linux/app/poly-desktop-devtools`
5. Spawn a background log reader for app output
6. Wait up to 30 s for the HTTP eval bridge on port 9223, then record `Succeeded` or `Failed`

**Workflow after calling `launch_app`:**
```
launch_app {}                     # returns in ~1 s
get_last_build_status {}          # repeat every 5-10 s until state != "Running"
  state = "Running"  → keep polling
  state = "Succeeded" → call connect_cdp {}
  state = "Failed"   → call get_last_build_log {} to see the error
```

Do **NOT** call `connect_cdp` immediately after `launch_app` — the build may still be in progress.


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

## Build Approach — `dx build` non-blocking (DECISION, 2026-03-12)

**All desktop builds use `dx build --platform desktop`, spawned in a background tokio task.**

- `launch_app`: kills old process, records `state=Running`, **returns immediately**, spawns
  background task: `dx build` → launch binary → wait for bridge (up to 30 s) → `state=Succeeded|Failed`
- `rebuild_app`: delegates to `launch_app` (same non-blocking pattern)
- `reset_app`: wipes the data directory, then calls `launch_app` (same non-blocking pattern)

**Why non-blocking?**
- `dx build --platform desktop` can take 30-90 s — long enough to timeout VS Code's MCP client
- Background tasks prevent connection drops while still capturing full stdout/stderr
- Agent polls `get_last_build_status` until `state` transitions: `Running → Succeeded | Failed`
- Exit code + build log are still captured and available via `get_last_build_log`

**Why `dx build` instead of `dx serve --hotpatch`:**
- No background dx serve process to manage or pkill
- Simpler lifecycle: one binary PID to track
- No stale port / pkill-regex edge cases

After `rebuild_app` completes (`state=Succeeded`): call `connect_cdp` to verify the bridge.

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

Backend behavior update (2026-03-17): when connect/screenshot/JS-eval runs and the real Poly
app root `#main` is already present, the desktop MCP now auto-hides the transient `#__dx-toast`
overlay before inspection. This reduces screenshot/snapshot noise, but build counters and real
app markers remain the source of truth.

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
