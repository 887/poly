# poly-plugin-host

Dynamically-linked WASM Component Model plugin host for the Poly messenger.

## Overview

This crate isolates the heavy `wasmtime` runtime into a shared library
(`.so` on Linux, `.dll` on Windows, `.dylib` on macOS) so that iterating on
`poly-core` never triggers a wasmtime recompilation.

It loads messenger backend plugins compiled as WASM Component Model binaries
(`wasm32-wasip2`) and bridges them to the `ClientBackend` trait from `poly-client`.

## Usage

```rust
use poly_plugin_host::{PluginRegistry, PluginBackend};
use std::path::Path;

// Create the registry (initializes the wasmtime engine)
let mut registry = PluginRegistry::new()?;

// Load a plugin from disk
registry.load_from_file("demo", Path::new("plugins/poly_demo.wasm"))?;

// Instantiate it — returns a ClientBackend implementation
let backend = registry.instantiate("demo").await?;

// Use it like any other ClientBackend
let session = backend.authenticate(credentials).await?;
let servers = backend.get_servers().await?;
```

## Architecture

| Module | Purpose |
|---|---|
| `engine` | Wasmtime engine setup + WIT-generated bindings (`bindgen!`) |
| `host_impl` | Host-side `host-api` import implementation (HTTP, WebSocket, storage, logging) |
| `bridge` | Type conversion: WIT types ↔ `poly-client` types |
| `registry` | `PluginRegistry` (load/manage) + `PluginBackend` (`ClientBackend` impl) |

## Testing

Tests live in the companion crate `poly-plugin-loader-tests` (**77 tests total**):

```sh
# Integration test — load all 6 plugins:
cargo test -p poly-plugin-loader-tests --test integration -- --nocapture

# Full E2E client interface tests (all 6 clients):
cargo test -p poly-plugin-loader-tests --all-features -- --nocapture
```

## Decision

**DECISION(D22):** Dynamic linking boundary for wasmtime isolation. See `docs/phase-2.14-plan.md`.

## License

MIT OR Apache-2.0
