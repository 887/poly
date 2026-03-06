# Phase 2.14 — WASM Plugin System

> **Created:** 2026-03-06  
> **Status:** In Progress  
> **Decision:** D21 — All messenger backends become WASM plugins loaded at runtime  
> **Decision:** D22 — Plugin host extracted to dynamically-linked crate (`poly-plugin-host`)  
> **Dependencies:** Phase 2.13 (existing client interface must be stable)  
> **Architecture Doc:** [WASM Plugin Architecture](wasm-plugin-architecture.md)

---

## Goal

Transform all messenger backend clients (demo, stoat, matrix, discord, teams, server-client) from
compile-time Rust library dependencies into **self-contained WASM Component Model binaries** that
are loaded at runtime by a plugin host. The main app ships with zero protocol-specific code built
in — backends are plugins.

**D22 addition (2026-03-06):** The plugin host runtime (wasmtime + host-api) lives in its own
dynamically-linked crate (`poly-plugin-host`, `crate-type = ["dylib"]`). This means:
- Changing poly-core **never** recompiles wasmtime (separate compilation unit)
- Relinking the final desktop binary is fast (dynamic .so/.dll/.dylib reference)
- On web: plugin host is simply not included (browser is the WASM runtime)
- Tests run in `poly-plugin-loader-tests` crate (no wasmtime recompile for test changes)

---

## 2.14.1 — Toolchain & Infrastructure Setup ✅

- [x] Install `wasm32-wasip2` rustup target (`rustup target add wasm32-wasip2`)
- [x] Install `cargo-component` tool (`cargo install cargo-component`)
- [x] Install `wasm-tools` (`cargo install wasm-tools`)
- [x] Verify `wasm32-wasip2` target compiles a trivial Rust cdylib
- [x] Create `wit/` directory in workspace root for shared WIT definitions
- [x] Add `wit/` to workspace `.gitignore` exclusions if needed

## 2.14.2 — WIT Interface Definition ✅

Write the canonical WIT file that mirrors the `ClientBackend` trait and all poly-client types.

- [x] Create `wit/messenger-plugin.wit` package header (`package poly:messenger@0.1.0`)
- [x] Define WIT `record` for `server` (id, name, icon-url, categories, backend, unread-count, account-id, account-display-name)
- [x] Define WIT `record` for `category` (id, name, channel-ids)
- [x] Define WIT `enum` for `channel-type` (text, voice, video)
- [x] Define WIT `record` for `channel` (id, name, channel-type, server-id, unread-count, last-message-id)
- [x] Define WIT `variant` for `message-content` (text, with-attachments)
- [x] Define WIT `record` for `attachment` (id, filename, content-type, url, size)
- [x] Define WIT `record` for `message` (id, author, content, timestamp, attachments, reactions, edited)
- [x] Define WIT `record` for `reaction` (emoji, count, me)
- [x] Define WIT `record` for `message-query` (before, after, limit)
- [x] Define WIT `record` for `user` (id, display-name, avatar-url, presence, backend)
- [x] Define WIT `enum` for `presence-status` (online, idle, do-not-disturb, invisible, offline)
- [x] Define WIT `enum` for `backend-type` (stoat, matrix, discord, teams, demo, poly)
- [x] Define WIT `record` for `session` (id, user, token, backend, icon-emoji, instance-id)
- [x] Define WIT `variant` for `auth-credentials` (token, email-password, oauth, device-code, poly-server)
- [x] Define WIT `record` for `group` (id, members, name, last-message, backend, account-id)
- [x] Define WIT `record` for `dm-channel` (id, user, last-message, unread-count, backend, account-id)
- [x] Define WIT `record` for `notification` (id, kind, backend, account-id, timestamp, read, preview)
- [x] Define WIT `variant` for `notification-kind` (mention, friend-request, server-invite, other)
- [x] Define WIT `record` for `voice-participant` (user, is-muted, is-deafened, is-streaming, is-video-on, is-speaking)
- [x] Define WIT `record` for `voice-connection` (channel-id, server-id, channel-name, server-name, backend, account-id, instance-id, is-muted, is-deafened, is-streaming, is-video-on)
- [x] Define WIT `record` for `account` (id, backend, display-name, connected)
- [x] Define WIT `variant` for `connection-status` (connected, connecting, disconnected, error)
- [x] Define WIT `enum` for `account-presence` (online, away, do-not-disturb, appear-offline, offline)
- [x] Define WIT `variant` for `client-error` (auth-failed, network, not-found, rate-limited, permission-denied, internal, not-supported)
- [x] Define WIT `variant` for `client-event` (all event types from events.rs)
- [x] Define WIT `enum` for `log-level` (trace, debug, info, warn, error)
- [x] Define WIT `interface host-api` with all host imports:
  - [x] `http-request` (method, url, headers, body) → result
  - [x] `websocket-connect` (url, headers) → result<handle>
  - [x] `websocket-send` (handle, data) → result
  - [x] `websocket-recv` (handle) → result<option<data>>
  - [x] `websocket-close` (handle) → result
  - [x] `storage-get` (key) → option<bytes>
  - [x] `storage-set` (key, value) → result
  - [x] `storage-delete` (key) → result
  - [x] `log` (level, message)
  - [x] `get-current-time` () → timestamp string (RFC3339)
