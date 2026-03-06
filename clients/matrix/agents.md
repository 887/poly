# poly-matrix — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-06

---

## Purpose

`poly-matrix` implements the `ClientBackend` trait for **Matrix** protocol using the `matrix-sdk` Rust crate.

## WASM Plugin Architecture (DECISION D21, 2026-03-06)

This crate builds as **both** a native Rust library AND a WASM Component Model plugin.

- **Crate type**: `["cdylib", "rlib"]`
- **Feature gate**: `native` feature (default) enables matrix-sdk, tokio, serde, async-trait, futures
- **WASM guest**: `src/guest.rs` — currently a **stub** returning errors/empty results. Must be completed when the native implementation is done.
- **cfg pattern**: `#[cfg(feature = "native")]` for native code, `#[cfg(target_os = "wasi")]` for WASI plugin code. **NEVER** use `target_arch = "wasm32"`.

### Building

```sh
# Native (default, part of workspace):
cargo build -p poly-matrix

# WASM plugin:
cargo component build -p poly-matrix --target wasm32-wasip2
# Output: target/wasm32-wasip1/debug/poly_matrix.wasm (~4.3MB debug)
```

### Key Files

| File | Purpose |
|---|---|
| `src/lib.rs` | Native `MatrixClient` stub, cfg-gated behind `feature = "native"` |
| `src/guest.rs` | WIT guest stub — returns errors for all operations, reports `BackendType::Matrix` |
| `Cargo.toml` | Dual crate-type, feature-gated deps, WASI wit-bindgen dep |

### guest.rs Notes

- `#![allow(unsafe_code)]` — required for wit-bindgen FFI
- All methods return `Err(ClientError::Internal("not yet implemented"))` or empty collections
- `get_backend_type()` returns `BackendType::Matrix`, `get_backend_name()` returns `"Matrix"`
- When implementing the real client, the guest bridge must convert between native types and WIT types

## Implementation Phase

**Phase 3.2** — Second real backend. See [Phase 3 Plan](../../docs/phase-3-plan.md) section 3.2.

## Technology

- **SDK**: `matrix-sdk` (production-grade, powers Element X)
- **Protocol**: Matrix client-server API over HTTPS + sync
- **E2EE**: Olm/Megolm via `matrix-sdk-crypto` (Vodozemac implementation)
- **Storage**: `matrix-sdk-sqlite` for session/crypto state (or integrate with our SurrealKV)
- **Auth**: Username/password, SSO (OIDC), token-based
- **Federation**: Any Matrix homeserver (matrix.org default)

## Research Notes (Phase 1)

### Matrix Concepts → Poly Mapping

| Matrix Concept | Poly Type | Notes |
|---|---|---|
| Space | `Server` | A Space organizes rooms into a hierarchy |
| Room | `Channel` | Rooms are channels (text by default) |
| Space child rooms | Channels in categories | Spaces can nest rooms in sub-spaces (categories) |
| User | `User` | Matrix user ID: @user:homeserver.tld |
| DM (2-person room) | `DmChannel` | |
| Multi-person room | `Group` | Rooms with 3+ members that aren't in a Space |
| VoIP events | Voice/Video | m.call.* events for WebRTC signaling |

### "Fake Servers" Feature
For Matrix rooms NOT in any Space, Poly lets users create custom groupings:
- User creates a "fake server" (named group)
- Drags rooms into it, creating categories
- Stored locally in SurrealKV, not on the Matrix server
- Displayed exactly like regular servers in the sidebar

### matrix-sdk Architecture
- `matrix_sdk::Client` — main client object
- `Client::sync()` — sync loop for real-time updates  
- `Room` type — represents a room (joined, invited, left)
- `Room::messages()` — paginated message history
- `Room::send()` — send messages
- `Room::typing_notice()` — typing indicators
- `RoomListService` — high-level room list management
- `Encryption` — automatic E2EE handling

### Key matrix-sdk Features
- Automatic E2EE (opt-in per room)
- Cross-signing and device verification (QR code, emoji)
- Lazy-loading room members
- Push notification rules
- SSO / OIDC authentication
- WASM support (for web target)
- SQLite or IndexedDB (web) storage

### Public Server Directory
- matrix.org is the default/largest homeserver
- `matrix.to` links for room/user discovery
- Room directory API: `GET /_matrix/client/v3/publicRooms` per homeserver
- Can fetch public rooms from any federated server

## Dependencies

### Native (default feature)
- `poly-client` — trait to implement
- `matrix-sdk` — official Matrix Rust SDK
- `matrix-sdk-sqlite` — storage backend (or custom SurrealKV adapter)
- `tokio` — async runtime
- `async-trait`, `futures` — async support

### WASM (target_os = "wasi" only)
- `poly-client` — type definitions only
- `wit-bindgen` — WIT code generation

## Module Structure

```
src/
├── lib.rs              # MatrixClient struct + ClientBackend impl (native-only)
├── guest.rs            # WIT guest bridge (WASI-only, stub)
├── auth.rs             # Login flows (password, SSO, token) (TODO)
├── sync.rs             # Sync loop management, event mapping (TODO)
├── rooms.rs            # Room → Channel/Server/DM mapping (TODO)
├── spaces.rs           # Space → Server mapping + fake servers (TODO)
├── messages.rs         # Message send/receive/history (TODO)
├── users.rs            # User profiles, presence, friends (TODO)
├── encryption.rs       # E2EE setup, device verification (TODO)
├── voip.rs             # VoIP signaling for voice/video (TODO)
└── directory.rs        # Public room/server directory browsing (TODO)
```

## E2E Test Coverage (2026-03-06)

**10 tests** in `crates/plugin-host-tests/tests/client_e2e/matrix.rs` — stub behavior verification through WASM plugin host:

- Backend identity (type=Matrix, name="Matrix")
- `authenticate()` returns `Err(Internal("not yet implemented"))`
- `is_authenticated()` returns false
- All list methods return empty `Ok(vec![])`
- `get_server()` / `get_channel()` return `Err(NotFound(...))`
- `set_presence()`, `logout()` return `Ok(())`
- Event stream returns valid (empty) stream

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
