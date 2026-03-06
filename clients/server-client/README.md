# poly-server-client

HTTP + WebSocket client implementation for connecting to a poly-server instance.

This crate provides everything needed for the UI layer or other Rust code to talk to a running poly-server (the Rust server in `/servers/server`). It is used by `poly-core` via the `poly-client` trait but can also be used standalone.

## WASM Plugin Support (2026-03-06) — ⚠️ CRITICAL BUILD DIFFERENCE

Builds as **both** native and WASM Component Model plugin, but behaves very differently:

```sh
# Native (workspace default):
cargo build -p poly-server-client

# WASM plugin (MUST use --no-default-features):
cargo component build -p poly-server-client --target wasm32-wasip2 --no-default-features
# Output: target/wasm32-wasip1/debug/poly_server_client.wasm (4.2MB debug)
```

**⚠️ CRITICAL**: Unlike all other client crates, this one **requires `--no-default-features`** when building as WASM because `tokio-tungstenite` depends on `native-tls` which links against OpenSSL and cannot cross-compile to WebAssembly.

Feature-gated (`native` feature default). Currently a **stub** — WIT guest implementation in `src/guest.rs` returns errors for all operations. Full implementation requires routing HTTP calls through host-api imports.

## Components

- [`PolyServerHttpClient`] — typed REST API client built on `reqwest`
- [`PolyServerWsClient`] — real‑time event client built on `tokio-tungstenite`
- [`PolyServerBackend`] — implements `poly_client::ClientBackend` by wrapping the
  HTTP/WS clients and mapping wire types
- `models` — serde types mirroring the server's JSON payloads (wire protocol)
- `error` — error definitions for the client libraries

## Features & Platform

- Native (default): Full `reqwest` + `tokio-tungstenite` support
- WASM (`--no-default-features`): Stub implementation with error returns
- Requires `tokio` runtime (native only)
- Hot‑reloading isn't needed here; the crate is fairly small

## Tests

Integration tests spin up an in-process `poly-server` instance (see
`tests/integration.rs`). Run with:

```sh
cargo test -p poly-server-client -- --test-threads=1
```

The suite exercises signup/signin, server/channel CRUD, messaging, friend
requests, invites, and WebSocket events.

**E2E plugin tests** (10 tests verifying WASM stub behavior):

```sh
cargo test -p poly-plugin-loader-tests --features test-server --test client_e2e -- --nocapture
```

### Dev experience

`poly-core` and `poly-client` re‑use this crate under the hood; changes here
require re-running the VS Code "Check: poly-web (WASM)" task to catch any
cross-crate errors.

## Dependencies

- `reqwest` (json, multipart) — REST client
- `tokio-tungstenite` — WebSocket support
- `ed25519-dalek` + `hex` — challenge-response auth
- `serde`, `serde_json` — serialization
- `tokio` (rt, macros, sync, time)
- `futures` / `futures-util` / `tokio-stream` — async utilities
- `chrono` — timestamps
- `async-trait` — trait object async methods
- `thiserror` — error definitions
- `tracing` — logging

## License

MIT / Apache-2.0