- [x] Define WIT `interface messenger-client` with all guest exports:
  - [x] `authenticate` (credentials) → result<session, client-error>
  - [x] `logout` () → result<_, client-error>
  - [x] `is-authenticated` () → bool
  - [x] `get-servers` () → result<list<server>, client-error>
  - [x] `get-server` (id) → result<server, client-error>
  - [x] `get-channels` (server-id) → result<list<channel>, client-error>
  - [x] `get-channel` (id) → result<channel, client-error>
  - [x] `send-message` (channel-id, content) → result<message, client-error>
  - [x] `get-messages` (channel-id, query) → result<list<message>, client-error>
  - [x] `get-user` (user-id) → result<user, client-error>
  - [x] `get-friends` () → result<list<user>, client-error>
  - [x] `get-channel-members` (channel-id) → result<list<user>, client-error>
  - [x] `get-groups` () → result<list<group>, client-error>
  - [x] `remove-group-member` (group-id, user-id) → result<_, client-error>
  - [x] `get-dm-channels` () → result<list<dm-channel>, client-error>
  - [x] `get-notifications` () → result<list<notification>, client-error>
  - [x] `get-voice-participants` (channel-id) → result<list<voice-participant>, client-error>
  - [x] `get-presence` (user-id) → result<presence-status, client-error>
  - [x] `set-presence` (status) → result<_, client-error>
  - [x] `poll-event` () → option<client-event>
  - [x] `backend-type` () → backend-type-enum
  - [x] `backend-name` () → string
- [x] Define WIT `world messenger-plugin` importing host-api, exporting messenger-client
- [x] Validate WIT file syntax with `wasm-tools component wit wit/messenger-plugin.wit`

## 2.14.3 — Plugin Host Runtime (poly-core) ✅ → Superseded by 2.14.15

> **NOTE:** This section was originally built inside poly-core. It has been
> completed and then **moved** to the standalone `poly-plugin-host` crate
> in step 2.14.15 (DECISION D22).

- [x] Add `wasmtime` dependency to poly-core (native-only: `cfg(not(target_arch = "wasm32"))`)
- [x] Add `wasmtime-wasi` dependency to poly-core (native-only)
- [x] Create `crates/core/src/plugin_host/` module directory
- [x] Create `crates/core/src/plugin_host/mod.rs` with module structure
- [x] Create `crates/core/src/plugin_host/engine.rs` — wasmtime Engine singleton configuration
  - [x] Enable Component Model (`wasm_component_model(true)`)
  - [x] Configure fuel metering for resource limits
- [x] Create `crates/core/src/plugin_host/host_impl.rs` — PluginHostState struct + host-api
  - [x] WASI context field (minimal — clocks + random only)
  - [x] ResourceTable for wasmtime component handles
  - [x] HTTP client (reqwest) for host-api http-request
  - [x] WebSocket connection map (HashMap<u64, WebSocketHandle>)
  - [x] Storage namespace prefix (per-plugin isolation)
  - [x] Logger target name (per-plugin)
- [x] Use `wasmtime::component::bindgen!` macro to generate host-side bindings from WIT
- [x] Implement `HostApiImports` trait for HostState:
  - [x] `http_request()` — delegate to reqwest
  - [x] `websocket_connect()` — delegate to tokio-tungstenite, store handle
  - [x] `websocket_send()` — write to stored WebSocket
  - [x] `websocket_recv()` — try_read from stored WebSocket
  - [x] `websocket_close()` — remove and drop stored WebSocket
  - [x] `storage_get()` — read from in-memory HashMap (SurrealKV wiring TODO)
  - [x] `storage_set()` — write to in-memory HashMap
  - [x] `storage_delete()` — delete from in-memory HashMap
  - [x] `log()` — route to `tracing` crate with plugin name context
  - [x] `get_current_time()` — return RFC3339 timestamp
