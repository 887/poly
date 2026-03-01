# Desktop (Wry) — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

## Purpose

Primary desktop entry point using the **Wry** system webview renderer. This is the default, most stable desktop build.

## How It Works

- This is a thin wrapper: `main.rs` initializes the Dioxus desktop app and mounts `poly_core::App`
- All logic lives in `poly-core` — this crate just sets up the platform
- Uses system webview (WebKitGTK on Linux, WebKit on macOS, WebView2 on Windows)

## Development

```bash
dx serve --hotpatch          # Run with hot-reload
dx serve --platform desktop  # Explicit desktop platform
```

## Build

```bash
dx build --release --platform desktop  # Release build
```

## Configuration

- `Dioxus.toml` — platform: desktop, renderer: webview (default)
- Window title, size, icon configured in Dioxus.toml
- Tokio multi-threaded runtime

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
