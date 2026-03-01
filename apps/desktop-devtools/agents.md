# desktop-devtools — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-01

---

## Purpose

`desktop-devtools` is a **special build of the Poly desktop app** with an
embedded HTTP eval-bridge server on port 9223. It renders the full Poly UI
(setup wizard, main layout, etc.) in a Wry/WebKit webview while exposing
inspection endpoints that the MCP server (`poly-devtools-mcp`) calls.

**This is NOT an MCP server itself** — it is the app being inspected.
The MCP server lives in `crates/poly-devtools-mcp/`.

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
    │   └── GET  /console    — buffered console messages (JSON array)
    │
    └── Injected <script> (DEVTOOLS_HEAD)
        └── Console capture: window.__polyLogs[] (intercepts console.*)
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

## Eval Bridge Pattern

The key pattern for JS evaluation:

1. `EVAL_TX` / `EVAL_RX` — `OnceLock<mpsc::Sender/Receiver>` created before dioxus starts
2. `DevtoolsShell` component runs `use_coroutine` that reads from `EVAL_RX`
3. Each `EvalRequest` contains `js: String` + `oneshot::Sender<Result<String, String>>`
4. The coroutine calls `eval(&req.js).await` and sends the result back
5. HTTP handlers call `do_eval(js)` which sends through `EVAL_TX` and awaits the `oneshot`

### Dioxus eval() Semantics

Scripts are wrapped as: `(new AsyncFunction("dioxus", SCRIPT))(dioxus)`

This means:
- Bare expressions like `document.title` do NOT return a value
- Must use `return document.title` explicitly
- `do_eval()` auto-prefixes `return` and strips trailing `;` for convenience

## Build Requirements

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