- [x] Create `crates/core/src/plugin_host/bridge.rs` — WIT↔poly-client type conversion
  - [x] Implements `from_wit_*` and `to_wit_*` for all types
  - [x] ClientEvent conversion for all 13 event variants
- [x] Create `crates/core/src/plugin_host/registry.rs` — PluginRegistry + PluginBackend
  - [x] Registry of available plugin types (plugin_id → Component)
  - [x] Load from bytes or filesystem
  - [x] PluginBackend implements `ClientBackend` trait
  - [x] Cached backend_type/backend_name from WIT exports at instantiation
  - [x] `event_stream()` via async polling loop + tokio channel
- [x] Wire plugin_host module into `crates/core/src/lib.rs`
- [x] Verify `cargo check --workspace` passes
- [x] Verify `cargo check -p poly-web --target wasm32-unknown-unknown` (plugin_host gated behind cfg)

## 2.14.4 — Convert Demo Client to WASM Plugin (Proof of Concept) ✅

- [x] Read poly-demo current code fully (lib.rs + data.rs)
- [x] Add `wit-bindgen = "0.53"` dependency to poly-demo Cargo.toml
- [x] Change poly-demo `[lib]` to `crate-type = ["cdylib", "rlib"]`
- [x] Add `[package.metadata.component]` section pointing to `../../wit`
- [x] Create `guest.rs` with WIT Guest impl using thread_local state
- [x] Feature-gate native deps behind `native` feature (default)
- [x] Build with `cargo component build -p poly-demo --target wasm32-wasip2`
- [x] Verify `.wasm` file produced (37 MB debug)
- [x] Test loading demo.wasm in the plugin host ✅

## 2.14.5 — Convert Stoat Client to WASM Plugin ✅

- [x] Add `wit-bindgen` dependency, `crate-type = ["cdylib", "rlib"]`
- [x] Add `[package.metadata.component]` section
- [x] Create `guest.rs` with stub WIT Guest impl
- [x] Feature-gate native deps behind `native` feature
- [x] Build with `cargo component build -p poly-stoat --target wasm32-wasip2` (4.3 MB)

## 2.14.6 — Convert Matrix Client to WASM Plugin ✅

- [x] Add `wit-bindgen` dependency, `crate-type = ["cdylib", "rlib"]`
- [x] Create `guest.rs` with stub WIT Guest impl
- [x] Feature-gate native deps behind `native` feature
- [x] Build (4.3 MB)

## 2.14.7 — Convert Discord Client to WASM Plugin ✅

- [x] Add `wit-bindgen` dependency, `crate-type = ["cdylib", "rlib"]`
- [x] Create `guest.rs` with stub WIT Guest impl
- [x] Feature-gate native deps behind `native` feature
- [x] Build (4.3 MB)

## 2.14.8 — Convert Teams Client to WASM Plugin ✅

- [x] Add `wit-bindgen` dependency, `crate-type = ["cdylib", "rlib"]`
- [x] Create `guest.rs` with stub WIT Guest impl
- [x] Feature-gate native deps behind `native` feature
- [x] Build (4.3 MB)

## 2.14.9 — Convert Server Client to WASM Plugin ✅

- [x] Add `wit-bindgen` dependency, `crate-type = ["cdylib", "rlib"]`
- [x] Create `guest.rs` with stub WIT Guest impl
- [x] Feature-gate native deps behind `native` feature (⚠️ requires `--no-default-features` for WASM build)
- [x] Build (4.2 MB)

## 2.14.10 — Remove Direct Client Dependencies from poly-core

- [ ] Remove `poly-demo` optional dependency from poly-core Cargo.toml
- [ ] Remove `poly-stoat` optional dependency from poly-core Cargo.toml
- [ ] Remove `poly-matrix` optional dependency from poly-core Cargo.toml
- [ ] Remove `poly-discord` optional dependency from poly-core Cargo.toml
- [ ] Remove `poly-teams` optional dependency from poly-core Cargo.toml
- [ ] Remove `poly-server-client` dependency from poly-core Cargo.toml
- [ ] Remove old feature flags (`demo`, `stoat`, `matrix`, `discord`, `teams`) from poly-core
- [ ] Add new feature flags (`embed-demo`, `embed-stoat`, etc.) that control `include_bytes!`
- [ ] Update `client_manager.rs` to instantiate plugins via PluginRegistry instead of direct Rust structs
- [ ] Update demo toggle logic to load/unload demo plugin WASM
- [ ] Update `poly-server-client` activation to go through plugin host
- [ ] Verify `cargo check --workspace` passes
- [ ] Verify `cargo check -p poly-web --target wasm32-unknown-unknown` passes
- [ ] Verify `cargo cranky --workspace` passes with zero warnings

