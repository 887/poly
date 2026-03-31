# poly-plugin-host — Agent Instructions

> **Read this before working on this crate.**
> **Last Updated:** 2025-06-03


---

## Priority 2 — Use Jujutsu (jj) Instead of Git

- **Always use `jj` commands** for version control, never raw `git`
- `jj status`, `jj diff`, `jj log`, `jj show` for inspection
- `jj new`, `jj describe`, `jj commit` for creating changes
- `jj git push` to push to remote
- Only fall back to `git` if `jj` cannot accomplish the task

---

---

## Purpose

`poly-plugin-host` is the **dynamically-linked WASM Component Model plugin host** for Poly.

It isolates the very heavy `wasmtime` runtime into a shared library (`.so`/`.dll`/`.dylib`)
so that changes to `poly-core` NEVER trigger a wasmtime recompilation. Only changes to
this crate itself cause the wasmtime rebuild (~2min saved per iteration on poly-core).

**DECISION(D22):** Dynamic linking boundary for wasmtime isolation.

---

## Architecture

```
poly-plugin-host (dylib: .so / .dll / .dylib)
├── engine.rs      — wasmtime Engine + WIT bindgen! (path: "../../wit")
├── host_impl.rs   — PluginHostState: implements host-api imports
├── bridge.rs      — WIT ↔ poly-client type conversions
└── registry.rs    — PluginRegistry + PluginBackend (ClientBackend impl)
```

### Key Types

| Type | Module | Description |
|---|---|---|
| `PluginRegistry` | `registry` | Loads/manages WASM plugin components |
| `PluginBackend` | `registry` | A loaded plugin that implements `ClientBackend` |
| `PluginHostState` | `host_impl` | Per-instance host state (storage, WS handles, WASI) |
| `MessengerPlugin` | `engine` | Generated bindings from `wit/messenger.wit` |

### How poly-core Uses This

```rust
// In poly-core lib.rs (cfg-gated for native-only):
#[cfg(not(target_arch = "wasm32"))]
pub use poly_plugin_host as plugin_host;
```

poly-core re-exports this crate as `plugin_host` so the rest of the app
sees `poly_core::plugin_host::PluginRegistry` etc. transparently.

---

## MANDATORY RULES

### 1. crate-type = ["dylib"]

This MUST remain a Rust `dylib`. Do NOT change to `cdylib` (C ABI),
`rlib` (static), or `lib` (default). The whole point is dynamic linking.

### 2. WIT Path

The `bindgen!` macro in `engine.rs` uses `path: "../../wit"`.
This works because the crate lives at `crates/plugin-host/` — two levels
below the workspace root where `wit/` lives.

If this crate is ever moved, the path MUST be updated.

### 3. No WASM32 Target

This crate is **native-only**. It uses `wasmtime` which cannot compile to
`wasm32-unknown-unknown`. The cfg gate is applied in poly-core's re-export.

### 4. Fuel Budget

Every guest call must be refueled via `store.set_fuel(1_000_000_000)`.
The `refuel()` helper in `registry.rs` does this. Never forget to refuel
between consecutive guest calls or the plugin will trap.

### 5. No Tests in This Crate

Tests live in the separate `poly-plugin-loader-tests` crate to avoid
pulling test deps into the dylib. The test crate depends on this crate.

---

## Dependencies

| Dep | Why |
|---|---|
| `wasmtime` + `wasmtime-wasi` | WASM runtime + WASI p2 support |
| `poly-client` | Shared types + `ClientBackend` trait |
| `reqwest` | Host-API `http_request` implementation |
| `tokio-tungstenite` | Host-API WebSocket implementation |
| `chrono` | Host-API `get_current_time` |
| `tracing` | Host-API `log` + internal diagnostics |
| `async-trait` + `futures` | `ClientBackend` trait requirements |
| `tokio-stream` | `ReceiverStream` for `event_stream()` |

---

## Platform Strategy

| Platform | Behavior |
|---|---|
| Desktop (Wry/Blitz) | ✅ dylib loaded at runtime |
| Mobile (Android/iOS) | ✅ dylib loaded at runtime |
| Web client (WASM) | ❌ Not included (browser IS the runtime) |
| Web fullstack server | ✅ Linked natively (server-side Axum) |

---

## Session Log

- **2026-03-06** — Created crate. Extracted from `crates/core/src/plugin_host/`. DECISION(D22).
- **2026-03-06** — Companion test crate `poly-plugin-loader-tests` now has 77 tests: 1 integration (load all 6 plugins) + 76 E2E client interface tests (26 demo + 50 stub). See `crates/plugin-host-tests/agents.md`.
