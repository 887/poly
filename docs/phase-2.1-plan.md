# Phase 2.1 Plan — MCP DevTools Infrastructure

> **Status:** ✅ Complete  
> **Target Start:** During Phase 2 UI development  
> **Parent:** [Overall Plan](overall-plan.md)  
> **Depends On:** [Phase 2](phase-2-plan.md) (workspace structure, UI components)

---

## Overview

MCP (Model Context Protocol) devtools infrastructure for dogfooding Poly.
Two backends — desktop (HTTP eval-bridge) and web (Chrome CDP) — share a common
protocol crate and expose identical + backend-specific tools to GitHub Copilot
and other MCP-compatible AI clients.

### Architecture

```
┌──────────────────────────────────────────────────┐
│                  MCP Protocol                     │
│           (poly-devtools-protocol)                │
│   DevtoolsBackend trait + JSON-RPC main loop      │
└────────────┬─────────────────────┬───────────────┘
             │                     │
     ┌───────┴──────┐      ┌──────┴───────┐
     │  Desktop MCP │      │   Web MCP    │
     │ (poly-devtools│     │(web-devtools) │
     │   -mcp)       │     │              │
     └───────┬──────┘      └──────┬───────┘
             │ HTTP :9223         │ CDP WebSocket :9222
     ┌───────┴──────┐      ┌──────┴───────┐
     │  Desktop App │      │   Chrome/    │
     │ (desktop-    │      │  Chromium +  │
     │  devtools)   │      │  dx serve    │
     │ [eval bridge]│      │  [:8080]     │
     └──────────────┘      └──────────────┘
```

---

## 2.1.1 Desktop DevTools App (`apps/desktop-devtools`)

- [x] **2.1.1.1** Create desktop-devtools crate with Dioxus desktop + devtools head injection
- [x] **2.1.1.2** Implement embedded axum HTTP server (port 9223) with routes:
  - `/status` — health check
  - `/eval` (POST) — evaluate JS via dioxus `eval()` bridge
  - `/screenshot` (GET) — capture via SVG foreignObject → Canvas → PNG
  - `/dom` (GET) — return `document.documentElement.outerHTML`
  - `/console` (GET) — return buffered console.log/warn/error messages
- [x] **2.1.1.3** Implement eval-bridge pattern: `use_coroutine` + `mpsc::channel<EvalRequest>` + `OnceLock<Sender>`
- [x] **2.1.1.4** Inject `DEVTOOLS_HEAD` script: console capture (`__polyLogs[]`)
- [x] **2.1.1.5** Fix `do_eval()` to auto-prefix `return` for bare expressions (dioxus wraps in `async function(dioxus) { SCRIPT }`)
- [x] **2.1.1.6** Configure `Dioxus.toml` for desktop platform
- [x] **2.1.1.7** **CRITICAL: Use `dx build` not `cargo build`** — asset!() macro requires dx linker to substitute placeholder paths
- [x] **2.1.1.8** Replace SVG foreignObject screenshot with `webkit2gtk::WebViewExt::snapshot()` — real WebKit capture API, pixel-perfect results, saved to `devtools-screenshots/desktop-{ts}.png`
- [x] **2.1.1.9** Fix double-instance: `launch_app` kills existing `poly-desktop-devtools` process + 800ms sleep before spawning

## 2.1.2 Shared Protocol Crate (`crates/poly-devtools-protocol`)

- [x] **2.1.2.1** Create `DevtoolsBackend` async trait with standard methods:
  - Lifecycle: `launch_app()`, `kill_app()`, `connect()`
  - Inspection: `screenshot()`, `js_eval()`, `get_dom()`, `get_console()`
  - Interaction: `click()`, `type_text()`
  - Navigation: `reset_app()`, `navigate()`
  - Extension: `handle_extension_tool()`, `extension_tools()`
- [x] **2.1.2.2** Implement MCP JSON-RPC helpers: `text_result()`, `image_result()`, `mcp_response()`, `mcp_error()`, `parse_request()`
- [x] **2.1.2.3** Implement `standard_tool_list()` with all 12 base tools (including `reset_app`, `navigate`)
- [x] **2.1.2.4** Implement `dispatch_tool()` — routes tool calls to `DevtoolsBackend` methods
- [x] **2.1.2.5** Implement `run_mcp_loop()` — stdio JSON-RPC main loop with `initialize`, `tools/list`, `tools/call` handling

## 2.1.3 Desktop MCP Server (`crates/poly-devtools-mcp`)

- [x] **2.1.3.1** Implement `DesktopHttpBackend` — HTTP client to eval-bridge (port 9223)
- [x] **2.1.3.2** `launch_app()` uses `dx build --platform desktop` (not `cargo build`)
- [x] **2.1.3.3** `reset_app()` kills app + removes `~/.local/share/poly` data dir
- [x] **2.1.3.4** Refactor to use `poly-devtools-protocol::mcp::run_mcp_loop()`

## 2.1.4 Web DevTools MCP Server (`apps/web-devtools`)