## 2.14.11 — Remove Client Crates from Workspace Native Build

- [ ] Update workspace `Cargo.toml` — move client crates to `exclude` (they build with cargo-component, not regular cargo build)
- [ ] OR: Keep in workspace but mark them as `[lib] crate-type = ["cdylib"]` only (cargo build skips them for native target)
- [ ] Create `scripts/build-plugins.sh` script that builds all plugin .wasm files
- [ ] Add VS Code task for building all plugins
- [ ] Update CI to build plugins separately with cargo-component
- [ ] Update CI to run plugin integration tests

## 2.14.12 — Embed Built-in Plugins

- [ ] Create `crates/core/src/plugin_host/embedded.rs` module
- [ ] Use `include_bytes!("../../../../target/wasm32-wasip2/release/poly_demo.wasm")` (or similar path)
- [ ] Gate each embed behind feature flags (`embed-demo`, etc.)
- [ ] Register embedded plugins in PluginRegistry at startup
- [ ] Verify demo toggle works with embedded WASM plugin
- [ ] Verify app builds and runs with embedded demo plugin

## 2.14.13 — Integration Testing & Verification (Partially Complete)

- [x] Build ALL client plugins as .wasm files (demo, stoat, matrix, discord, teams, server-client)
- [ ] Verify `cargo tree -p poly-core` shows ZERO deps on any client crate except poly-client
- [ ] Verify `cargo tree -p poly-demo` shows ZERO deps on poly-core, poly-stoat, or other clients
- [ ] Verify each client's only workspace dependency is `poly-client`
- [ ] Verify no `use poly_demo::`, `use poly_stoat::`, etc. anywhere in poly-core source
- [x] Load demo plugin → integration test passes (backend_type, backend_name verified)
- [x] Load stoat plugin (stub) → integration test passes
- [x] Load matrix plugin (stub) → integration test passes
- [x] Load discord plugin (stub) → integration test passes
- [x] Load teams plugin (stub) → integration test passes
- [x] Load server-client plugin → integration test passes (BackendType::Poly)
- [ ] Test plugin loading from filesystem (`~/.poly/plugins/`)
- [ ] Measure plugin load time (first load, cached load)
- [ ] Measure function call overhead (benchmark get_servers, get_messages)
- [ ] Run `cargo check --workspace` — zero errors
- [ ] Run `cargo cranky --workspace` — zero warnings
- [ ] Run `cargo check -p poly-web --target wasm32-unknown-unknown` — WASM compat verified
- [ ] Run `cargo fmt --all`

## 2.14.14 — Documentation & agents.md Updates ✅

- [x] Update `clients/client/agents.md` — document WIT correspondence
- [x] Update `clients/demo/agents.md` — document WASM plugin structure
- [x] Update `clients/stoat/agents.md` — document WASM plugin structure
- [x] Update `clients/matrix/agents.md` — document WASM plugin structure
- [x] Update `clients/discord/agents.md` — document WASM plugin structure
- [x] Update `clients/teams/agents.md` — document WASM plugin structure
- [x] Update `clients/server-client/agents.md` — document WASM plugin structure
- [ ] Update `crates/core/agents.md` — document plugin_host module removal / re-export
- [x] Update `crates/plugin-host/agents.md` — document new crate ✅
- [ ] Update root `agents.md` — add WASM plugin rules + D22
- [x] Write workspace README with plugin architecture description
- [ ] Write `wit/README.md` explaining the WIT interface

## 2.14.15 — Extract Plugin Host to Dynamically-Linked Crate (DECISION D22)

> **Created:** 2026-03-06  
> **Why:** Compiling wasmtime (42.x) every time poly-core changes is prohibitively slow.
> By moving the plugin host to a Rust `dylib` crate, wasmtime compiles once into
> a shared library (.so/.dll/.dylib). Editing poly-core only relinks against the
> .so reference — no wasmtime recompilation, fast iteration.

### Architecture

