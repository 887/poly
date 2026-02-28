# Desktop Blitz (WGPU Native) — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

## Purpose

Desktop entry point using the **Blitz** WGPU native renderer. This is Dioxus's experimental GPU-accelerated HTML/CSS renderer — no webview dependency.

## ⚠️ Experimental

Blitz is a work-in-progress. Not all CSS is supported. See https://blitz.is/status/css for current support.

## How It Works

- Same thin wrapper pattern as desktop-wry
- `main.rs` initializes Dioxus with the native/Blitz renderer and mounts `poly_core::App`
- All logic lives in `poly-core`

## Development

```bash
dx serve --hotpatch --renderer native  # Run Blitz with hot-reload
```

## Build

```bash
dx build --release --renderer native  # Release build with Blitz
```

## Configuration

- `Dioxus.toml` — platform: desktop, renderer: native
- May need different CSS adjustments for Blitz vs webview

## Known Limitations

- Some CSS features not supported (check blitz.is/status/css)
- May render differently from webview version
- Accessibility support still developing
