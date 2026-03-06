# Poly — WASM Plugin Architecture

> **Created:** 2026-03-06  
> **Status:** Phase 2.14 — WASM Plugin System  
> **Decision:** D21 — All messenger backends are WASM plugins loaded at runtime

---

## 1. Overview

All messenger backend clients (Demo, Stoat, Matrix, Discord, Teams, Poly Server) are compiled as
**self-contained WebAssembly Component Model binaries** (`.wasm` files) and loaded at runtime by a
plugin host. The only crate that remains as a direct Rust dependency is `poly-client`, which defines
the shared trait/type interface that gets translated into a **WIT (WebAssembly Interface Types)**
contract.

### Why?

1. **App Store distribution**: The main Poly app ships with zero third-party client code built in.
   Backend plugins are loaded dynamically — the app itself is a generic messenger shell.
2. **Sandboxing**: Each plugin runs in an isolated WASM sandbox with zero direct filesystem/network
   access. All I/O goes through host-provided "syscall-like" imports.
3. **Hot-swappable**: Plugins can be updated independently of the main app.
4. **Community extensible**: Third parties can write plugins for new backends without modifying the
   core app.
5. **Legal isolation**: The app doesn't "contain" any protocol-specific code — plugins are separate
   artifacts distributed separately.

---

## 2. Architecture Diagram

```
                        ┌────────────────────────────┐
                        │      WIT Interface          │
                        │  (wit/messenger-plugin.wit) │
                        │                             │
                        │  Defines:                   │
                        │   - Types (Message, Server…)│
                        │   - Exports (guest → host)  │
                        │   - Imports (host → guest)   │
                        └─────────────┬──────────────┘
                                      │
              ┌───────────────────────┼───────────────────────┐
              │                       │                       │
     ┌────────┴───────┐    ┌─────────┴────────┐    ┌────────┴──────┐
     │  poly-demo     │    │  poly-stoat       │    │  poly-matrix  │ ...
     │  (WASM plugin) │    │  (WASM plugin)    │    │  (WASM plugin)│
     │                │    │                   │    │               │
     │  cdylib target │    │  cdylib target    │    │  cdylib target│
     │  wasm32-wasip2 │    │  wasm32-wasip2    │    │  wasm32-wasip2│
     └────────┬───────┘    └─────────┬────────┘    └────────┬──────┘
              │                      │                      │
              └──────────────────────┼──────────────────────┘
                                     │  .wasm files
                                     ▼
    ┌────────────────────────────────────────────────────────────────┐
    │                      Plugin Host Runtime                       │
    │                  (in poly-core / plugin_host.rs)               │
    │                                                                │
    │  ┌───────────────────────────────────────────────────────┐    │
    │  │ #[cfg(not(target_arch = "wasm32"))]                   │    │
    │  │ → wasmtime (Component Model, async, WIT bindings)     │    │
    │  │   Platforms: Desktop + Mobile (AOT on iOS)            │    │
    │  └───────────────────────────────────────────────────────┘    │
    │                                                                │
    │  ┌───────────────────────────────────────────────────────┐    │
    │  │ #[cfg(target_arch = "wasm32")]                        │    │
    │  │ → Server-side hosting (web fullstack — Axum server    │    │
    │  │   runs wasmtime, browser communicates via HTTP/WS)    │    │
    │  │   OR: Browser WebAssembly.instantiate() with          │    │
    │  │   jco-transpiled core modules                         │    │
    │  └───────────────────────────────────────────────────────┘    │
    │                                                                │
    │  Implements ClientBackend by delegating to WASM component:    │
    │    authenticate() → call_authenticate() in WASM               │
    │    get_servers()  → call_get_servers() in WASM                │
    │    poll_event()   → call_poll_event() in WASM                 │
    │    ...                                                        │
    │                                                                │
    │  Provides host imports (the "syscalls"):                      │
    │    http_request()      → reqwest                              │
    │    websocket_connect() → tokio-tungstenite                    │
    │    websocket_send()    → write to stored connection           │
    │    websocket_recv()    → read from stored connection          │
    │    storage_get/set()   → SurrealKV (namespaced per plugin)   │
    │    log()               → tracing                              │
    └────────────────────────────────────────────────────────────────┘
                                     │
                            implements Arc<dyn ClientBackend>
                                     ▼
                        ┌────────────────────────┐
                        │    ClientManager        │
                        │  (unchanged — holds     │
                        │   BackendHandle per     │
                        │   account, doesn't care │
                        │   if native or WASM)    │
                        └────────────────────────┘
```

---

## 3. Technology Choices

| Component | Choice | Version | Rationale |
|---|---|---|---|
| **WASM runtime** | wasmtime | 42.x | Reference Component Model implementation, full async, best WIT support |
| **Interface definition** | WIT (WebAssembly Interface Types) | Component Model spec | Standard, typed, future-proof — maps 1:1 to poly-client types |
| **Guest bindings** | wit-bindgen | 0.53.x | Official WIT → Rust guest code generator |
| **Host bindings** | wasmtime::component::bindgen! | (part of wasmtime) | Generates typed Rust wrappers for host-side |
| **Plugin compilation** | cargo-component | 0.21.x | Builds Rust → WASM Component (wasm32-wasip2) |
| **Plugin target** | wasm32-wasip2 | rustup target | WASI Preview 2 + Component Model |
| **Web strategy** | Server-hosted plugins | — | Axum server runs wasmtime on behalf of browser |
| **Mobile strategy** | AOT precompilation | — | engine.precompile_component() → .cwasm cache |

