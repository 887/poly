# poly-plugin-loader-tests

Comprehensive integration and end-to-end tests for the `poly-plugin-host` WASM plugin runtime.

## Test Coverage

**77 tests total** — all passing ✅

| Test Suite | Tests | Scope |
|---|---|---|
| `integration.rs` | 1 | Load all 6 WASM plugins, verify backend types + names |
| `client_e2e/demo.rs` | 26 | Full E2E: authenticate → retrieve data → mutate → logout |
| `client_e2e/stoat.rs` | 10 | Stub behavior verification |
| `client_e2e/matrix.rs` | 10 | Stub behavior verification |
| `client_e2e/discord.rs` | 10 | Stub behavior verification |
| `client_e2e/teams.rs` | 10 | Stub behavior verification |
| `client_e2e/server.rs` | 10 | Stub behavior verification |

## Feature Flags

Each client has a feature flag to enable/disable its test module:

- `test-demo` (**default**) — full E2E demo tests
- `test-stoat` — Stoat stub tests
- `test-matrix` — Matrix stub tests
- `test-discord` — Discord stub tests
- `test-teams` — Teams stub tests
- `test-server` — Poly Server stub tests

## Running

```sh
# Build plugin WASM binaries first:
cargo component build -p poly-demo -p poly-stoat -p poly-matrix \
    -p poly-discord -p poly-teams -p poly-server-client \
    --target wasm32-wasip2

# Run demo E2E tests only (default feature):
cargo test -p poly-plugin-loader-tests --test client_e2e -- --nocapture

# Run ALL tests (all clients):
cargo test -p poly-plugin-loader-tests --all-features -- --nocapture

# Run a specific client's tests:
cargo test -p poly-plugin-loader-tests --features test-stoat --test client_e2e -- --nocapture
```

## Architecture

- **`src/lib.rs`**: Shared helpers — `workspace_root()`, `wasm_dir()`, `load_plugin()`
- **`tests/integration.rs`**: Basic load-all-plugins smoke test
- **`tests/client_e2e/harness.rs`**: Reusable test functions for the full `ClientBackend` interface
- **`tests/client_e2e/<client>.rs`**: Per-client test modules, feature-gated

## License

MIT OR Apache-2.0
