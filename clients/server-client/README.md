# poly-server-client

HTTP + WebSocket client implementation for connecting to a poly-server instance.

This crate is **native-only** and provides everything needed for the UI layer or
other Rust code to talk to a running poly-server (the Rust server in
`/servers/server`). It is used by `poly-core` via the `poly-client` trait but
can also be used standalone by applications that know they only ever speak to
poly-server.

## Components

- [`PolyServerHttpClient`] — typed REST API client built on `reqwest`
- [`PolyServerWsClient`] — real‑time event client built on `tokio-tungstenite`
- [`PolyServerBackend`] — implements `poly_client::ClientBackend` by wrapping the
  HTTP/WS clients and mapping wire types
- `models` — serde types mirroring the server's JSON payloads (wire protocol)
- `error` — error definitions for the client libraries

## Features & Platform

- No `wasm32` support; depends on native `reqwest` and `tokio-tungstenite`.
- Requires `tokio` runtime.
- Hot‑reloading isn’t needed here; the crate is fairly small.

## Tests

Integration tests spin up an in-process `poly-server` instance (see
`tests/integration.rs`). Run with:

```sh
cargo test -p poly-server-client -- --test-threads=1
```

The suite exercises signup/signin, server/channel CRUD, messaging, friend
requests, invites, and WebSocket events.

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
