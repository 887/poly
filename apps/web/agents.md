# Web — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

## Purpose

Web entry point for Poly. Uses **Dioxus fullstack** — WASM frontend + Axum backend, all in one binary.

## How It Works

- `main.rs` initializes Dioxus fullstack and mounts `poly_core::App`
- Frontend compiles to WASM, served by Axum
- Can use Dioxus server functions for any server-side operations
- All shared logic lives in `poly-core`
- SurrealDB in web target: may need `kv-mem` or IndexedDB adapter instead of SurrealKV (investigate)

## Development

```bash
dx serve --platform web  # Run fullstack with hot-reload
```

## Build

```bash
dx build --release --platform web  # Production build (WASM + server binary)
```

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
