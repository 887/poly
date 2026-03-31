# Desktop (Wry) — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.
> **Last Updated:** 2026-03-28


## Purpose

Primary desktop entry point using the **Wry** system webview renderer. This is the default, most stable desktop build.

## How It Works

- This is a thin wrapper: `main.rs` initializes the Dioxus desktop app and mounts `poly_core::App`
- All logic lives in `poly-core` — this crate just sets up the platform
- Uses system webview (WebKitGTK on Linux, WebKit on macOS, WebView2 on Windows)

## Web-Shell Mode (2026-03-28)

The `poly-desktop-devtools-mcp` now defaults to **web-shell mode**: it runs
`dx serve --platform web --port 3002` in this directory, compiling the app as WASM,
and loads it in `apps/desktop-web` (a thin Wry shell that stays alive across rebuilds).

### WASM Target Compatibility

The `Cargo.toml` uses cfg-gated dependencies so the same crate compiles for both
native desktop and wasm32:
- **Native:** `dioxus = ["desktop"]`, `tokio`, `tracing-subscriber`
- **WASM:** `dioxus = ["web"]`, `getrandom04-wasm`

The `main.rs` cfg-gates `tracing_subscriber` init and `install_wasm_crash_handler()`
based on `target_arch`.

## Native Development

```bash
dx serve --hotpatch          # Run with hot-reload (native Wry)
dx serve --platform desktop  # Explicit desktop platform
```

## Build

```bash
dx build --release --platform desktop  # Native release build
dx build --platform web                # WASM build (for web-shell mode)
```

## Configuration

- `Dioxus.toml` — platform: desktop (default), renderer: webview
- Window title, size, icon configured in Dioxus.toml
- Tokio multi-threaded runtime (native only)

## Supported OS

- Linux (WebKitGTK)
- macOS (WebKit)
- Windows (WebView2)

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.