- [x] **2.1.4.1** Create web-devtools crate with Chrome CDP backend
- [x] **2.1.4.2** `launch_app()` starts `dx serve` + launches Chrome with `--remote-debugging-port=9222`
- [x] **2.1.4.3** `connect()` discovers WebSocket URL via `GET http://localhost:9222/json`, opens WebSocket
- [x] **2.1.4.4** `screenshot()` uses `Page.captureScreenshot` (real pixel-perfect PNG)
- [x] **2.1.4.5** `js_eval()` uses `Runtime.evaluate` with `awaitPromise: true`
- [x] **2.1.4.6** `click()` uses `Input.dispatchMouseEvent` (mousePressed + mouseReleased)
- [x] **2.1.4.7** `type_text()` uses `Input.insertText`
- [x] **2.1.4.8** `reset_app()` clears localStorage/sessionStorage/IndexedDB + `Page.reload`
- [x] **2.1.4.9** Extension tools: `page_reload`, `set_viewport`

## 2.1.5 VSCode Integration

- [x] **2.1.5.1** `.vscode/mcp.json` with both `poly-devtools` (desktop) and `poly-devtools-web` server entries

## 2.1.6 CSS Fix

- [x] **2.1.6.1** Root cause: `asset!()` macro placeholder not substituted with `cargo build`
- [x] **2.1.6.2** Fix: use `dx build --platform desktop` for desktop-devtools
- [x] **2.1.6.3** Verified: `getComputedStyle(document.body).backgroundColor` → `rgb(26, 26, 46)` (dark theme ✓)
- [x] **2.1.6.4** Verified: stylesheet href → `tailwind-dxh4edbe6aa97264b0.css` (hashed ✓)

---

## Key Decisions

| # | Decision | Rationale |
|---|---|---|
| DX-MCP-1 | HTTP eval-bridge for desktop, real CDP for web | WebKit2GTK's inspector uses a proprietary binary protocol, NOT Chrome CDP. HTTP bridge via dioxus `eval()` is the only reliable path. |
| DX-MCP-2 | Shared `DevtoolsBackend` trait | Avoid duplicating MCP protocol handling. Backends differ only in transport. |
| DX-MCP-3 | `dx build` required for desktop-devtools | The `asset!()` macro needs dx's linker to resolve; `cargo build` leaves a placeholder URL. |
| DX-MCP-4 | `webkit2gtk::WebViewExt::snapshot()` for desktop screenshots | GDK pixbuf captures window chrome only — WebKit renders GPU-accelerated, producing blank captures. `webkit2gtk::snapshot()` is WebKit's own native capture pipeline and produces pixel-perfect results. cairo::Surface → PNG via `cairo-rs`. |
| DX-MCP-6 | Screenshots saved to `devtools-screenshots/` dir + returned inline | Like Blender MCP — both disk persistence and inline base64 image response for VS Code chat history. Added to `.gitignore`. |
| DX-MCP-5 | `Page.captureScreenshot` for web screenshots | Real CDP provides pixel-perfect screenshots — superior to the SVG foreignObject approach. |

---

## File Map

| File | Purpose |
|---|---|
| `crates/poly-devtools-protocol/src/lib.rs` | Protocol crate entry point |
| `crates/poly-devtools-protocol/src/backend.rs` | `DevtoolsBackend` trait + types |
| `crates/poly-devtools-protocol/src/mcp.rs` | MCP JSON-RPC main loop + helpers |
| `crates/poly-devtools-mcp/src/main.rs` | Desktop MCP server (HTTP backend) |
| `apps/desktop-devtools/src/main.rs` | Desktop app with embedded HTTP eval-bridge |
| `apps/web-devtools/src/main.rs` | Web MCP server (Chrome CDP backend) |
| `.vscode/mcp.json` | MCP server configuration for VS Code |

---

## Session Log

### Session 2025-02-28
- Discovered WebKit2GTK inspector is NOT Chrome CDP (proprietary binary protocol)
- Pivoted to HTTP eval-bridge architecture — embedded axum server in desktop-devtools
- Got eval working, identified CSS root cause (need `dx build`)
- Built complete infrastructure: shared protocol crate, desktop MCP, web CDP MCP
- CSS confirmed working via eval: `rgb(26, 26, 46)` = dark theme background

### Session 2025-03-01 (continued) — Storage Abstraction
- **`crates/poly-core/src/storage/`** built: `mod.rs` (typed helpers + `Storage` newtype), `native.rs` (SurrealDB 3.0 + SurrealKV), `web.rs` (gloo-storage LocalStorage)
- **Cross-platform**: surrealdb gated to `cfg(not(wasm32))`, gloo-storage to `cfg(wasm32)` in `Cargo.toml`
- **Global `STORAGE: OnceLock<Storage>`** in `lib.rs`, initialized by `use_future` in `App`; wizard `on_complete` handler spawns async write
- **SurrealDB 3.0 pitfalls resolved** (see poly-core/agents.md for full notes):
  - Typed SDK (`db.upsert`, `db.select`) excluded — requires internal `#[derive(SurrealValue)]` not exposed downstream
  - Field `payload` (not `value`) — `VALUE` is a SurrealQL keyword causing silent failures
  - Bind via `serde_json::json!({ "payload": ... })` — `serde_json::Value: SurrealValue` satisfies `IntoVariables`
  - Turbofish required on `take::<Option<T>>()` — type inference fails through `map_err()?` chain
  - `take(0usize)` not `take(0)` — `{integer}: QueryResult<T>` ambiguity
- **MCP self-test PASSED**: wizard → kill → relaunch → wizard skipped (chat layout loaded directly) ✓
