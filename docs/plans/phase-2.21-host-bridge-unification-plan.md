# Phase 2.21 — Host-bridge unification & storage over bridge

> **2026-04-12 addendum:** steps 6–9 have been superseded by the
> fullstack pivot below. The per-shell axum listeners on port 9333
> were removed; every UI crate is now a Dioxus fullstack app whose
> server half merges `poly_host::router(state)` into the Dioxus router
> and binds to the **same port as the WASM bundle** (3000 / 3001 /
> 3002). One process, one port per shell. The standalone
> `apps/poly-host` daemon still exists for browser-only use of
> `apps/web` without fullstack, and its library (`HostState`, `router`,
> `resolve_data_dir`) is what all the fullstack server binaries call
> into. See the "Fullstack pivot" section at the bottom of this file.

> Status: in progress — core refactor landing incrementally.
> Driver: poly-web is a single-user dev shell but ships with an isolated
> IndexedDB silo, making its accounts / settings invisible to native
> tooling (MCP servers, CLI, external scripts) and inconsistent with the
> SQLite / SurrealKV backends used by every other platform.

## Why this exists

Poly runs on five native shells (`apps/desktop`, `apps/desktop-electron`,
`apps/web` + browser, iOS, Android) plus a wasmtime plugin sandbox. Each
one has grown its own answer to "how does WASM code reach the outside
world," and the answers have drifted:

| Capability               | apps/desktop  | apps/desktop-electron | apps/web (browser) |
|--------------------------|---------------|-----------------------|--------------------|
| Subprocess (`exec`)      | `/host` POST  | `/host` POST          | **n/a**            |
| Raw HTTP (`http`)        | `/host` POST  | `/host` POST          | `fetch` fallback   |
| KV storage               | SQLite disk   | SQLite disk           | **IndexedDB silo** |
| Plugin KV storage        | **in-memory** | **in-memory**         | **in-memory**      |
| Bridge port              | `9223` (shared w/ MCP) | `9333` | none         |

Problems:

1. **apps/web storage is a fourth silo** — invisible to MCP, CLI, and
   the native SQLite file every other platform writes to. Accounts added
   via poly-web's Test Accounts panel don't surface anywhere useful.
2. **Plugin KV is a `HashMap`** — never persisted, even on native. Plugins
   that try to cache auth, remember last-read markers, or store anything
   between launches silently lose it.
3. **Ports disagree** — `poly_host_bridge::BRIDGE_URL` says `9333`, but
   `apps/desktop-web` actually mounts `/host` on `9223`. A bridge call
   from inside the Wry webview hits the wrong port and 404s.
4. **Single-route dispatch** — the bridge uses one `POST /host`
   tagged-union route. Easy to grow wrong (new variants keep piling into
   one enum) and awkward to debug (every request looks identical on the
   wire). The user's ask is every host operation lives under `/host/…`
   with a clean sub-route per category.
5. **No per-account plugin storage** — the WIT defines plugin-scoped
   storage but not account-scoped. A plugin that serves multiple
   accounts currently has to encode account IDs into key strings.

## Design

### Canonical bridge layout

All host operations live under `/host/<category>/<op>`:

```
POST /host/exec                      — spawn subprocess
POST /host/http                      — one-shot HTTP via native reqwest
POST /host/kv/get                    — app KV get
POST /host/kv/set                    — app KV set
POST /host/kv/delete                 — app KV delete
POST /host/kv/clear                  — app KV clear_all
POST /host/plugin-kv/get             — plugin KV get (body: {plugin, account?, key})
POST /host/plugin-kv/set             — plugin KV set
POST /host/plugin-kv/delete          — plugin KV delete
GET  /host/status                    — liveness ping (no body)
POST /host                           — LEGACY single-dispatch, kept for
                                       one cycle so old WASM builds still work
```

Port: **9333** everywhere. `apps/desktop-web` adds a second axum listener
on `9333` (keeping `9223` for the MCP eval bridge, unchanged). The
legacy `/host` POST route on port `9223` stays for now and will be
removed in a follow-up.

### Protocol wire shape

`HostCall` gains new variants. The tagged union is still what the legacy
`POST /host` endpoint accepts; new sub-routes take a flat body instead.

