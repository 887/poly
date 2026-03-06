# poly-demo

Demo/mock messenger client for **Poly** (PolyGlot Messenger).

## Purpose

Implements the `ClientBackend` trait with hardcoded fake data. Used for UI development and testing without requiring real messenger accounts.

## WASM Plugin Support (2026-03-06)

Builds as **both** native and WASM Component Model plugin:

```sh
# Native (workspace default):
cargo build -p poly-demo

# WASM plugin:
cargo component build -p poly-demo --target wasm32-wasip2
# Output: target/wasm32-wasip1/debug/poly_demo.wasm (37MB debug)
```

The crate uses feature-gating (`native` feature default) to enable heavy deps (dioxus, tokio, futures) only for native builds. WASM builds depend only on `poly-client` and `wit-bindgen`.

## Features

- Generates demo servers, categories, channels (text/voice/video)
- Hardcoded demo users with avatars and presence states
- Hardcoded demo messages with varied content types
- Hardcoded demo friend lists, group chats, notifications
- Simulates real-time events (new messages, presence changes, typing)

## Usage

Enable in Poly settings as a "Demo Account" backend. Instantly populates the entire UI with realistic mock data.

## Testing

**26 E2E tests** verify the full `ClientBackend` interface through the WASM plugin host:

```sh
cargo test -p poly-plugin-loader-tests --features test-demo --test client_e2e -- --nocapture
```

## License

MIT / Apache-2.0