---

## 4. The WIT Interface

The WIT file defines the contract between host and guest. It is the **single source of truth** for
the plugin interface. Located at `wit/messenger-plugin.wit` in the workspace root.

### 4.1 Host Imports (syscalls — host provides to plugin)

| Function | Purpose | Host Implementation |
|---|---|---|
| `http-request` | Make HTTP requests | `reqwest` |
| `websocket-connect` | Open WebSocket | `tokio-tungstenite` |
| `websocket-send` | Send on WebSocket | Write to stored connection |
| `websocket-recv` | Receive from WebSocket | Read from stored connection |
| `websocket-close` | Close WebSocket | Drop stored connection |
| `storage-get` | Key-value read | SurrealKV (namespaced per plugin) |
| `storage-set` | Key-value write | SurrealKV (namespaced per plugin) |
| `storage-delete` | Key-value delete | SurrealKV (namespaced per plugin) |
| `log` | Structured logging | `tracing` |
| `get-current-time` | Wall clock time | `std::time` / WASI clocks |

### 4.2 Guest Exports (plugin provides to host)

These mirror the `ClientBackend` trait exactly:

| Function | Returns | Maps to |
|---|---|---|
| `authenticate` | `result<session, client-error>` | `ClientBackend::authenticate()` |
| `logout` | `result<_, client-error>` | `ClientBackend::logout()` |
| `is-authenticated` | `bool` | `ClientBackend::is_authenticated()` |
| `get-servers` | `result<list<server>, client-error>` | `ClientBackend::get_servers()` |
| `get-server` | `result<server, client-error>` | `ClientBackend::get_server()` |
| `get-channels` | `result<list<channel>, client-error>` | `ClientBackend::get_channels()` |
| `get-channel` | `result<channel, client-error>` | `ClientBackend::get_channel()` |
| `send-message` | `result<message, client-error>` | `ClientBackend::send_message()` |
| `get-messages` | `result<list<message>, client-error>` | `ClientBackend::get_messages()` |
| `get-user` | `result<user, client-error>` | `ClientBackend::get_user()` |
| `get-friends` | `result<list<user>, client-error>` | `ClientBackend::get_friends()` |
| `get-channel-members` | `result<list<user>, client-error>` | `ClientBackend::get_channel_members()` |
| `get-groups` | `result<list<group>, client-error>` | `ClientBackend::get_groups()` |
| `get-dm-channels` | `result<list<dm-channel>, client-error>` | `ClientBackend::get_dm_channels()` |
| `get-notifications` | `result<list<notification>, client-error>` | `ClientBackend::get_notifications()` |
| `get-voice-participants` | `result<list<voice-participant>, client-error>` | `ClientBackend::get_voice_participants()` |
| `get-presence` | `result<presence-status, client-error>` | `ClientBackend::get_presence()` |
| `set-presence` | `result<_, client-error>` | `ClientBackend::set_presence()` |
| `poll-event` | `option<client-event>` | Replaces `event_stream()` |
| `backend-type` | `backend-type-enum` | `ClientBackend::backend_type()` |
| `backend-name` | `string` | `ClientBackend::backend_name()` |

### 4.3 Event Streaming Strategy

The `ClientBackend::event_stream()` method returns a `Pin<Box<dyn Stream>>` — this cannot cross
the WASM boundary directly. Instead:

- **Guest exports** `poll-event() → option<client-event>` — returns the next pending event or None
- **Host drives** the event loop: calls `poll_event()` periodically from an async task
- **WebSocket data** is buffered host-side; the plugin calls `websocket-recv()` (a host import)
  to pull data and process it internally, then queues events for `poll-event` to return
- **Alternative** (future): When WIT async stabilizes, migrate to `wait-for-event()` blocking call
  that suspends the WASM fiber until data arrives

---

## 5. Plugin Location & Loading

### 5.1 Where plugins live

| Source | Path | Use Case |
|---|---|---|
| **Built-in** | Embedded in binary via `include_bytes!` | Shipped with the app (demo, poly-server) |
| **Local directory** | `~/.poly/plugins/*.wasm` | User-installed plugins |
| **Downloaded** | Via HTTPS from plugin registry | Future: community plugin marketplace |

### 5.2 Plugin discovery

At startup, the plugin host:
1. Loads any embedded (built-in) plugins
2. Scans `~/.poly/plugins/` for `.wasm` files
3. Validates each plugin against the WIT world (wasmtime verifies exports/imports at load time)
4. Registers available plugin types in a `PluginRegistry`

### 5.3 Plugin instantiation

When the user adds an account:
1. They select a backend type (from available plugins)
2. The plugin host instantiates a new WASM component with a fresh `Store`
3. The host provides import implementations (HTTP, WS, storage — sandboxed per-instance)
4. The plugin is ready to authenticate

