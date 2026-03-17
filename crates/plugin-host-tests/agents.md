# poly-plugin-loader-tests — Agent Instructions

> **Read this before working on this crate.**
> **Last Updated:** 2026-03-06

---

## Purpose

Integration and E2E test crate for `poly-plugin-host`. Tests live here (not in the
dylib crate) to avoid polluting the shared library with test dependencies and to
avoid recompiling wasmtime for test changes.

## Architecture

```
crates/plugin-host-tests/
├── Cargo.toml              # Feature flags: test-demo, test-stoat, etc.
├── src/lib.rs              # Shared test helpers (workspace_root, wasm_dir, load_plugin, load_plugin_with_host_state)
└── tests/
    ├── integration.rs      # Loads all 6 plugins, verifies types + names
    └── client_e2e/
        ├── main.rs         # Crate root with feature-gated module declarations
        ├── harness.rs      # Shared test suite (~360 lines, interface contract tests)
        ├── demo.rs         # Demo: 26 full E2E tests (authenticate → data → logout)
        ├── stoat.rs        # Stoat: real guest-path auth tests with mocked host I/O
        ├── matrix.rs       # Matrix: 10 stub behavior verification tests
        ├── discord.rs      # Discord: 10 stub behavior verification tests
        ├── teams.rs        # Teams: 10 stub behavior verification tests
        └── server.rs       # Poly Server: 10 stub behavior verification tests
```

## Feature Flags

| Feature | Default | What It Tests |
|---|---|---|
| `test-demo` | ✅ Yes | Full E2E: authenticate, servers, channels, messages, DMs, groups, notifications, voice, presence, logout |
| `test-stoat` | No | Real guest-path auth tests with mocked host I/O, plus current non-auth guest coverage |
| `test-matrix` | No | Stub verification: correct types, error returns, empty lists |
| `test-discord` | No | Stub verification: correct types, error returns, empty lists |
| `test-teams` | No | Stub verification: correct types, error returns, empty lists |
| `test-server` | No | Stub verification: correct types, error returns, empty lists |

## Running Tests

```sh
# 1. Build all WASM plugin binaries first:
cargo component build -p poly-demo -p poly-stoat -p poly-matrix \
    -p poly-discord -p poly-teams -p poly-server-client \
    --target wasm32-wasip2

# 2. Run integration test (all 6 plugins load + verify):
cargo test -p poly-plugin-loader-tests --test integration -- --nocapture

# 3. Run demo E2E tests (26 tests):
cargo test -p poly-plugin-loader-tests --features test-demo --test client_e2e -- --nocapture

# 4. Run ALL client E2E tests (77 tests total):
cargo test -p poly-plugin-loader-tests --all-features -- --nocapture
```

## Test Inventory (77 total)

### Integration Tests (1 test)

| Test | File | Description |
|---|---|---|
| `load_all_wasm_plugins` | `tests/integration.rs` | Loads all 6 `.wasm` files, instantiates each, verifies `backend_type` and `backend_name` |

### Demo E2E Tests (26 tests)

Full lifecycle through WASM plugin host: authenticate → retrieve all data types → mutate → verify → logout.

- Backend identity (type, name)
- Authenticate + logout lifecycle
- Session field validation
- Servers (list, get by ID, not found)
- Channels (list, get by ID, not found, type validation)
- Messages (list, send)
- Users (friends, channel members, get by ID)
- Groups (list, remove member)
- DMs (list, messages)
- Notifications, voice participants
- Presence (get, set to Idle)
- Event stream validation
- Full lifecycle integration test

### Stub Client Tests (10 tests each × 4 clients = 40 tests)

Each remaining stub (matrix, discord, teams, server) verifies:
- Correct `BackendType` and `backend_name()`
- `authenticate()` returns `Err(Internal("not yet implemented"))`
- `is_authenticated()` returns false
- All list methods return empty `Ok(vec![])`
- `get_server()` / `get_channel()` return `Err(NotFound(...))`
- `set_presence()` returns `Ok(())`
- `event_stream()` returns a valid (empty) stream
- `logout()` returns `Ok(())`

### Stoat Plugin Tests (10 tests)

Stoat is now the first plugin to validate a **real guest-path** slice through mocked host I/O:

- backend identity
- mocked email/password auth success through `host-api.http-request`
- mocked token auth success through `host-api.http-request`
- guard that dummy auth no longer returns a stub-marker error string
- current non-auth guest behavior coverage for not-found / presence / event-stream / logout

## Shared Helpers (`src/lib.rs`)

| Function | Returns | Purpose |
|---|---|---|
| `workspace_root()` | `PathBuf` | Resolve workspace root from `CARGO_MANIFEST_DIR` |
| `wasm_dir()` | `PathBuf` | Path to `target/wasm32-wasip1/debug/` |
| `load_plugin(id, filename)` | `Result<PluginBackend>` | Create fresh registry, load one WASM file, instantiate and return |
| `load_plugin_with_host_state(id, filename, host_state)` | `Result<PluginBackend>` | Instantiate a real WASM plugin with deterministic mocked host I/O |

## Shared Harness (`harness.rs`)

Reusable test functions organized by category:
- **Identity**: `assert_backend_type()`, `assert_backend_name()`
- **Lifecycle**: `authenticate_with_token()`, `authenticate_returns_error()`, `logout_succeeds()`, `authenticate_does_not_use_stub_path()`
- **Data Retrieval**: servers, channels, messages, users, friends, groups, DMs, notifications, voice
- **Mutations**: `send_text_message()`, `set_presence()`, `get_presence()`
- **Events**: `event_stream_is_valid()`
- **Stubs**: `assert_stub_returns_empty_lists()` — comprehensive empty-list verification

## Key Notes

- `main.rs` has `#![allow(clippy::unwrap_used, clippy::expect_used)]` at crate root — this is a **test binary**, so these are allowed per project policy
- `load_plugin()` returns `Result` — callers in test files `.unwrap()` it
- PresenceStatus variants: `Online`, `Idle`, `DoNotDisturb`, `Invisible`, `Offline` — NOT `Away`
- Each test creates a fresh `PluginBackend` instance (no shared state between tests)
- **Critical rule:** plugin work must be validated through the real WASM guest path when possible. Native-only success is not sufficient proof for plugin behavior.

## Session Log

- **2026-03-06** — Created crate. Moved integration test from `poly-core::plugin_host::tests` (D22).
- **2026-03-06** — Added comprehensive E2E client interface test framework (2.14.16): 76 E2E tests + 1 integration test, feature-flagged per client, shared harness with full interface contract testing.
- **2026-03-17** — Added mocked host HTTP support (`PluginHostState::with_mock_http_response` + `load_plugin_with_host_state`) so client E2E tests can exercise real guest logic through the WASM plugin path instead of validating only stub/native behavior. Stoat now uses this for guest auth coverage.
