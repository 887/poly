# poly-stoat — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-16

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
- Because `wit_bindgen::generate!` lives in `src/wit_bindings.rs`, the guest export must use:
	`export!(StoatPlugin with_types_in crate::wit_bindings)`
	rather than plain `export!(StoatPlugin)`.
- The `messenger-plugin` world also requires `plugin_metadata::Guest`; even stub plugins must implement
	`get_translations`, `get_settings_schema`, `get_display_name_key`, and `get_icon`.

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
├── config.rs        # Base URL normalization + credential extraction
├── http.rs          # reqwest transport/session scaffolding
├── guest.rs         # WIT guest bridge (WASI-only, stub)
├── api/             # REST API client (TODO)
├── ws/              # WebSocket event handling (TODO)
├── types/           # Stoat-specific type definitions (TODO)
└── voice/           # WebRTC voice/video (TODO)
```

## Current Implementation Status (2026-03-16)

Completed the first Phase 3.1 native isolation slice:

- `src/config.rs` now owns Stoat-specific base URL normalization and derives:
	- REST root
	- Bonfire websocket URL
	- route-safe `instance_id`
- `StoatAuthInput` now cleanly extracts only supported Stoat credential shapes from `poly-client::AuthCredentials`
- `src/http.rs` now owns the isolated reqwest transport and `x-session-token` request scaffolding
- `StoatClient` exposes transport metadata helpers without leaking Stoat protocol logic into app crates
- WASM component build now succeeds again after fixing the guest export syntax and adding required plugin metadata stubs

Completed the second Phase 3.1 auth slice:

- `src/api.rs` now contains typed Stoat API models for:
	- `RevoltConfig`
	- login request / response
	- user + presence mapping
- `StoatClient::authenticate()` now supports:
	- `AuthCredentials::EmailPassword`
	- `AuthCredentials::Token`
- Native auth uses:
	- `POST /auth/session/login`
	- `GET /users/@me`
	- `POST /auth/session/logout`
- `Focus` and `Busy` currently map to `PresenceStatus::DoNotDisturb` because Poly does not yet expose Stoat's exact focus-mode semantics.
- Native end-to-end-style coverage lives in `clients/stoat/tests/integration.rs` with a mock Stoat HTTP server.
- Full Stoat protocol/feature reference now lives in `clients/stoat/SPEC.md`.

Completed the third Phase 3.1 retrieval slice:

- `src/api.rs` now contains typed models and Poly mapping helpers for:
	- servers
	- categories
	- channels
	- `/sync/unreads` payloads
- Native retrieval now supports:
	- `GET /servers/{id}` → `get_server(id)`
	- `GET /channels/{id}` → `get_channel(id)`
	- `GET /servers/{id}` + per-channel fetch fanout → `get_channels(server_id)`
	- `GET /sync/unreads` → mention counts + conservative unread badges
- Important protocol finding: the published Stoat REST schema still does **not** show an obvious joined-server collection endpoint for the authenticated account.
	- `get_servers()` therefore remains explicit `NotSupported(...)` until Bonfire ready-state / sync caching is wired or a dedicated REST endpoint is confirmed.
- `get_channel(id)` intentionally rejects DM/group channels because Poly models those through `DmChannel` / `Group`, not shared server-channel `Channel`.
- Native integration coverage now also includes:
	- server detail mapping
	- channel list retrieval
	- single-channel retrieval
	- unread/mention enrichment
	- DM-channel rejection

Completed the fourth Phase 3.1 retrieval slice:

- `src/api.rs` now also contains typed models and mapping helpers for:
	- `GET /channels/{target}/messages`
	- Stoat `BulkMessageResponse` array/envelope variants
	- bundled message users / members / webhook metadata
	- file-service (`Autumn`) URL derivation from `GET /`
- Native retrieval now also supports:
	- `get_messages(channel_id, query)`
	- `before`, `after`, and `around`/`nearby` pagination windows
	- reply preview hydration when the referenced message is in the current batch
	- reaction mapping with `me` detection from the authenticated user id
	- chronologically sorted Poly messages using timestamps derived from Stoat ULID message IDs
- The shared WIT/plugin ABI was also realigned in this slice:
	- `wit/messenger-plugin.wit` `session` now includes `backend-url`
	- `wit/messenger-plugin.wit` `server` now includes `banner-url`
	- `crates/plugin-host/src/bridge.rs` and `clients/demo/src/guest.rs` were updated to preserve those fields across the WIT boundary
- Native integration coverage now additionally includes:
	- expanded-envelope message fetches with bundled users/members
	- plain-array `BulkMessageResponse` handling
	- attachment URL mapping
	- reply preview mapping
	- reaction mapping

Completed the fifth Phase 3.1 send slice:

- Native message sending now supports:
	- `send_message(channel_id, MessageContent::Text(...))`
	- `send_reply_message(channel_id, reply_to_message_id, MessageContent::Text(...))`
	- reply preview hydration by fetching the referenced Stoat message and mapping it into Poly's `MessageReplyPreview`
- Stoat send requests currently use `POST /channels/{target}/messages` with:
	- `content`
	- generated `nonce`
	- optional `replies: [{ id, mention: false, fail_if_not_exists: true }]`
- Attachment upload is **still pending**.
	- `MessageContent::WithAttachments` currently returns `ClientError::NotSupported("Stoat attachment upload is not implemented yet")`
	- this is intentional until the upload/attachment-id lifecycle is implemented against Stoat's file APIs
- Native test coverage now additionally includes:
	- verifying the outbound JSON payload for text sends
	- verifying reply intent payloads for reply sends
	- verifying reply preview hydration on the returned Poly message
	- verifying the current explicit attachment-upload `NotSupported` behavior

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

Additional native transport E2E-style tests (2026-03-16):

```sh
cargo test -p poly-stoat
```

These cover the implemented auth slice end-to-end over a mock HTTP server:
- root config fetch
- email/password login
- token restore
- MFA error branch
- disabled-account error branch
- logout

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

**Additional exception for `guest.rs`**: `#![allow(unsafe_code)]` is required for wit-bindgen FFI.

See root `agents.md` § 7a for the full rationale.
