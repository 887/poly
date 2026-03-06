# poly-client

Shared messenger client protocol for **Poly** (PolyGlot Messenger).

## Purpose

Defines the `ClientBackend` trait that all messenger backends must implement, plus shared data types for servers, channels, messages, users, and events.

This crate is the **contract** between `poly-core` (the UI/app logic) and the backend implementations (`poly-stoat`, `poly-matrix`, `poly-discord`, `poly-teams`, `poly-demo`, `poly-server-client`).

## WASM Plugin Architecture (2026-03-06)

**ALL backend clients compile to both native AND WASM Component Model plugins** (Decision D21).

This crate is the **only** Rust dependency for WASM plugin builds:
- Native backends: `cargo build -p poly-<backend>`
- WASM plugins: `cargo component build -p poly-<backend> --target wasm32-wasip2`

Types here are the **source of truth** for the WIT (WebAssembly Interface Types) definitions in `wit/messenger-plugin.wit`. Every type in this crate has a corresponding WIT type, and bridge code in `crates/plugin-host/src/bridge.rs` converts between them.

When you modify types here, you must also update:
1. `wit/messenger-plugin.wit` (WIT type definitions)
2. `crates/plugin-host/src/bridge.rs` (host-side conversions)
3. Every backend's `src/guest.rs` (guest-side conversions)
4. Rebuild all WASM plugins
5. Run E2E tests: `cargo test -p poly-plugin-loader-tests --all-features`

## Key Types

- `ClientBackend` — trait for all backend operations (auth, servers, channels, messages, users, events)
- `Server` — a community/workspace (Discord guild, Stoat server, Matrix Space, Teams team)
- `Channel` — text/voice/video channel within a server
- `Message` — a chat message with content, author, timestamp, attachments
- `User` — user profile with name, avatar, presence
- `ClientEvent` — real-time event enum (new message, presence change, etc.)
- `BackendType` — enum identifying the backend (Stoat, Matrix, Discord, Teams, Demo, Poly)

## Design

- Backend-agnostic: no imports from any specific backend crate
- WASM-compatible: must work in `wasm32-wasip2` targets with only `wit-bindgen` available
- Minimal dependencies: serde, chrono, futures only (no dioxus, tokio, or UI deps)
- All methods async
- Event-driven via `Stream<Item = ClientEvent>`

## E2E Validation

The entire `ClientBackend` interface is validated by **77 E2E tests** that exercise every method through the WASM plugin host:

```sh
cargo test -p poly-plugin-loader-tests --all-features -- --nocapture
```

## License

MIT / Apache-2.0
