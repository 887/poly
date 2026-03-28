# Desktop Web Shell — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.
> **Last Updated:** 2026-03-28

## Purpose

Thin native Wry shell for web-shell development mode. Launched automatically by
`poly-desktop-devtools-mcp` — you should never need to build or run this manually.

## How It Works

- Wry/tao opens a native window loading the Poly WASM app from `dx serve` on port 3002
- HTTP eval-bridge on port 9223 lets `poly-desktop-devtools-mcp` drive the app
- The shell binary **never recompiles** during development — only the WASM page reloads
- Screenshots use WebKit2GTK's native `snapshot()` API on the GTK main thread

## Key Implementation Details

- **GTK container:** Must use `window.default_vbox()` for `build_gtk()`, NOT `gtk_window()`.
  Using `gtk_window()` results in 0x0 viewport — the webview gets no size allocation.
- **Event loop:** Uses `tao::event_loop::ControlFlow::Wait` with `UserEvent` enum for
  eval requests, URL reloads, and screenshot wake-ups
- **Console buffer:** Injects JS that intercepts `console.log/warn/error/info/debug` and
  buffers last 200 entries for the `/console` endpoint

## Building

```bash
cargo build -p poly-desktop-web
```

The MCP handles launching this binary. To run manually for debugging:

```bash
POLY_DEV_URL=http://127.0.0.1:3002 cargo run -p poly-desktop-web
```

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.