```
crates/plugin-host/         # poly-plugin-host (crate-type = ["dylib"])
├── src/
│   ├── lib.rs              # Public API + re-exports
│   ├── engine.rs           # wasmtime engine + WIT bindgen! macro
│   ├── host_impl.rs        # host-api implementation (HTTP, WS, storage, logging)
│   ├── bridge.rs           # WIT ↔ poly-client type conversion
│   └── registry.rs         # PluginRegistry + PluginBackend (impl ClientBackend)
│
crates/plugin-host-tests/   # poly-plugin-loader-tests
├── tests/
│   └── integration.rs      # load_all_wasm_plugins test
│
crates/core/                # poly-core (NO more wasmtime dependency)
├── src/
│   └── lib.rs              # pub use poly_plugin_host as plugin_host; (cfg-gated)
```

### Platform Strategy

| Platform | Plugin Host | How |
|----------|------------|-----|
| Desktop (Wry/Blitz/Electron) | poly-plugin-host.so | Rust `dylib`, linked dynamically |
| Mobile (Android/iOS) | poly-plugin-host.so | Same dylib, ARM build |
| Web (browser client) | Not included | `cfg(not(target_arch = "wasm32"))` — browser is the WASM runtime |
| Web (fullstack server) | Native wasmtime | Server process loads plugins natively |

### Checklist

- [x] Create `crates/plugin-host/` directory
- [x] Create `crates/plugin-host/Cargo.toml` with `crate-type = ["dylib"]`
  - [x] Dependencies: wasmtime, wasmtime-wasi, poly-client, tokio, chrono, reqwest, etc.
  - [x] All heavy deps isolated here — poly-core gets none of them
- [x] Create `crates/plugin-host/cranky.toml` (same lint policy as workspace)
- [x] Create `crates/plugin-host/agents.md`
- [x] Create `crates/plugin-host/README.md`
- [x] Move `engine.rs` from poly-core → poly-plugin-host
- [x] Move `host_impl.rs` from poly-core → poly-plugin-host
- [x] Move `bridge.rs` from poly-core → poly-plugin-host
- [x] Move `registry.rs` from poly-core → poly-plugin-host (without tests)
- [x] Create `crates/plugin-host/src/lib.rs` with module declarations + re-exports
- [x] Update workspace `Cargo.toml` — add `crates/plugin-host` and `crates/plugin-host-tests` to members
- [x] Add `poly-plugin-host` to `[workspace.dependencies]`
- [x] Update poly-core `Cargo.toml`:
  - [x] Remove `wasmtime`, `wasmtime-wasi` dependencies
  - [x] Remove `tokio-tungstenite`, `futures-util`, `tokio-stream` dependencies
  - [x] Add `poly-plugin-host` as native-only dependency
- [x] Replace `crates/core/src/plugin_host/` module with thin re-export:
  - [x] `pub use poly_plugin_host as plugin_host;` (cfg-gated)
  - [x] Remove all original source files from `crates/core/src/plugin_host/`
- [x] Create `crates/plugin-host-tests/` directory
- [x] Create test crate with `poly-plugin-host` dependency
- [x] Move integration test (`load_all_wasm_plugins`) to test crate
- [x] Verify `cargo check --workspace` passes ✅
- [x] Verify `cargo check -p poly-web --target wasm32-unknown-unknown` passes ✅
- [x] Verify `cargo test -p poly-plugin-loader-tests` passes ✅ (all 6 plugins loaded + verified)
- [x] Verify `cargo cranky --workspace` passes ✅ (zero warnings)
- [x] Verify `cargo fmt --all` ✅
- [x] Update agents.md for poly-core, poly-plugin-host, poly-plugin-loader-tests

## 2.14.16 — End-to-End Client Interface Tests (via WASM Plugin Host)

> **Created:** 2026-03-06
> **Why:** Every client plugin implements the same `ClientBackend` trait through WIT.
> A shared test harness exercises the full interface (authenticate, get data, lifecycle)
> to ensure all plugins conform to the contract. Feature flags allow running tests
> per-client (e.g. `cargo test --features test-demo`).

### Architecture

```
crates/plugin-host-tests/
├── Cargo.toml              # Feature flags: test-demo, test-stoat, ...
├── src/lib.rs              # Shared test helpers (plugin loading)
└── tests/
    ├── integration.rs      # Existing: load all 6 plugins
    └── client_e2e/
        ├── main.rs         # Feature-gated module declarations
        ├── harness.rs      # Shared test suite (interface contract tests)
        ├── demo.rs         # Demo: full E2E (authenticate → data → logout)
        ├── stoat.rs        # Stoat: stub behavior verification
        ├── matrix.rs       # Matrix: stub behavior verification
        ├── discord.rs      # Discord: stub behavior verification
        ├── teams.rs        # Teams: stub behavior verification
        └── server.rs       # Poly Server: stub behavior verification
```

