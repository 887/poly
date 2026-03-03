# desktop-devtools — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-03 (Build-id counter file + rebuild detection best practices)

---

## Purpose

`desktop-devtools` is a **special build of the Poly desktop app** with an
embedded HTTP eval-bridge server on port 9223. It renders the full Poly UI
(setup wizard, main layout, etc.) in a Wry/WebKit webview while exposing
inspection endpoints that the MCP server (`poly-desktop-devtools-mcp`) calls.

**This is NOT an MCP server itself** — it is the app being inspected.
The MCP server lives in `crates/poly-desktop-devtools-mcp/`.

---

## MCP and App Isolation (IMPORTANT)

**As of 2026-03-01:** The MCP and the desktop app are now **decoupled**.

- The MCP runs in its **own background process** with dedicated terminal
- Calling `kill_app()` from the MCP **does NOT kill the MCP itself**
- This allows hot-reload: kill + relaunch the app while MCP stays connected

**Pattern:** Use pattern `"poly-desktop-devtools[^-]"` in pkill to match the
app binary but exclude the `-mcp` variant. This ensures only the UI app dies,
not the MCP server.

---

## Architecture

```
poly-desktop-devtools
    ├── Dioxus desktop app (Wry/WebKit webview)
    │   ├── poly-core::ui::App — the actual Poly UI
    │   └── DevtoolsShell — wrapper that runs the eval + screenshot coroutines
    │
    ├── Embedded axum HTTP server (127.0.0.1:9223)
    │   ├── GET  /status     — "ok" health check
    │   ├── POST /eval       — evaluate JS via dioxus eval() bridge
    │   ├── GET  /screenshot — PNG via webkit2gtk::WebViewExt::snapshot()
    │   ├── GET  /dom        — document.documentElement.outerHTML
    │   ├── GET  /console    — buffered console messages (JSON array)
    │   └── GET  /generation — {generation, build_id, pid} for rebuild detection
    │
    └── Injected <script> (DEVTOOLS_HEAD)
        └── Console capture: window.__polyLogs[] (intercepts console.*)

poly-desktop-devtools-mcp (separate process)
    │ Runs in its own background terminal
    │ Communicates via HTTP to port 9223
    └── Survives app kill/restart cycles
```

## Screenshot Implementation

**Uses `webkit2gtk::WebViewExt::snapshot()`** — the native WebKit capture API.

**IMPORTANT: GDK pixbuf does NOT work** for WebKit content. GDK captures the
window chrome, but WebKit renders via GPU acceleration — the captured area is
blank/white. Always use the webkit2gtk snapshot approach.

Pattern:
1. `SCREENSHOT_TX/RX` channels — same pattern as eval bridge
2. `DevtoolsShell` runs `use_coroutine` that holds the webkit2gtk `WebView` handle
3. Callback-based API: `wv.snapshot(region, options, cancellable, callback)`
4. Callback fires on GLib main thread with `cairo::Surface`
5. `Surface::write_to_png()` encodes to PNG bytes in a `Vec<u8>`
6. Results polled via `std::sync::mpsc` + 16ms sleep ticks (yields to GLib loop)

Dependencies needed:
- `webkit2gtk = "2.0"` — snapshot API + gio + glib
- `cairo-rs = { version = "0.18", features = ["png"] }` — PNG encoding
- `wry = "0.53"` — `WebViewExtUnix::webview()` to get the underlying `webkit2gtk::WebView`

Correct imports (webkit2gtk 2.0 has no `prelude` module):
```rust
use webkit2gtk::WebViewExt as _;  // brings snapshot() into scope
// cairo::Surface — from the cairo-rs crate (imported as `cairo`)
// webkit2gtk::SnapshotRegion, SnapshotOptions — re-exported from crate root
// webkit2gtk::gio::Cancellable::NONE — for optional cancellable parameter
```

## Eval Bridge Pattern (Hot-Reload Safe)

The eval bridge uses a **recreatable channel** pattern so it survives `dx serve --hotpatch`:

1. `EVAL_TX` / `SCREENSHOT_TX` — `std::sync::Mutex<Option<mpsc::Sender>>` (NOT `OnceLock`)
2. On each coroutine start (including after hot-patch remount), fresh `mpsc` channels are
   created and the sender is stored in the global mutex
3. Each `EvalRequest` contains `js: String` + `oneshot::Sender<Result<String, String>>`
4. The coroutine calls `eval(&req.js).await` and sends the result back
5. HTTP handlers call `do_eval(js)` which clones the current sender from the mutex
6. HTTP server binds :9223 once (guarded by `AtomicBool`), survives component remounts

**Why not `OnceLock`?** `OnceLock` can only be set once per process. If Dioxus hot-patches
the component tree and remounts `DevtoolsShell`, the old receiver is dropped but `OnceLock`
prevents creating new channels — the eval bridge becomes permanently dead.

### Dioxus eval() Semantics

Scripts are wrapped as: `(new AsyncFunction("dioxus", SCRIPT))(dioxus)`

This means:
- Bare expressions like `document.title` do NOT return a value
- Must use `return document.title` explicitly
- `do_eval()` auto-prefixes `return` and strips trailing `;` for convenience

## `/generation` Endpoint — Rebuild Detection

`GET /generation` returns a JSON object with three fields for detecting rebuild cycles:

```json
{ "generation": 1, "build_id": 3, "pid": 2890763 }
```

**All three fields are always included in every response** — they're returned together in one JSON object.

| Field | Reset condition | Increments on |
|---|---|---|
| `generation` | Process restart (→1) | `DevtoolsShell` component FULLY unmounts + remounts (rare under hotpatch — see below) |
| `build_id` | System reboot / file deleted | Every `rebuild_app` MCP call — the MCP writes `/tmp/poly-devtools-rebuild-counter` |
| `pid` | Never resets (OS assigns) | Process restart only |

