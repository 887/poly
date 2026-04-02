# poly-matrix — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.
> **Last Updated:** 2026-04-01


---

## Purpose

`poly-matrix` implements the `ClientBackend` trait for **Matrix** protocol using
the Matrix client-server HTTP API directly. **No `matrix-sdk` dependency** — all
Matrix protocol logic is implemented in this crate, same pattern as `poly-stoat`.

## WASM Plugin Architecture (DECISION D21, 2026-03-06)

This crate builds as **both** a native Rust library AND a WASM Component Model plugin.

- **Crate type**: `["cdylib", "rlib"]`
- **Feature gate**: `native` feature (default) enables reqwest, tokio, serde, async-trait, futures, dioxus
- **WASM guest**: `src/guest.rs` — currently a **stub** returning errors/empty results. Must be completed with direct Matrix HTTP calls using `host_api::http_request()`.
- **cfg pattern**: `#[cfg(feature = "native")]` for native code, `#[cfg(target_os = "wasi")]` for WASI plugin code. **NEVER** use `target_arch = "wasm32"`.

### Why No matrix-sdk

1. `matrix-sdk` depends on `reqwest` + `tokio` which cannot compile to `wasm32-wasip2`
2. `matrix-sdk::ClientBuilder::http_client()` only accepts `reqwest::Client` — no pluggable HTTP trait
3. Using matrix-sdk on the host side breaks plugin updatability (updating Matrix logic would require a new host binary, not a new `.wasm` file)
4. We only need the subset of Matrix protocol that Poly uses — implementing directly keeps us in full control

### Building

```sh
# Native (default, part of workspace):
cargo build -p poly-matrix

# WASM plugin:
cargo component build -p poly-matrix --target wasm32-wasip2

# Native tests:
cargo test -p poly-matrix
```

### Key Files

| File | Purpose |
|---|---|
| `src/lib.rs` | Native `MatrixClient` + `ClientBackend` impl, cfg-gated behind `feature = "native"` |
| `src/config.rs` | Homeserver URL normalization, auth input parsing |
| `src/http.rs` | Native reqwest HTTP transport + session management |
| `src/api.rs` | Matrix client-server API types (login, sync, rooms, messages, etc.) |
| `src/guest.rs` | WIT guest — stub now, will use `host_api::http_request()` for all I/O |
| `src/wit_bindings.rs` | WIT code generation wrapper |
| `locales/*/plugin.ftl` | Fluent translation files (en, de, fr, es) |

### guest.rs Notes

- `#![allow(unsafe_code)]` — required for wit-bindgen FFI
- All methods currently return `Err(ClientError::Internal("not yet implemented"))` or empty collections
- `get_backend_type()` returns `BackendType::Matrix`, `get_backend_name()` returns `"Matrix"`
- When implementing real client, guest must call `wit::host_api::http_request()` for all Matrix HTTP calls
- Because `wit_bindgen::generate!` lives in `src/wit_bindings.rs`, the export must use:
	`export!(MatrixPlugin with_types_in crate::wit_bindings)`
- The `messenger-plugin` world also requires a minimal `plugin_metadata::Guest` implementation

## Implementation Phase

**Phase 3.2** — Second real backend. See [Phase 3.2 Plan](../../docs/phase-3.2-matrix-plan.md).

## Technology

- **Protocol**: Matrix client-server API over HTTPS + `/sync` long-poll
- **Auth**: Username/password (`m.login.password`), SSO (`m.login.token`), access token
- **E2EE**: vodozemac (pure Rust Olm/Megolm — compiles to WASM, no networking deps)
- **Storage**: Plugin uses `host_api::storage_get/set()` WIT imports; native uses reqwest sessions
- **Federation**: Any Matrix homeserver (matrix.org default)

## Matrix Concepts → Poly Mapping

| Matrix Concept | Poly Type | Notes |
|---|---|---|
| Space | `Server` | A Space organizes rooms into a hierarchy |
| Room | `Channel` | Rooms are channels (text by default) |
| Space child rooms | Channels in categories | Spaces can nest rooms in sub-spaces (categories) |
| User | `User` | Matrix user ID: @user:homeserver.tld |
| DM (2-person room) | `DmChannel` | `m.direct` account data |
| Multi-person room | `Group` | Rooms with 3+ members that aren't in a Space |
| VoIP events | Voice/Video | m.call.* events for WebRTC signaling |

### "Fake Servers" Feature
For Matrix rooms NOT in any Space, Poly lets users create custom groupings:
- User creates a "fake server" (named group)
- Drags rooms into it, creating categories
- Stored locally via crates/core storage layer (SQLite by default)
- Displayed exactly like regular servers in the sidebar

## Module Structure

```
src/
├── lib.rs              # MatrixClient struct + ClientBackend impl (native-only)
├── config.rs           # Homeserver URL normalization, auth input parsing (native-only)
├── http.rs             # reqwest transport + session management (native-only)
├── api.rs              # Matrix CS API types: login, sync, rooms, messages (native-only)
├── guest.rs            # WIT guest bridge (WASI-only)
├── wit_bindings.rs     # wit-bindgen code generation (WASI-only)
├── auth.rs             # Login flows (TODO)
├── sync.rs             # Sync loop management, event mapping (TODO)
├── rooms.rs            # Room → Channel/Server/DM mapping (TODO)
├── spaces.rs           # Space → Server mapping + fake servers (TODO)
├── messages.rs         # Message send/receive/history (TODO)
├── users.rs            # User profiles, presence, friends (TODO)
└── voip.rs             # VoIP signaling for voice/video (TODO)
```

## E2E Test Coverage (2026-04-01)

**10 tests** in `crates/plugin-host-tests/tests/client_e2e/matrix.rs` — stub behavior verification through WASM plugin host:

- Backend identity (type=Matrix, name="Matrix")
- `authenticate()` returns `Err(Internal("not yet implemented"))`
- `is_authenticated()` returns false
- All list methods return empty `Ok(vec![])`
- `get_server()` / `get_channel()` return `Err(NotFound(...))`
- `set_presence()` returns `Ok(())`
- Event stream returns valid (empty) stream
- `logout()` returns error (stub)

```sh
cargo test -p poly-plugin-loader-tests --features test-matrix --test client_e2e -- --nocapture
```

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

**Additional exception for `guest.rs`**: `#![allow(unsafe_code)]` is required for wit-bindgen FFI.

See root `agents.md` § 7a for the full rationale.
