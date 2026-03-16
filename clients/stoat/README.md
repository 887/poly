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
- `src/api.rs` contains typed Stoat login/root-config/user/server/channel/message models and Poly mapping helpers
- `src/http.rs` provides isolated reqwest transport/session scaffolding using Stoat's `x-session-token` header
- `StoatAuthInput` extracts supported Stoat credential types from `poly-client::AuthCredentials`
- Native auth now supports email/password login, token resume, fetch-self mapping, and logout
- Native server/channel retrieval now supports:
	- `get_server(id)` via `GET /servers/{id}`
	- `get_channels(server_id)` via server channel IDs + `GET /channels/{id}` fanout
	- `get_channel(id)` for server channels
	- `/sync/unreads` enrichment for mention counts and conservative unread badges
- Native message retrieval now supports:
	- `get_messages(channel_id, query)` via `GET /channels/{target}/messages`
	- `before`, `after`, and `around`/`nearby` pagination windows
	- Stoat reply previews when the referenced message is present in the returned window
	- bundled user/member display-name mapping plus reaction state
	- best-effort Autumn attachment URLs using the instance `GET /` file-service config
- `get_servers()` is still intentionally unsupported because the published Stoat REST schema does not currently expose an obvious authenticated joined-server collection endpoint
- `clients/stoat/tests/integration.rs` provides mock-backed native end-to-end tests for the implemented auth slice
- `clients/stoat/tests/integration.rs` now also covers server detail, channel list/detail, unread mapping, DM-channel rejection, and both Stoat bulk-message response variants
- `src/guest.rs` still behaves as a stub for backend operations, but the WASM component build is wired correctly and now includes required plugin metadata exports
- The shared WIT/plugin boundary is now aligned with `poly-client::Session.backend_url` and `poly-client::Server.banner_url`

Send/edit/delete, DMs/groups, realtime, and voice/video implementation are still upcoming in Phase 3.1.

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

Native auth/integration coverage:

```sh
cargo test -p poly-stoat
```

Complete plugin-host contract coverage after the WIT update:

```sh
cargo test -p poly-plugin-loader-tests --all-features
```


## API Source

- Local OpenAPI snapshot: `api-1.json`
- Download source: `https://developers.stoat.chat/api-reference/`
- Protocol/feature spec: [`SPEC.md`](./SPEC.md)


## License

MIT / Apache-2.0
