# apps/web

**Poly** web app entry point using [Dioxus](https://dioxuslabs.com/) compiled to WebAssembly.

## Purpose

Delivers `poly-core`'s UI as a WASM web application. Can be served as a static site (client-side only) or extended to a fullstack app backed by Axum. This target is also the foundation for `apps/desktop-electron`.

## How It Works

1. Dioxus compiles `poly-core` to WASM via `wasm-bindgen`
2. The WASM module boots in the browser and mounts into `<div id="main">`
3. All state, i18n, and theming run entirely client-side inside the WASM module
4. SurrealDB uses SurrealKV (IndexedDB-backed) for local persistence in the browser

## Building & Serving

### ⚠️ CRITICAL: Use the Web MCP for Devtools

**For testing with Copilot/devtools**, use the web MCP (`poly-web-devtools-mcp`), not manual `dx serve`:

```bash
# In VS Code: Run task "Serve: web (MCP)"
# Or terminate any manual dx serve first, then:
cargo run --bin poly-web-devtools-mcp
```

The MCP automatically manages:
- `dx serve --platform web --port 3000` (no `--hotpatch`)
- Chromium with remote debugging
- Auto-restart on crash

**Do NOT use `--hotpatch` for web/WASM** on Dioxus 0.7.3 — it can leave the browser stuck in a rebuild loop.

### Manual Development (Optional)

```bash
# Standard hot-reload (without hotpatch, on port 3000)
dx serve --platform web --port 3000

# Production WASM bundle
dx build --release --platform web
```

The built output lands in `target/dx/poly-web/release/web/` and can be served from any static host.

## Key Dependencies

| Crate | Role |
|---|---|
| `poly-core` | All UI, state, DB, i18n, theming |
| `dioxus` (`web` feature) | WASM DOM renderer |

## License

MIT / Apache-2.0
