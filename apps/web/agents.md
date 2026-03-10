# Web — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-07

## Purpose

Web entry point for Poly. Uses **Dioxus** compiled to WebAssembly (WASM).

## How It Works

- `main.rs` initializes Dioxus web and mounts `poly_core::App`
- `main.rs` must call `poly_core::install_wasm_crash_handler()` before `dioxus::launch(App)`
- Frontend compiles to WASM via `wasm-bindgen`, served by Axum dev server
- All shared logic lives in `poly-core`
- SurrealDB uses SurrealKV with IndexedDB backend for browser-side persistence

## WASM Crash Visibility (2026-03-10)

The web app now installs a shared browser crash handler before launch.

If the app hits a Rust panic, `window.onerror`, or `window.unhandledrejection`:

- crash metadata is stored on `window.__polyCrashState`
- a DOM overlay `#poly-wasm-crash-overlay` is injected

When debugging route freezes, inspect `window.__polyCrashState` via devtools/MCP if the page is still responsive.
If MCP methods start returning timeout errors, treat that as evidence that the renderer or CDP path is wedged — not as "no result yet".

## Development & Testing

### ✅ Recommended: Use the Web MCP

```bash
# VS Code: Run task "Serve: web (MCP + Chromium)"
# Or:
cargo run --bin poly-web-devtools-mcp
```

The MCP automatically manages:
- `dx serve --platform web --port 3000` (no `--hotpatch`)
- Chromium with remote debugging (CDP port 9222)
- Auto-restart on crash
- Stale process cleanup

### ⚠️ Manual Development (If Needed)

**Do NOT use `--hotpatch`** — Dioxus 0.7.3 WASM support is incomplete.

```bash
# Standard hot-reload (correct)
dx serve --platform web --port 3000

# Wrong — breaks the rebuild system
dx serve --hotpatch  # DO NOT USE FOR WEB
```

## Troubleshooting

Common issues?  See `docs/web-devtools-setup.md` for:
- Browser stuck on "Your app is being rebuilt" → kill hotpatch
- Chrome CDP connection failures
- Port conflicts (3000, 8080, 9222)
- Stale process cleanup

Run: `./scripts/web-cleanup.sh` if stuck

## Configuration

- `Dioxus.toml` — platform: web, fullstack
- WASM bundle splitting enabled (Dioxus 0.7.3 feature) for smaller initial load
- TailwindCSS auto-detected

## Web-Specific Notes

- WebRTC is native to browsers — voice/video should work well here
- IndexedDB for browser-side storage (if SurrealKV doesn't compile to WASM)
- Service worker for offline support (future)
- PWA manifest for installable web app (future)

## Known Concerns

- SurrealDB WASM compilation — SurrealKV may not work in WASM; may need `kv-mem` or remote WebSocket mode
- If SurrealKV doesn't compile: use SurrealDB remote mode connecting to a local server, or in-memory with remote backup
- WASM binary size — use code splitting and lazy loading (Dioxus 0.7.3)

## WASM Compatibility Check

The standard `cargo build --workspace` / `cargo cranky --workspace` only compiles for the host target.
WASM-specific breakage is invisible until the web build is attempted.

**After any change to `poly-core` or `poly-web`, run:**

```bash
cargo check -p poly-web --target wasm32-unknown-unknown
```

Or use the VS Code task **"Check: poly-web (WASM)"** — errors appear in the Problems panel.
This is a `check` (no link step), so it's fast. The task does NOT run automatically on folder open.

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.
