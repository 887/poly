# poly-stoat

Stoat (formerly Revolt) messenger client for **Poly** (PolyGlot Messenger).

## Purpose

Implements the `ClientBackend` trait for Stoat/Revolt messenger. Supports both the official server and self-hosted instances.

## WASM Plugin Support (2026-03-06)

Builds as **both** native and WASM Component Model plugin:

```sh
# Native (workspace default):
cargo build -p poly-stoat

# WASM plugin:
cargo component build -p poly-stoat --target wasm32-wasip2
# Output: target/wasm32-wasip1/debug/poly_stoat.wasm (4.3MB debug)
```

Feature-gated (`native` feature default).

## Current Status (2026-03-16)

The crate is no longer just an empty shell:

- `src/config.rs` implements Stoat-specific base URL normalization and derives:
	- REST base URL
	- Bonfire websocket URL
	- route-safe `instance_id`
- `src/http.rs` provides isolated reqwest transport/session scaffolding using Stoat's `x-session-token` header
- `StoatAuthInput` extracts supported Stoat credential types from `poly-client::AuthCredentials`
- `src/guest.rs` still behaves as a stub for backend operations, but the WASM component build is wired correctly and now includes required plugin metadata exports

Full auth/server/channel/message implementation is still upcoming in Phase 3.1.

## Features

- Email/password authentication
- Server browsing with categories and channels
- Text messaging (send, receive, edit, delete)
- Voice and video calling (WebRTC via Vortex)
- Real-time events via WebSocket
- Friend management and DMs
- Group chats
- Self-hosted instance support (configurable base URL)

## Implementation

Built from scratch using the Stoat REST API + WebSocket protocol. No existing Rust SDK — this is a custom implementation.

- API docs: https://developers.stoat.chat
- REST API for CRUD operations
- WebSocket (Bonfire) for real-time events
- WebRTC (Vortex) for voice/video

## Testing

**10 E2E tests** verify stub behavior through the WASM plugin host:

```sh
cargo test -p poly-plugin-loader-tests --features test-stoat --test client_e2e -- --nocapture
```


## API Source

the file api-1.json
downloaded from
https://developers.stoat.chat/api-reference/


## License

MIT / Apache-2.0
