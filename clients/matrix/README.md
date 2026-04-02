# poly-matrix

Matrix protocol client for Poly. Implements `ClientBackend` using the Matrix
client-server HTTP API directly — no `matrix-sdk` dependency.

## Build Modes

| Mode | Command | Feature | Output |
|------|---------|---------|--------|
| Native | `cargo build -p poly-matrix` | `native` (default) | `libpoly_matrix.rlib` |
| WASM plugin | `cargo component build -p poly-matrix --target wasm32-wasip2` | (none) | `poly_matrix.wasm` |

## Architecture

- **Native**: `config.rs` + `http.rs` + `api.rs` → `MatrixClient` implementing `ClientBackend`
- **WASM guest**: `guest.rs` implements the Matrix HTTP protocol using `host_api::http_request()` WIT import

Both paths implement the same protocol directly. No external Matrix SDK.

## Current Status (2026-04-01)

- Scaffolding in place: Cargo.toml, modules, config, HTTP transport, API types
- All `ClientBackend` methods are stubs returning empty/error
- WASM guest is stub, returning "not yet implemented"
- 10 E2E tests verify stub behavior through plugin host
- Locale files: en, de, fr, es

## Testing

```sh
# Native unit tests
cargo test -p poly-matrix

# E2E plugin tests
cargo test -p poly-plugin-loader-tests --features test-matrix --test client_e2e -- --nocapture
```

## License

MIT / Apache-2.0