### ⭐ **ALWAYS USE `build_id` TO DETECT REBUILDS — Check All Three Together**

**`build_id` is the universal rebuild indicator.** To verify **nothing changed**, all three fields must be identical from the previous poll:

| `generation` | `build_id` | `pid` | Meaning |
|---|---|---|---|
| **Same** | **Same** | **Same** | ✅ No changes (no rebuild, no hot-patch, no process restart) |
| Changed | Same | Same | 🔨 Hot-patch occurred (window alive, component remounted — rare) |
| **Changed** | **Changed** | **Same** | 🔨 **Rebuild triggered** (most common case — window stayed alive) |
| Changed | Changed | Changed | 🔄 Full process restart |
| Any changed | **Any changed** | Any changed | ⚠️ **Something changed** — `build_id` specifically indicates rebuild |

**For visual/screenshot testing:** After each rebuild, verify that `build_id` increased from the previous poll.  
**Do NOT rely on `generation` alone** — it may not change even if a rebuild was triggered (component state preserved by hot-patch).

**Always check all three to verify no changes occurred** — if all three values match the previous poll, then nothing happened.

### Why NOT `generation` — Hot-Patch State Preservation

Under Dioxus 0.7 `--hotpatch`, `use_coroutine` hook state is **preserved** because
subsecond patches function bodies in-place — the component scope is NOT dropped. This means:

- `GENERATION` atomically increments **only when `DevtoolsShell` fully unmounts + remounts**
- For RSX-only structural changes that Dioxus classifies as non-hot-reloadable, Dioxus
  reloads the component tree from scratch → `generation` increments to 2
- But in practice, **many structural changes skip a full unmount → generation stays at 1**
- **`generation` is unreliable for rebuild detection** — use `build_id` instead

### `build_id` — Counter File Approach

The counter file (`/tmp/poly-devtools-rebuild-counter`) is **incremented by the MCP** on
each `rebuild_app` call, and **read at runtime by the app** on each `/generation` request.

This is more reliable than the previously-attempted compile-time `env!("POLY_BUILD_TS")`
approach because:

1. `touch`-only file changes do NOT trigger `build.rs` reruns — cargo uses **content checksums**,
   not mtime. Only actual content changes trigger build script reruns.
2. The `rerun-if-changed` trigger file (`crates/core/src/lib.rs`) is touched by the MCP via
   `touch`, so cargo sees no content change → `build.rs` never reruns → `POLY_BUILD_TS` is
   always the same value.
3. Runtime file read has zero compile-time complexity and works immediately without any
   recompilation.

---

## Hotpatch Behaviour — Verified Findings (2026-03-03)

### What works ✅

- **RSX color/text changes**: Hot-patched instantly (< 1s) without any component remount
- **RSX structural changes** (adding/removing elements): Classified as "non-hot-reloadable"
  by `dx serve`; triggers a full cargo recompile + hotpatch. Window stays alive (same PID).
  The Dioxus toast "Your app is being rebuilt" appears during compilation.
- **Eval bridge**: Works correctly before, during (if rebuild finishes), and after hotpatch
- **Navigation via eval + click**: Works across all rebuild cycles

### What resets on hotpatch (non-hot-reloadable structural changes) ⚠️

- **Navigation state**: The app resets to the setup wizard (Welcome to Poly) after a
  structural RSX change + hotpatch. This is because Dioxus re-initialises the component tree.
- **Page-level state** (form inputs, etc.): Also reset

### Multiple dx serve instances accumulate — ALWAYS kill before relaunch

Each `launch_app` call starts a **new `dx serve` process**. If `kill_app` is not called
(or fails) before the next `launch_app`, multiple `dx serve` instances will accumulate and
ALL watch the same workspace directory. This causes rebuild conflicts and can prevent
hotpatching from working correctly (multiple instances compete to rebuild and hotpatch).

**Workaround:** At the start of each coding session, call `kill_app` (or `pkill -f "dx serve"`)
before `launch_app` to ensure a clean slate. The MCP's process tracking should handle this,
but if you see multiple `dx serve` processes in `ps aux`, kill the stale ones manually.

### `touch` vs content change for cargo (critical pitfall)

The MCP's `rebuild_app` calls `touch crates/core/src/lib.rs` to wake up dx serve's
file watcher. This works for triggering a cargo recompile (because dx serve sees the mtime
change and invokes cargo). **BUT** cargo itself uses content checksums (not mtime) internally
for `rerun-if-changed` build script triggers. This means:

- `touch lib.rs` → dx serve fires → cargo checks → `lib.rs` content unchanged → cargo reruns
  `poly-core` compile (because the package changed) but does NOT rerun `poly-desktop-devtools/build.rs`
  (because its tracked files' content didn't change)
- Only an actual content edit to `src/main.rs` or `build.rs` causes `build.rs` to rerun

This is why the compile-time `POLY_BUILD_TS` approach failed: the build script never reran.

---



**MUST use `dx build --platform desktop`** — NOT `cargo build`.

The `asset!()` macro in poly-core inserts a placeholder path. Only the dx
linker resolves it to the actual hashed filename. Running `cargo build` leaves
the placeholder intact → CSS never loads.

```bash
cd apps/desktop-devtools && dx build --platform desktop
```

Output: `target/dx/poly-desktop-devtools/debug/linux/app/poly-desktop-devtools`

## Key Files

| File | Purpose |
|---|---|
| `src/main.rs` | Entry point, eval bridge, HTTP server, DevtoolsShell component |
| `Cargo.toml` | Dependencies (dioxus desktop, axum, poly-core with demo feature) |
| `Dioxus.toml` | dx build config (1440×900, desktop platform) |

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.