```rust
enum HostCall {
    ExecCommand { program, args },
    HttpRequest { method, url, headers, body_b64 },
    // NEW:
    KvGet    { key },
    KvSet    { key, value_json },
    KvDelete { key },
    KvClear,
    PluginKvGet    { plugin: String, account: Option<String>, key: String },
    PluginKvSet    { plugin, account, key, value_b64 },
    PluginKvDelete { plugin, account, key },
}

enum HostOk {
    ExecOutput   { … },
    HttpResponse { … },
    // NEW:
    KvValue        { value_json: Option<Value> },
    PluginKvValue  { value_b64:  Option<String> },
    Void,
}
```

### Storage trait split

Today `crates/core/src/storage/mod.rs` picks one of three `StorageInner`
implementations at compile time via `cfg`. We add a fourth:

```
native        — SQLite on disk (unchanged)
native-surreal — SurrealKV (unchanged, opt-in)
web           — IndexedDB (kept for browsers that can't reach the bridge)
host-bridge   — NEW: every get/set/delete routes through
                poly_host_bridge::Client to POST /host/kv/*
```

Selection precedence:

1. If `feature = "storage-host-bridge"` is enabled, use the bridge.
2. Else on `wasm32` use the IndexedDB backend.
3. Else use SQLite (or SurrealKV if `storage-surreal`).

`apps/web/Cargo.toml` flips this on. Result: every `Storage::get/set`
call the UI makes (accounts, theme, favorites, last-visited routes,
plugin settings) travels to the native daemon and lands in the same
SQLite file the desktop shell uses.

### The `poly-host` daemon

New crate `apps/poly-host` — a tiny standalone binary that:

- Binds `127.0.0.1:9333`.
- Serves the full `/host/…` route set.
- Owns the canonical SQLite file at `$XDG_DATA_HOME/poly/poly.db`
  (same path apps/desktop uses) so **one file is shared across every
  shell that talks to the bridge**.
- Is the only piece the user needs to run alongside `dx serve
  --platform web` to get real storage in poly-web.

On desktop and electron, the existing native shells already own the
storage directly — they don't need to run `poly-host`; they just
expose the same routes from within their own process.

### Plugin storage

The in-memory `HashMap` in `plugin-host/src/host_impl.rs` becomes a
`Box<dyn PluginStorageBackend>` handle, with two implementations:

- `InMemoryPluginStorage` (kept for tests).
- `BridgePluginStorage` that proxies to `/host/plugin-kv/*`.

WIT gains an optional `account` parameter:

```wit
storage-get:    func(key: string) -> option<list<u8>>;                // plugin scope
storage-set:    func(key: string, value: list<u8>) -> result<_, string>;
storage-delete: func(key: string) -> result<_, string>;

// NEW — explicit per-account scope. Keys are disjoint from the
// plugin-global store above.
account-storage-get:    func(account: string, key: string) -> option<list<u8>>;
account-storage-set:    func(account: string, key: string, value: list<u8>) -> result<_, string>;
account-storage-delete: func(account: string, key: string) -> result<_, string>;
```

This is additive — existing plugins don't have to care. The
`BridgePluginStorage` implementation namespaces keys on the daemon side:
`plugin:{plugin_id}:{scope}:{key}` where `scope` is either `global` or
`account:{account_id}`.

## Rollout

