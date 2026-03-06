# poly-stoat — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-06

---

## Purpose

`poly-stoat` implements the `ClientBackend` trait for **Stoat** (formerly Revolt) messenger. Supports both the official Stoat server and self-hosted instances.

## WASM Plugin Architecture (DECISION D21, 2026-03-06)

This crate builds as **both** a native Rust library AND a WASM Component Model plugin.

- **Crate type**: `["cdylib", "rlib"]`
- **Feature gate**: `native` feature (default) enables reqwest, tokio-tungstenite, serde, async-trait, tokio, futures
- **WASM guest**: `src/guest.rs` — currently a **stub** returning errors/empty results. Must be completed when the native implementation is done.
- **cfg pattern**: `#[cfg(feature = "native")]` for native code, `#[cfg(target_os = "wasi")]` for WASI plugin code. **NEVER** use `target_arch = "wasm32"`.

### Building

```sh
# Native (default, part of workspace):
cargo build -p poly-stoat

# WASM plugin:
cargo component build -p poly-stoat --target wasm32-wasip2
# Output: target/wasm32-wasip1/debug/poly_stoat.wasm (~4.3MB debug)
```

### Key Files

| File | Purpose |
|---|---|
| `src/lib.rs` | Native `StoatClient` stub, cfg-gated behind `feature = "native"` |
| `src/guest.rs` | WIT guest stub — returns errors for all operations, reports `BackendType::Stoat` |
| `Cargo.toml` | Dual crate-type, feature-gated deps, WASI wit-bindgen dep |

### guest.rs Notes

- `#![allow(unsafe_code)]` — required for wit-bindgen FFI
- All methods return `Err(ClientError::Internal("not yet implemented"))` or empty collections
- `get_backend_type()` returns `BackendType::Stoat`, `get_backend_name()` returns `"Stoat"`
- When implementing the real client, the guest bridge must convert between native types and WIT types

## Implementation Phase

**Phase 3.1** — First real backend to implement. See [Phase 3 Plan](../../docs/phase-3-plan.md) section 3.1.

## Technology

- **Protocol**: REST API + WebSocket for real-time events
- **API Documentation**: https://developers.stoat.chat
- **Auth**: Email/password login → session token
- **Self-hosted**: Configurable base URL (different Stoat/Revolt instances)
- **Voice/Video**: WebRTC-based (Stoat's Vortex voice server)

## Research Notes (Phase 1)

### API Overview
- Stoat (Revolt) rebranded in 2025. API docs at `developers.stoat.chat`.
- The backend is written in Rust, but there is NO official Rust client SDK.
- Existing Rust crates (`revolt-rs`, `rive`) are unmaintained (2+ years old).
- We are building this client from scratch using the REST/WebSocket API.

### Key API Areas
- **Auth**: `POST /auth/session/login` — email/password → token
- **Servers**: `GET /servers/{id}`, server members, roles
- **Channels**: `GET /channels/{id}`, messages, typing indicators
- **Messages**: `GET/POST/PATCH/DELETE` on channel messages
- **Users**: `GET /users/{id}`, relationships (friends)
- **WebSocket**: `wss://ws.stoat.chat` — Bonfire real-time protocol
- **Voice**: Vortex voice server (WebRTC with SDP exchange)

### Type Mapping
| Stoat Concept | Poly Type |
|---|---|
| Server | `Server` |
| Channel (Text/Voice) | `Channel` |
| Category | `Category` |
| User | `User` |
| Group (DM with multiple users) | `Group` |
| Direct Message | `DmChannel` |

### No Existing Rust SDK
Must build from scratch:
1. HTTP client (reqwest) for REST API
2. WebSocket client (tokio-tungstenite) for real-time events
3. Type definitions matching Stoat API schemas
4. Auth flow management
5. WebRTC integration for voice/video (Vortex protocol)

## Dependencies

### Native (default feature)
- `poly-client` — trait to implement
- `reqwest` — HTTP client
- `tokio-tungstenite` — WebSocket
- `serde`, `serde_json` — API type (de)serialization
- `url` — base URL management
- `async-trait`, `tokio`, `futures` — async runtime
- `webrtc` — voice/video (Phase 3.1)

### WASM (target_os = "wasi" only)
- `poly-client` — type definitions only
- `wit-bindgen` — WIT code generation

## Module Structure

```
src/
├── lib.rs           # StoatClient struct + ClientBackend impl (native-only)
├── guest.rs         # WIT guest bridge (WASI-only, stub)
├── api/             # REST API client (TODO)
├── ws/              # WebSocket event handling (TODO)
├── types/           # Stoat-specific type definitions (TODO)
└── voice/           # WebRTC voice/video (TODO)
```

## E2E Test Coverage (2026-03-06)

**10 tests** in `crates/plugin-host-tests/tests/client_e2e/stoat.rs` — stub behavior verification through WASM plugin host:

- Backend identity (type=Stoat, name="Stoat")
- `authenticate()` returns `Err(Internal("not yet implemented"))`
- `is_authenticated()` returns false
- All list methods return empty `Ok(vec![])`
- `get_server()` / `get_channel()` return `Err(NotFound(...))`
- `set_presence()`, `logout()` return `Ok(())`
- Event stream returns valid (empty) stream

```sh
cargo test -p poly-plugin-loader-tests --features test-stoat --test client_e2e -- --nocapture
```

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

**Additional exception for `guest.rs`**: `#![allow(unsafe_code)]` is required for wit-bindgen FFI.

See root `agents.md` § 7a for the full rationale.
