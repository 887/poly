# poly-matrix

Matrix protocol client for **Poly** (PolyGlot Messenger).

## Purpose

Implements the `ClientBackend` trait for Matrix using the official `matrix-sdk` Rust crate (the same SDK that powers Element X).

## WASM Plugin Support (2026-03-06)

Builds as **both** native and WASM Component Model plugin:

```sh
# Native (workspace default):
cargo build -p poly-matrix

# WASM plugin:
cargo component build -p poly-matrix --target wasm32-wasip2
# Output: target/wasm32-wasip1/debug/poly_matrix.wasm (4.3MB debug)
```

Feature-gated (`native` feature default). Currently a **stub** — WIT guest implementation in `src/guest.rs` returns errors for all operations. Full implementation coming in Phase 3.2.

## Features

- Username/password and SSO authentication
- Matrix Spaces displayed as servers (with room hierarchies as categories)
- Rooms displayed as channels
- End-to-end encryption (Olm/Megolm)
- Device verification (QR code, emoji)
- Voice and video calls (Matrix VoIP + WebRTC)
- Federation — works with any Matrix homeserver
- Public room directory browsing
- "Fake servers" — user-created local groupings for rooms not in Spaces
- DMs and multi-user group chats

## Key Dependency

- `matrix-sdk = "0.16.0"` — production-grade Matrix Rust SDK

## Testing

**10 E2E tests** verify stub behavior through the WASM plugin host:

```sh
cargo test -p poly-plugin-loader-tests --features test-matrix --test client_e2e -- --nocapture
```

## License

MIT / Apache-2.0
