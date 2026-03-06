# poly-server-client — Agent Instructions

> **Read the root `agents.md` first.**
> **Last updated:** 2026-03-06

---

## Purpose

This crate implements the complete client for talking to a running
poly-server instance.  It is consumed by `poly-core` via the shared
`poly-client` trait but is otherwise self-contained.  Everything that talks
directly to the server's HTTP or WebSocket APIs lives here.

The public API consists of three types:

- `PolyServerHttpClient` — typed REST client (reqwest-based)
- `PolyServerWsClient` — WebSocket client with broadcast publisher
- `PolyServerBackend` — `ClientBackend` adapter used by the UI

In addition there are wire-format models, error types, and a handful of
utility helpers.

## WASM Plugin Architecture (DECISION D21, 2026-03-06)

This crate builds as **both** a native Rust library AND a WASM Component Model plugin.

- **Crate type**: `["cdylib", "rlib"]`
- **Feature gate**: `native` feature (default) enables ALL heavy deps (reqwest, tokio-tungstenite, ed25519-dalek, hex, etc.)
- **WASM guest**: `src/guest.rs` — currently a **stub** returning errors/empty results. Must be completed when HTTP/WS calls are routed through host-api imports.
- **cfg pattern**: `#[cfg(feature = "native")]` for native code, `#[cfg(target_os = "wasi")]` for WASI plugin code. **NEVER** use `target_arch = "wasm32"`.
- **All module declarations** in `lib.rs` (`pub mod backend`, `pub mod error`, `pub mod http`, `pub mod models`, `pub mod ws`) plus re-exports are cfg-gated behind `#[cfg(feature = "native")]`

### Building

```sh
# Native (default, part of workspace):
cargo build -p poly-server-client

# WASM plugin (MUST use --no-default-features — native-tls can't cross-compile):
cargo component build -p poly-server-client --target wasm32-wasip2 --no-default-features
# Output: target/wasm32-wasip1/debug/poly_server_client.wasm (~4.2MB debug)
```

**⚠️ CRITICAL**: Unlike all other client crates, this one REQUIRES `--no-default-features` when building as WASM because `tokio-tungstenite` pulls in `native-tls` which links against OpenSSL and cannot cross-compile to WASM.

### Key Files

| File | Purpose |
|---|---|
| `src/lib.rs` | All native modules + re-exports cfg-gated behind `feature = "native"` |
| `src/guest.rs` | WIT guest stub — returns errors for all operations, reports `BackendType::Poly` |
| `Cargo.toml` | Dual crate-type, ALL native deps made optional behind `native` feature |

### guest.rs Notes

- `#![allow(unsafe_code)]` — required for wit-bindgen FFI
- All methods return `Err(ClientError::Internal("not yet implemented"))` or empty collections
- `get_backend_type()` returns `BackendType::Poly`, `get_backend_name()` returns `"Poly Server"`
- Future: HTTP calls will be routed through the WIT `host-api` imports instead of reqwest

## Development notes

- **Tests:** The integration suite (`tests/integration.rs`) boots an in-process
  `poly-server` with a temporary SurrealKV database. Run as
  `cargo test -p poly-server-client -- --test-threads=1` (tests are
  currently serialized to avoid SurrealKV collisions).

- **Linting:** This crate has its own `cranky.toml` with the workspace lints.
  Run `cargo cranky -p poly-server-client` frequently; the workspace CI will
  treat any clippy warning as an error.

- **Hot-reload:** Not required—HTTP/WS client code rarely changes—but any
  structural change to shared models should be verified via `dx serve` in
  the desktop-devtools project since poly-core imports this crate.

- **Dependency updates:** Because this crate is upstream of several
  backends (poly-core, poly-client tests, demo data), keep its dependencies
  on the latest versions.  Check `last-crate-update-date` regularly.

## File structure

```
clients/server-client/
├── Cargo.toml           # crate manifest (dual crate-type, feature-gated deps)
├── agents.md            # this file
├── README.md            # overview of the client
├── src/
│   ├── lib.rs           # re-exports and visibility (native cfg-gated)
│   ├── backend.rs       # PolyServerBackend implementation (native)
│   ├── http.rs          # HTTP client definitions (native)
│   ├── ws.rs            # WebSocket client (native)
│   ├── models.rs        # serde wire-format types (native)
│   ├── error.rs         # error type and Result alias (native)
│   └── guest.rs         # WIT guest bridge (WASI-only, stub)
└── tests/
    └── integration.rs   # end-to-end scenarios against a live server
```

## Building & testing

1. Ensure workspace is up-to-date:
   ```sh
   cargo update
   cargo cranky --workspace
   cargo check --workspace
   cargo check -p poly-web --target wasm32-unknown-unknown
   ```

2. Run this crate's tests:
   ```sh
   cargo test -p poly-server-client -- --test-threads=1
   ```

3. Build the WASM plugin:
   ```sh
   cargo component build -p poly-server-client --target wasm32-wasip2 --no-default-features
   ```

4. After making API changes also re-run the `poly-client` and `poly-core`
   integration suites to confirm downstream compatibility.

5. Format with:
   ```sh
   cargo fmt --all
   ```

## E2E Test Coverage (2026-03-06)

**10 tests** in `crates/plugin-host-tests/tests/client_e2e/server.rs` — stub behavior verification through WASM plugin host:

- Backend identity (type=Poly, name="Poly Server")
- `authenticate()` returns `Err(Internal("not yet implemented"))`
- `is_authenticated()` returns false
- All list methods return empty `Ok(vec![])`
- `get_server()` / `get_channel()` return `Err(NotFound(...))`
- `set_presence()`, `logout()` return `Ok(())`
- Event stream returns valid (empty) stream

```sh
cargo test -p poly-plugin-loader-tests --features test-server --test client_e2e -- --nocapture
```

## Crate-specific lints

Inherited from workspace but worth repeating here:

- `warnings = true` (treat all compiler warnings as errors)
- `unsafe_code = true` (no unsafe allowed in native code)
- Deny `clippy::unwrap_used`, `expect_used`, `panic`, `indexing_slicing`,
  `print_stdout`, `print_stderr`.

Integration tests may use `allow(clippy::unwrap_used)` where appropriate.

**Exception for `guest.rs`**: `#![allow(unsafe_code)]` is required for wit-bindgen FFI.

## Notes

- The client is intentionally minimal; it mirrors the server API rather than
  inventing a separate abstraction layer.  Any new server endpoints should
  have corresponding methods/structs added here before the UI can consume
  them.

- Keep wire models (`models.rs`) in sync with server schema.  When the
  server changes, update here and add a migration in `tests/integration.rs`
  that exercises the new shape.