| Step | Scope                                                       | State |
|------|-------------------------------------------------------------|-------|
| 1    | Plan doc (this file)                                        | ✅    |
| 2    | Extend protocol with KV request/response payloads           | ✅    |
| 3    | Add multi-route `/host/<cat>/<op>` dispatcher               | ✅    |
| 4    | Add `storage-host-bridge` backend in core                   | ✅    |
| 5    | Wire `apps/web` feature flag                                | ✅    |
| 6    | Update `apps/desktop-web` to serve the new routes on 9333   | ✅    |
| 7    | Update `apps/desktop-electron-web/electron/host_bridge.js`  | ✅    |
| 8    | New `apps/poly-host` daemon (lib + bin, reused by Wry shell)| ✅    |
| 9    | Update CLAUDE.md + READMEs with port/run-order              | ✅    |
| 10   | Persistent plugin storage (native + bridge impls)           | 🟦 follow-up |
| 11   | WIT `account-storage-*` functions                           | 🟦 follow-up |
| 12   | Unified SQLite backend for Electron (replace JSON file)     | ✅ (via fullstack pivot — Electron's server half uses `poly_host::HostState`) |
| 13   | Fullstack pivot: one port per shell, no 9333 sidecar        | ✅ |

## Fullstack pivot (2026-04-12)

The original plan mounted a second axum listener on loopback **9333**
inside every native shell. Feedback during rollout: "one port, one
process — not a separate sidecar." We switched the three UI crates to
Dioxus fullstack:

- `apps/web`, `apps/desktop-electron`, `apps/desktop` each compile into
  **both** a WASM client and a native axum server from the same
  `src/main.rs`, gated by `target_arch` + the `server` feature.
- The server branch constructs `poly_host::HostState`, then merges
  `poly_host::router(state)` into the Dioxus router before binding via
  `dioxus_cli_config::fullstack_address_or_localhost()`. Result: the
  same port that serves `/assets/…` and `/` also serves `/host/*`.
- `apps/desktop-web` (Wry dev shell) dropped its 9333 listener and its
  `poly-host` dependency. Eval bridge on 9223 stays. The Wry webview
  reaches `/host/*` via the `apps/desktop` fullstack server on 3002.
- `apps/desktop-electron-web/electron/host_bridge.js` was deleted. The
  JSON-file KV backend is gone. Electron's renderer now talks to the
  Rust fullstack server (`poly-desktop-electron` bin with `server`
  feature) on 3001, which owns the shared SQLite file.

### Exact dx invocation

dx fullstack requires per-half platform + feature sets. The client
half builds for `wasm32-unknown-unknown`; without `@server --platform
server` dx tries to build the server for wasm32 too and fails.

```bash
dx serve --platform web --fullstack \
  @client --no-default-features --features "dev-plugins,web" \
  @server --platform server --no-default-features --features "dev-plugins,server"
```

### Feature gating pattern

Each UI crate uses the same layout in `Cargo.toml`:

```toml
[features]
default = ["dev-plugins", "web"]                     # NOT server
web    = ["dioxus/web"]
server = [
  "dioxus/fullstack", "dioxus/server",
  "dep:axum", "dep:tokio", "dep:poly-host",
  "dep:dioxus-cli-config", "dep:anyhow",
]

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
axum             = { workspace = true, optional = true }
tokio            = { workspace = true, optional = true, features = [...] }
poly-host        = { workspace = true, optional = true }
dioxus-cli-config = { workspace = true, optional = true }
anyhow           = { workspace = true, optional = true }
```

Keeping `server` OUT of `default` is load-bearing: with it in default,
cargo's feature unification pulls the native deps into the wasm build
and the WASM compile fails on `mio` / `socket2` / `getrandom`.

### Port layout

| Shell | Fullstack bundle+bridge port | Debug/eval port |
|-------|------------------------------|-----------------|
| apps/web                | 3000 | 9222 (CDP) |
| apps/desktop-electron   | 3001 | 9224 (CDP) |
| apps/desktop            | 3002 | 9223 (HTTP eval) |
| apps/poly-host (daemon) | 9333 | — |

The standalone `apps/poly-host` daemon still exists for the case where
a user runs `apps/web` as a pure static WASM bundle (no fullstack) and
needs the bridge alongside it.

## Open questions

- **Legacy cleanup timeline.** We keep `POST /host` (legacy single
  dispatch) and port 9223's `/host` route for one release so old WASM
  builds don't regress. Remove both in phase 2.22.
- **Schema migrations.** The daemon owns the SQLite file that
  `apps/desktop` also owns. First-run migrations must be idempotent and
  the daemon must not hold an exclusive lock that blocks the desktop
  shell (and vice versa). WAL mode + short-lived connections per request
  is the intended answer.
- **Authentication.** The bridge is loopback-only, but any local process
  can hit `127.0.0.1:9333`. Follow-up: token in `~/.config/poly/bridge.token`
  required via `X-Poly-Token` header. Tracked as phase 2.22.
- **CORS for standalone poly-web.** `apps/web` is served from
  `http://127.0.0.1:3000` and will cross-origin to `9333`. Every shell
  that mounts `/host/*` must set permissive CORS (Wry / Electron / new
  daemon all already do this for the legacy `/host` route).