---

## 6. Security Model

### 6.1 Sandboxing guarantees

- **No filesystem access**: Plugins cannot read/write any files
- **No raw network access**: All HTTP/WS goes through host-mediated imports
- **URL allowlisting**: Host can restrict which URLs a plugin can connect to
- **Storage isolation**: Each plugin instance gets a namespaced KV prefix
- **Memory isolation**: Each WASM instance has its own linear memory — no cross-plugin access
- **Resource limits**: Fuel metering / epoch interruption to prevent infinite loops
- **No host memory access**: WASM sandbox prevents reading host memory

### 6.2 Trust levels

| Level | Description | Example |
|---|---|---|
| **Built-in** | Ships with the app, fully trusted | Demo, Poly Server |
| **Verified** | Signed by Poly maintainers | Future: official Stoat/Matrix plugins |
| **Community** | User-installed, sandboxed | Third-party plugins |

---

## 7. Platform-Specific Considerations

### 7.1 Desktop (Linux, macOS, Windows)
- wasmtime runs natively, JIT compilation
- Fastest path — no special handling needed

### 7.2 Mobile — Android
- wasmtime's Cranelift supports aarch64
- JIT is allowed on Android
- First load: compile .wasm → native, cache result

### 7.3 Mobile — iOS
- **JIT is prohibited** by iOS
- Use wasmtime AOT: `engine.precompile_component()` → `.cwasm` file
- Pre-compile during build step OR on first load, cache to app sandbox
- Alternatively: ship pre-compiled `.cwasm` files per architecture

### 7.4 Web
- wasmtime cannot compile to wasm32-unknown-unknown (Cranelift cannot run in browser)
- **Strategy**: Server-side hosting — the Axum fullstack server runs wasmtime
- Browser communicates with plugins via the server (HTTP/WebSocket relay)
- This fits Poly's existing web architecture perfectly

---

## 8. Dependency Changes

### 8.1 poly-core gains
- `wasmtime` (native only, `cfg(not(target_arch = "wasm32"))`)
- `wasmtime-wasi` (native only)
- Plugin host module (`src/plugin_host/`)

### 8.2 poly-core loses (direct deps)
- `poly-demo` (optional dep removed)
- `poly-stoat` (optional dep removed)
- `poly-matrix` (optional dep removed)
- `poly-discord` (optional dep removed)
- `poly-teams` (optional dep removed)
- `poly-server-client` (moved to plugin)

### 8.3 Client crates gain
- `wit-bindgen` (guest-side WIT bindings)
- `crate-type = ["cdylib"]` (produces .wasm binary)
- `[package.metadata.component]` for cargo-component

### 8.4 Client crates lose
- `async-trait` (WIT functions are synchronous from guest perspective)
- `futures` (no `Stream` — replaced by `poll-event`)
- `tokio` (no async runtime in WASM guest — host handles async)
- `reqwest` / `tokio-tungstenite` (all I/O via host imports)
- `dioxus` (poly-demo currently depends on this — must be removed)

### 8.5 poly-client changes
- Remains as a Rust crate for type definitions and trait definition
- WIT file is the **canonical** interface; poly-client types must match WIT exactly
- New: `poly-client` may provide a `wit-guest` feature that re-exports wit-bindgen types

---

## 9. Feature Flag Changes

### Before (compile-time linking)
```toml
[features]
demo = ["dep:poly-demo"]
stoat = ["dep:poly-stoat"]
# etc.
```

### After (runtime plugin loading)
```toml
[features]
# Feature flags for EMBEDDING plugins in the binary (via include_bytes!)
embed-demo = []
embed-stoat = []
# etc.
# No dep: references — plugins are .wasm blobs, not Rust deps
```

The feature flags now control whether a `.wasm` binary is embedded in the final executable
(for built-in plugins), not whether a Rust crate is compiled in.

---

## 10. Migration Path

### Phase 1: WIT definition + plugin host skeleton
1. Write `wit/messenger-plugin.wit` mirroring `ClientBackend` + types
2. Create `PluginHost` struct that implements `ClientBackend` by delegating to wasmtime
3. Add wasmtime deps to poly-core (native-only)

### Phase 2: Convert demo client to WASM plugin (proof of concept)
1. Change poly-demo to `crate-type = ["cdylib"]`, target wasm32-wasip2
2. Remove dioxus/tokio/async-trait/futures deps, add wit-bindgen
3. Implement WIT Guest trait instead of ClientBackend
4. Replace direct HTTP/socket calls with host imports
5. Build with `cargo component build`
6. Load in plugin host, verify it works

### Phase 3: Convert all remaining clients
1. Repeat for stoat, matrix, discord, teams, server-client
2. Remove all `dep:poly-*` optional deps from poly-core
3. Use `include_bytes!` for built-in plugins

### Phase 4: Verification
1. Build all client `.wasm` files
2. Verify no cargo dependency links between any client crate and poly-core
3. Verify all clients load and function via plugin host
4. Test on desktop, web (server-hosted), mobile (AOT)