### Checklist

- [x] Add feature flags to `poly-plugin-loader-tests` Cargo.toml
- [x] Create `tests/client_e2e/main.rs` with feature-gated modules
- [x] Create `tests/client_e2e/harness.rs` — shared interface contract tests
- [x] Create `tests/client_e2e/demo.rs` — full E2E demo test suite (26 tests)
- [x] Create `tests/client_e2e/stoat.rs` — stub verification (10 tests)
- [x] Create `tests/client_e2e/matrix.rs` — stub verification (10 tests)
- [x] Create `tests/client_e2e/discord.rs` — stub verification (10 tests)
- [x] Create `tests/client_e2e/teams.rs` — stub verification (10 tests)
- [x] Create `tests/client_e2e/server.rs` — stub verification (10 tests)
- [x] Verify `cargo test -p poly-plugin-loader-tests --features test-demo` passes (26/26 ✅)
- [x] Verify `cargo test -p poly-plugin-loader-tests --all-features` passes (76/76 ✅ + 1 integration)
- [x] Update `crates/plugin-host-tests/agents.md`
- [x] Update all README.md and agents.md files

---

## Session Log

### 2026-03-06 — Phase 2.14 Created
- Created WASM plugin architecture document (`docs/wasm-plugin-architecture.md`)
- Added D21 decision to overall-plan.md

### 2026-03-06 — D21 Complete, D22 Extraction
- All 6 client plugins built as WASM Component Model binaries (2.14.4–2.14.9)
- Plugin host runtime built in poly-core (2.14.3)
- Integration test passing: all 6 plugins load + instantiate + report correct types
- D22: Extracted plugin_host to `poly-plugin-host` dylib crate (2.14.15)
  - wasmtime isolated behind dynamic linking boundary
  - poly-core re-exports via `pub use poly_plugin_host as plugin_host`
  - Old `crates/core/src/plugin_host/` directory deleted
  - Test crate `poly-plugin-loader-tests` created with integration test
  - All verification passes: check, cranky, WASM, fmt, test

### 2026-03-06 — E2E Client Interface Tests (2.14.16)
- Created comprehensive E2E test suite exercising full ClientBackend interface through WASM
- Feature-flagged per-client: `test-demo`, `test-stoat`, `test-matrix`, `test-discord`, `test-teams`, `test-server`
- Shared harness tests interface contract (identity, lifecycle, data, errors)
- Demo gets deep E2E tests (authenticate → servers → channels → messages → DMs → groups → notifications → voice → presence → logout)
- Stubs get behavior verification (correct types, empty lists, proper errors)
- Created this phase plan with detailed checkboxes for every sub-step
- Technology choices: wasmtime 42.x + WIT Component Model + cargo-component
- Architecture: Host provides syscall-like imports, guests export messenger-client interface
- Platform strategy: Native wasmtime (desktop/Android), AOT (iOS), server-hosted (web)

### 2026-03-06 — Steps 2.14.1–2.14.9 Completed + Integration Test
- All 6 WASM plugins built and passing integration test
- Backend type/name caching fix in registry.rs (plugins self-report via WIT exports)
- All agents.md and README.md files updated with WASM architecture info

### 2026-03-06 — DECISION D22: Dynamic Linking Extraction
- Moved plugin_host to `poly-plugin-host` crate with `crate-type = ["dylib"]`
- Created `poly-plugin-loader-tests` for isolated integration testing
- poly-core no longer depends on wasmtime — uses re-export from dylib
- Rationale: wasmtime compilation time unacceptable for iterative poly-core development

### 2026-03-06 — E2E Tests Complete (2.14.16)
- **77 total tests passing** (76 E2E + 1 integration)
- Demo: 26 tests — full lifecycle (authenticate → data → mutate → logout)
- Stubs: 10 tests each × 5 clients = 50 tests — behavior verification
- Fixed: `load_plugin()` returns `Result` (cranky-compliant), PresenceStatus::Idle not Away
- All agents.md and README.md updated across: root, core, plugin-host, plugin-host-tests, client, demo, stoat, matrix, discord, teams, server-client
- Fixed outdated root README references from `crates/core/src/plugin_host/` to `crates/plugin-host/`
