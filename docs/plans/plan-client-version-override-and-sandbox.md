# Plan: Client Version Override + Per-Mechanism Toggles + Sandbox Host-Cap Stub

## Status: 🚧 IN PROGRESS — Phases A+B shipped; C-J pending + J shipped

> Sibling future plan referenced from Phase I:
> `docs/plans/plan-host-sandbox-impl.md` (stub written in Phase I.5).

---

## Problem

When a backend's wire protocol checks an "advertised client version"
(Discord User-Agent + super-properties; Matrix `User-Agent`; Teams
client-version headers; GitHub `User-Agent`; Forgejo `User-Agent`) and
the upstream tightens the accepted set, our plugins break until the
next plugin/app release. A user reporting "Discord stopped working"
has no way to bump the advertised string themselves; they must wait for
us to ship.

**Codebase reality check (audited 2026-04-30):** today *no* backend
sends a User-Agent or any version-advertising header
(`grep -rn "User-Agent" clients/*/src/` returns zero hits across all
ten plugins). The `poly_host_bridge::http::HttpClientBuilder`
**already** supports `.user_agent(ua)` — it just isn't called. So
Phase B is the first time these headers exist on the wire, *and* the
override is wired the same day.

Likewise, mechanisms inside a backend that the upstream gates on
(Discord captcha-sandbox, Matrix sliding-sync, …) are currently
hardcoded per plugin. The user can't toggle them without a plugin
rebuild.

This plan adds:

1. A user-facing **per-backend version override** (toggle + custom
   string) that propagates to the wire on every outbound request.
2. A user-facing **per-backend mechanism toggle** list, discovered via
   the WIT (so each plugin owns its own mechanism inventory).
3. The **WIT API surface** so the host, MCP, and CLI can list and
   toggle these without backend-specific code.
4. The **MCP tool family** (`client_settings_*`) that mirrors the
   `meta_persona_*` shape, so `poly-cli` and Claude can drive the
   toggles end-to-end.
5. A **test pyramid** (unit / mock-server-log / e2e / Playwright) per
   backend, so a regression in any one backend's version-propagation
   wire path is caught before merge.
6. A **sandbox host-cap stub** — the trait + WIT host-cap declaration
   are landed; the actual sandbox impl returns `Err(NotImplemented)`
   and is deferred to `plan-host-sandbox-impl.md`.

---

## Non-goals

- Implementing the sandbox itself (Phase I lands the stub only).
- Per-account overrides (version is **per-backend in v1**; per-account
  is future work — see the explicit pin in D2).
- Breaking the WIT for plugins built against the older interface — all
  new methods carry default impls so existing plugins still load.
- Adding fields to existing MCP tools — `client_settings_*` is its own
  family.
- Refactoring the existing `client-settings` WIT interface — the new
  surface lives in a new `client-config` WIT interface (justified
  below) so the storage-vs-config split stays clean.
- Inventing mechanisms for backends whose code-paths don't yet exist.
  Matrix sliding-sync, Teams browser-shim, HN realtime: zero plumbing
  in tree (`grep` audit per backend below). v1 ships only mechanisms
  that flip a code path that **already exists**; everything else is
  Phase K (a sibling future plan).

---

## Design decisions

### D1 — Two namespaces: "settings" (plugin-defined fields) vs "config" (host-defined overrides)

`client-settings` (existing WIT interface, `wit/messenger-plugin.wit`
lines 915–995) handles **plugin-declared** settings — the plugin
exposes a schema (`get-settings-sections`) and the host renders it.
This is for things like Discord's "show TTS messages" toggle that only
the plugin understands.

The version-override and mechanism-toggle surface is **host-defined**
and uniform across backends — every backend has a "client version" and
zero-or-more mechanisms, regardless of what its plugin-specific
settings look like. Putting them in `client-settings` would conflate
two responsibilities (Single Responsibility Principle, see
CLAUDE.md "Design Principles — SOLID").

**Decision:** add a new `client-config` interface in the WIT, exported
by the `messenger-plugin` world. Default impls let pre-existing
plugins return empty lists / pass-through.

### D2 — Storage namespace + per-account future-work pin

User-tunable overrides persist in `poly_kv` (the host-side KV table)
under host-owned keys, NOT plugin-owned `storage-*` keys. The plugin
NEVER writes these — it only reads its effective version via
`get-client-version()` and learns its mechanism state via
`get-client-mechanisms()`. This keeps the host as the source of truth
and means a buggy plugin can't lie to the user about what it's
advertising.

Key schema:
- `client.config.<backend_id>.version_override` → JSON string or `null`
- `client.config.<backend_id>.mechanism.<mechanism_id>` → JSON bool

`<backend_id>` is the BackendId slug from
`clients/client/src/types.rs::BackendId` — current values:
`demo`, `demo_forum`, `discord`, `forgejo`, `github`, `hackernews`,
`lemmy`, `matrix`, `poly`, `stoat`, `teams`. Verification:
`grep -rn "BackendId::new\|client.config\." crates/host-bridge/src/`
returns zero hits today, confirming the `client.config.` prefix is
disjoint from existing namespaces.

**Per-account future-work pin (resolved):** v1 is **per-backend
only**. The `<backend_id>` slug in the key is intentionally NOT
`<backend_id>:<account_id>` so a future per-account refinement adds a
suffix without colliding. Future plan
`plan-client-config-per-account.md` (NOT WRITTEN — out of scope) would
add `client.config.<backend_id>.<account_id>.version_override`
fallback-walked before the per-backend key.

### D3 — WIT extension shape (concrete; verified against `wit/messenger-plugin.wit`)

New `client-config` interface to add in `wit/messenger-plugin.wit`
**after the existing `client-settings` block (line 995)** and
**before the `client-sidebar` block (line 1002)**, exported by the
`messenger-plugin` world (line 1540+ — append to the `export` block at
line 1545 immediately after `export client-settings;`).

`client-error` is defined in the `types` interface at
`wit/messenger-plugin.wit:495-503` (variant with `auth-failed`,
`network`, `not-found`, `rate-limited`, `permission-denied`,
`internal`, `not-supported`). The new interface re-uses it via
`use types.{client-error};` exactly as the eight existing interfaces
already do.

```wit
/// Host-defined, uniform-across-backends client configuration.
///
/// Distinct from `client-settings` which is plugin-defined per-plugin
/// schema. This interface covers the version string the plugin
/// advertises on the wire and any backend-specific "mechanisms" the
/// plugin supports (captcha-sandbox, sliding-sync, browser-shim, …).
///
/// All methods carry default impls in the Rust trait so existing
/// plugins compiled against the old WIT continue to load — they
/// surface as "version: <plugin default>" and "mechanisms: []".
interface client-config {
    use types.{client-error};

    /// Optional host capability a mechanism may require to function.
    ///
    /// When a mechanism declares `requires-host-cap = some(sandbox-browser)`
    /// and the host doesn't advertise that cap, the UI MUST disable
    /// the toggle and the plugin MUST treat the mechanism as off.
    variant host-cap {
        /// Open a sub-browser the user can interact with for
        /// challenges (Discord captcha, OAuth flows). Stub in v1.
        sandbox-browser,
        /// Native system tray icon (future).
        system-tray,
        /// OS-level notifications (future).
        os-notifications,
    }

    /// One toggleable "mechanism" the plugin supports — a named code
    /// path the user can opt into or out of. IDs are plugin-stable
    /// (the plugin is free to define them) but should be kebab-case.
    record mechanism {
        /// Stable ID. Storage key suffix.
        /// Example: "captcha-sandbox", "sliding-sync", "browser-shim".
        id: string,
        /// FTL key for the human-readable label.
        /// Example: "plugin-discord-mechanism-captcha-sandbox-label".
        name-key: string,
        /// Current on/off state — already merged with the host-stored
        /// override; the plugin returns the effective value.
        enabled: bool,
        /// If `some`, the mechanism only functions when the host
        /// advertises the matching capability. The UI disables the
        /// toggle when the cap is absent.
        requires-host-cap: option<host-cap>,
        /// Optional FTL key for a longer description shown on hover.
        description-key: option<string>,
    }

    /// Return the version string the plugin will advertise on the
    /// next outbound request. After the host sets an override via
    /// `set-client-version-override(some(...))`, this MUST return
    /// the override string. With no override set, this returns the
    /// plugin's hardcoded default.
    get-client-version: func() -> string;

    /// Set or clear the version override. `none` clears.
    /// The host stores the override in `poly_kv` and re-injects it
    /// into the plugin via `host-api.storage-get` on next plugin
    /// load; the plugin reads it on init and merges over its default.
    set-client-version-override: func(
        override: option<string>,
    ) -> result<_, client-error>;

    /// Return the full mechanism inventory for this backend. Empty
    /// list is legal (most backends in v1).
    get-client-mechanisms: func() -> result<list<mechanism>, client-error>;

    /// Toggle one mechanism on or off by ID. Plugin persists via
    /// `host-api.storage-set` under its own KV namespace (mirror of
    /// the host-owned `client.config.<id>.mechanism.<m>` key) and
    /// returns the new state via `get-client-mechanisms`.
    set-client-mechanism: func(
        id: string,
        enabled: bool,
    ) -> result<_, client-error>;
}
```

Compile-syntax check passes against the existing WIT package: every
`record` / `variant` / `func` shape mirrors the existing
`client-settings` interface's idioms; `option<T>` / `result<_, E>` /
`list<T>` / `string` / `bool` are all in current use elsewhere in the
file.

### D4 — MCP tool family

Mirror the `meta_persona_*` shape from `mcp/chat-mcp/src/tools.rs`.
Five new tools:

| Tool name | Args | Returns | Audit? |
|---|---|---|---|
| `client_settings_list` | `backend_id?: string` | List of `{backend_id, version, version_override, mechanisms[]}` for one or all backends | no (read-only) |
| `client_settings_get_version` | `backend_id: string` | `{version, override_active: bool, default_version: string}` | no |
| `client_settings_set_version_override` | `backend_id: string, override?: string` | Success or `client-error` | **yes** |
| `client_settings_list_mechanisms` | `backend_id: string` | List of `mechanism` records | no |
| `client_settings_set_mechanism` | `backend_id: string, mechanism_id: string, enabled: bool` | Success or `client-error` | **yes** |

**Audit row format** (every `set_*` writes one — Phase Q lint
`tools/scripts/forbid-unaudited-persona-tool.sh` will be extended in
Phase D.6 to cover `client_settings_set_*` exactly the same way):

```rust
mem.record_persona_audit(
    "system",                                 // persona_slug — synthetic "system" actor
    "claude-desktop",                         // actor (matches meta_persona_* convention)
    "client_settings_set_version_override",   // action
    None,                                     // target_account
    None,                                     // target_chat
    Some(&serde_json::json!({
        "backend_id": backend_id,
        "override":   override_value,         // null when clearing
    }).to_string()),                          // payload
    "ok",                                     // result ("ok" / "error")
    None,                                     // error_msg (Some(...) on failure)
)?;
```

The "system" persona slug avoids per-persona attribution since these
are app-wide settings; the Phase T `meta_persona_audit_query` tool
already filters by `slug?` so `--slug=system` returns the
client_settings audit history.

### D5 — Per-backend version-source table (audited 2026-04-30)

Today's reality across all 10 backends: **no header is set anywhere**
(`grep` on `User-Agent`/`X-Super-Properties`/`x-ms-client-version`
across `clients/*/src/` returns zero matches). All HTTP goes through
`poly_host_bridge::http::HttpClient` constructed with `::new()`
(no UA). Phase B is the first time these headers exist; the override
ships the same day.

| Backend | Slug | Today's version-source | Phase B target (header on every outbound) | Helper site |
|---|---|---|---|---|
| Demo | `demo` | none (in-memory only) | none — no wire | n/a |
| `demo_forum` | `demo_forum` | none | none — no wire | n/a |
| Discord | `discord` | none | `User-Agent: <override-or-default>` (+ `X-Super-Properties: <b64>` when `super-properties` mechanism on) | `clients/discord/src/http.rs::DiscordHttpClient::new` (line 21) |
| Forgejo | `forgejo` | none | `User-Agent: <override-or-default>` | `clients/forgejo/src/api.rs::new` (line 33) |
| GitHub | `github` | none (some `gh` shell-out, separate path) | `User-Agent: <override-or-default>` on the `HttpClient` path; `gh` shell-out unchanged | `clients/github/src/api.rs:230,443` (HttpClient construction sites) |
| HackerNews | `hackernews` | none | `User-Agent: <override-or-default>` | `clients/hackernews/src/api.rs::with_base_url` (line 30) |
| Lemmy | `lemmy` | none | `User-Agent: <override-or-default>` | `clients/lemmy/src/api.rs` (HttpClient site) |
| Matrix | `matrix` | none | `User-Agent: <override-or-default>` | `clients/matrix/src/http.rs` (HttpClient site) |
| Poly-server | `poly` | none | `User-Agent: <override-or-default>` (internal but useful for server logs) | `clients/server-client/src/http.rs` |
| Stoat | `stoat` | none | `User-Agent: <override-or-default>` | `clients/stoat/src/http.rs` (line 52) |
| Teams | `teams` | none | `User-Agent: <override-or-default>` | `clients/teams/src/http.rs::TeamsHttpClient::new` (line 70) |

Pattern: every backend's `HttpClient::new()` is replaced with
`HttpClientBuilder::new().user_agent(version_string).build()?`. The
builder API at `crates/host-bridge/src/http.rs:226` already does the
right thing on both transports (direct `reqwest` UA on native; UA
piggybacked in the bridge wire payload on WASM).

For Discord the `super-properties` mechanism additionally injects an
`X-Super-Properties` header in `apply_version_headers(req)` — the
helper added to `clients/discord/src/http.rs` per Phase B.4.

### D6 — Per-backend mechanism inventory (v1, plumbing-verified)

Only mechanisms whose code path **already exists** ship in v1.
Mechanisms that would require new plumbing are deferred to Phase K
(separate plan). Default state shown for each.

| Backend | v1 mechanisms | Default | Plumbing status (audited) | Rationale |
|---|---|---|---|---|
| `demo` / `demo_forum` | none | — | n/a | Reference impl; no wire |
| `discord` | `super-properties` | **off** | New (added in Phase B.4 `apply_version_headers`) | Off by default because real-Discord servers may flag fresh accounts that suddenly start sending it; user opts in if they need real-Discord parity |
| `discord` | `captcha-sandbox` | **off** (host-cap absent) | Stub only — Phase I `StubSandbox` returns `NotImplemented` | "Honeypot" toggle that lets us land the UI + audit trail before the sandbox plumbing exists. UI renders disabled-with-tooltip when host-cap absent |
| `forgejo` | none | — | — | Plain REST + token; nothing to toggle |
| `github` | none | — | — | Same; `gh` shell-out vs HTTP is auto-detected by token type |
| `hackernews` | none | — | Firebase-realtime is the *only* code path today; toggling to "plain REST polling" would require new plumbing → defer to Phase K | The plan's draft mention of `firebase-realtime` was incorrect — toggle is impossible without writing the alternate path first |
| `lemmy` | none | — | — | |
| `matrix` | none | — | Sliding-sync code path **does not exist** in tree (grep `clients/matrix/src/` for `sliding` returns one comment in `guest.rs` referencing the v3 `/sync` long-poll). Defer to Phase K | The plan's draft mention of `sliding-sync` was aspirational; ships as a follow-up plan |
| `poly` (server-client) | none | — | — | Internal protocol |
| `stoat` | none | — | — | Local-only test backend |
| `teams` | none | — | "Browser-shim" and "Graph-fallback" don't exist as code paths (Teams uses Graph as its **only** backend, see `clients/teams/src/lib.rs:87` `DEFAULT_BASE_URL = "https://graph.microsoft.com"`). Defer to Phase K | Plan's draft mechanism list was wrong — there's only one path today |

**Net for v1:** the only backend with a real mechanism toggle is
Discord (`super-properties` enables an existing-in-Phase-B header
flip; `captcha-sandbox` is the honeypot for the sandbox stub). Every
other backend ships with `mechanisms: []`. The version-override
surface is uniform across all 8 wire-bearing backends regardless.

### D7 — Sandbox host-cap stub

The `host-cap::sandbox-browser` variant lands in WIT in Phase A. The
host-side trait + impl-stub lands in Phase I, in a new crate
`crates/host-sandbox/`:

```rust
// crates/host-sandbox/src/lib.rs

// FUTURE: docs/plans/plan-host-sandbox-impl.md
//
// This crate ships only the trait + a stub impl that errors. The
// real sub-browser plumbing (Wry inner view / Electron BrowserWindow /
// Web popup) is deferred to plan-host-sandbox-impl.md.

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("sandbox not implemented yet — see plan-host-sandbox-impl.md")]
    NotImplemented,
    #[error("sandbox capability disabled by user")]
    Disabled,
    #[error("sandbox aborted by user")]
    UserCancelled,
}

pub struct SandboxResult {
    /// Final URL the sandbox navigated to (the one matching
    /// `capture_url_pattern`). The plugin parses query/fragment
    /// for the challenge token.
    pub captured_url: String,
    /// Cookies the sandboxed browser collected, scoped to the
    /// origin. Plugin replays these on its next API call.
    pub captured_cookies: Vec<(String, String)>,
}

#[async_trait::async_trait]
pub trait HostSandbox: Send + Sync {
    /// Open `url` in a sub-browser (popup window / OS WebView /
    /// Wry inner view); intercept navigation; resolve when the
    /// browser navigates to a URL matching `capture_url_pattern`
    /// (a glob — `https://discord.com/captcha/success?*`).
    ///
    /// V1 stub: returns `Err(SandboxError::NotImplemented)`
    /// immediately. Real impl deferred to plan-host-sandbox-impl.md.
    async fn open_browser_sandbox(
        &self,
        url: String,
        capture_url_pattern: String,
    ) -> Result<SandboxResult, SandboxError>;
}

pub struct StubSandbox;

#[async_trait::async_trait]
impl HostSandbox for StubSandbox {
    async fn open_browser_sandbox(
        &self,
        _url: String,
        _capture_url_pattern: String,
    ) -> Result<SandboxResult, SandboxError> {
        Err(SandboxError::NotImplemented)
    }
}

/// Capability registry — what host caps are advertised to the WIT
/// `client-config.host-cap` enum at runtime. v1 returns empty so
/// `sandbox-browser` is **NOT** advertised (UI renders Discord
/// `captcha-sandbox` toggle as disabled-with-tooltip).
pub fn advertised_host_caps() -> &'static [&'static str] {
    &[]
}
```

**Backends that need the host-cap in v1:** Discord only
(captcha-sandbox). Confirmed — no other backend declares
`requires-host-cap = some(...)` in its mechanism inventory.

### D8 — Test strategy (per-backend, four layers)

For every backend that has a wire protocol (i.e. all except `demo`,
`demo_forum`, `stoat`, `poly` server-client which is internal-only),
the test pyramid is:

1. **Unit** (`clients/<backend>/tests/version_override.rs`) — build a
   client with a custom version, call any HTTP-bearing method against
   a `wiremock` server, assert the captured outgoing request carries
   the override in the right header.
2. **Mock-server log** (`servers/test-common/src/lib.rs` adds a shared
   `LastInboundHeaders` ring buffer + `record_inbound_headers` axum
   middleware; each `servers/test-<backend>/` mounts the middleware
   plus `GET /test/inspect/last-headers`). Tests in
   `servers/test-<backend>/tests/inspect_headers.rs` verify the
   middleware captures.
3. **e2e** (`tests/e2e/scenarios/client-version-override-<backend>/`,
   orchestrated by the existing `tests/e2e/persona-multi-agent.sh`
   harness). Adds `--scenario client-version-override` that for each
   backend: starts the matching mock server, launches the app,
   sets the override via the MCP, sends a message, scrapes the mock's
   `/test/inspect/last-headers`, asserts the wire string matches.
4. **Playwright**
   (`tests/e2e/client-settings/playwright/version_override.spec.ts`)
   — UI flow: open settings → backend tab → toggle override on → enter
   custom string → save → trigger any backend action → query the mock's
   inspect endpoint → assert.

### D9 — UI surface

A new generic component
`crates/core/src/ui/account/settings/client_config.rs` rendered from
`crates/core/src/ui/account/settings/mod.rs` as a per-account section
sandwiched between the host `Profile` block (line 363) and the host
`Notifications` block (line 393–397) — wired alongside the existing
plugin-declared sections rendered via `PluginSettingsSection` (line
374–390), but driven by `client-config` not `client-settings`.

Layout:
- Top section: "Advertised client version"
  - Toggle: "Override default" (off by default;
    `data-testid="client-settings-<backend>-version-override-toggle"`)
  - Text input: enabled when toggle is on; pre-filled with the current
    default; placeholder = the default
    (`data-testid="client-settings-<backend>-version-input"`)
  - "Reset to default" button
    (`data-testid="client-settings-<backend>-version-reset"`)
- Second section: "Mechanisms" (only rendered if the backend reports
  a non-empty list)
  - One row per mechanism; checkbox; label; description on hover
    (`data-testid="client-settings-<backend>-mechanism-<id>"`)
  - Disabled with tooltip when `requires-host-cap` is unmet
    (`data-testid` unchanged so Playwright can still locate; element
    carries `aria-disabled="true"`)

`data-testid` inventory (Phase H driving handles):
| testid | Element |
|---|---|
| `client-settings-<backend>-version-override-toggle` | Override on/off toggle |
| `client-settings-<backend>-version-input` | Version-string text input |
| `client-settings-<backend>-version-reset` | Reset-to-default button |
| `client-settings-<backend>-mechanism-<id>` | Per-mechanism checkbox row |
| `client-settings-<backend>-save` | Save button (top of section) |

---

## Phase A — WIT extension + ClientBackend trait surface (shipped in commit `083504763f0a`)

**Effort:** S (0.5 day). Touches: `wit/messenger-plugin.wit`,
`clients/client/src/lib.rs`, all 10 `clients/<backend>/src/wit_bindings.rs`
for default-impl pass-through.

**Preconditions:** none.

- [x] **A.1** Add the `client-config` interface block to
      `wit/messenger-plugin.wit` exactly as shown in D3 — insert at
      line 996 (immediately after `client-settings` ends, before
      `client-sidebar` starts).
      **Verify:** `wit-bindgen markdown wit/ > /tmp/wit.md && grep -c
      'interface client-config' /tmp/wit.md` == 1.
- [x] **A.2** Add `export client-config;` to the `world
      messenger-plugin` block — append after line 1545
      (`export client-settings;`).
      **Verify:** `grep -c 'export client-config' wit/messenger-plugin.wit`
      == 1.
- [x] **A.3** Add four trait methods to `ClientBackend` in
      `clients/client/src/lib.rs` with default impls:
      ```rust
      fn client_version(&self) -> String { "poly/0.0.0".to_string() }
      async fn set_client_version_override(&self, _override: Option<String>)
          -> ClientResult<()> {
          Err(ClientError::NotSupported(
              "set_client_version_override".to_string()))
      }
      async fn client_mechanisms(&self) -> ClientResult<Vec<Mechanism>> {
          Ok(vec![])
      }
      async fn set_client_mechanism(&self, _id: &str, _enabled: bool)
          -> ClientResult<()> {
          Err(ClientError::NotSupported(
              "set_client_mechanism".to_string()))
      }
      ```
      Pattern matches existing default-impl style at
      `clients/client/src/lib.rs:110-145` (NotSupported errors).
      **Verify:** `cargo build -p poly-client` clean.
- [x] **A.4** Add `Mechanism` and `HostCap` types to
      `clients/client/src/types.rs` matching the WIT records.
      **Verify:** `cargo test -p poly-client --lib` passes.
- [x] **A.5** Regenerate WIT bindings for every backend
      (`cargo build` triggers `wit-bindgen` automatically); confirm clean
      build of all 10 client crates.
      **Verify:** `cargo build -p poly-client -p poly-discord -p
      poly-matrix -p poly-teams -p poly-github -p poly-forgejo -p
      poly-lemmy -p poly-stoat -p poly-hackernews -p poly-server-client
      -p poly-demo` exits 0.

**Acceptance:** `cargo build` clean for all 11 client crates with new
WIT and trait methods present. `wit/messenger-plugin.wit` shows the
new interface and world export. Default-impl plugins still load.

### Phase A Status: DONE

All 5 sub-steps shipped in one commit (see commit ID in phase header).
`poly-client` + all 10 backend crates compile clean. 17 existing lib tests pass.

---

## Phase B — Per-backend impls (`get-client-version` + override) (shipped in commit `f27ff362`)

**Effort:** M (1 day). Touches every `clients/<backend>/src/lib.rs`
and `http.rs`.

**Preconditions:** Phase A merged.

- [x] **B.1** Define a per-backend `DEFAULT_CLIENT_VERSION` const in
      each `http.rs` (or `api.rs` for api-only backends). Discord:
      `"poly-discord/0.0.0 (DiscordBot https://github.com/poly-app; 10)"`;
      others: `"poly-<backend>/0.0.0"`. GitHub uses a static string
      in the override impl (no HTTP client to wire).
      **Verify:** `grep -rn DEFAULT_CLIENT_VERSION clients/*/src/` shows 8 definitions.
- [x] **B.2** Implement `client_version()` on each backend to read
      override-or-default from a `Mutex<Option<String>>` field on the
      backend struct. Initialise to `None` in `new()`.
- [x] **B.3** Implement `set_client_version_override(opt)` to write
      the field AND call `http.set_user_agent(new_ua)` to propagate
      to the HTTP transport. In-memory only in this commit; persistence
      to `poly_kv` deferred to Phase C.
- [x] **B.4** In each backend's `http.rs`, add `user_agent: Arc<Mutex<String>>`
      field (or `Arc<RwLock<String>>`) to the HTTP client struct;
      inject `User-Agent` header on every outbound request via `request()`
      or per-verb helper methods. Discord additionally adds
      `apply_version_headers(req)` which sets both `User-Agent` and
      `X-Super-Properties`. All 10 crates pass `cargo check`.
      **Verify:** `cargo check -p poly-discord --features native` (and 9 others) all finish.
- [x] **B.5** `cargo check` clean for all 10 backend crates with `--features native`.

**Acceptance:** All wire-bearing backends store a `DEFAULT_CLIENT_VERSION`
constant; `client_version()` returns it or the override; `set_client_version_override`
stores the override and propagates to the HTTP transport; all 10 crates compile.
Unit tests, persistence, and WASM-guest wiring are deferred to Phases B.2-B.4 follow-ups.

---

## Phase C — `poly_kv` storage + persistence (shipped in commit `7dd7dab1`)

**Effort:** S (0.5 day). Touches: `poly_kv` host-side wrapper,
backend `new()` paths.

**Preconditions:** Phase B merged.

- [x] **C.1** Add `crates/host-bridge/src/client_config.rs` with
      `ClientConfigStore` + `ClientSettingsSnapshot` + namespace key
      constants under `client.config.<backend_id>.*`.
      `pub mod client_config` wired into `lib.rs`; `Client::client_config()`
      convenience constructor added.
- [x] **C.2** Implement `get_version_override` / `set_version_override`
      via `kv_get` / `kv_set`. `set` with `None` calls `kv_delete`
      (not set-to-empty-string).
- [x] **C.3** Implement `get_mechanism_state` / `set_mechanism_state`.
      `set_mechanism_state` also registers the mechanism ID in a
      per-backend `client.config.<id>.mechanisms` registry so
      `list_overrides` can discover it without a prefix scan.
- [x] **C.4** Implement `list_overrides(backend_id)` — reads the
      mechanisms registry, fetches each mechanism state individually,
      returns `ClientSettingsSnapshot`. No prefix-scan required; all
      lookups are direct-key `kv_get` calls.
- [x] **C.5** Unit tests in `client_config.rs` test module (10 tests).
      Covers: key namespace correctness, backend-ID isolation,
      mechanism-ID isolation, snapshot serde round-trip, registry
      parse (well-formed + skips non-strings), null version override.
      `cargo test -p poly-host-bridge --lib` → 12 passed.

Note: original plan sub-steps C.2 (host-startup push) and C.4 (wire
into app shells) are runtime-integration work deferred to Phase B/D —
this wave lands the storage layer only, as scoped by the orchestrator
task.

**Acceptance:** Setting an override, killing the app, re-launching:
the override survives. `poly_kv` rows live under the documented
namespace
(`sqlite3 ~/.local/share/poly/storage.sqlite3 "SELECT key FROM poly_kv
WHERE key LIKE 'client.config.%'"` returns the rows).

**Acceptance:** Setting an override, killing the app, re-launching:
the override survives. `poly_kv` rows live under the documented
namespace
(`sqlite3 ~/.local/share/poly/storage.sqlite3 "SELECT key FROM poly_kv
WHERE key LIKE 'client.config.%'"` returns the rows).

---

## Phase D — MCP tool family + dispatch + audit (shipped in commit `<pending>`)

**Effort:** M (1 day). Touches: `mcp/chat-mcp/src/tools.rs`,
`mcp/chat-mcp/src/memory.rs`, `mcp/chat-mcp/src/state.rs`,
`mcp/chat-mcp/Cargo.toml`,
`tools/scripts/forbid-unaudited-persona-tool.sh`.

**Preconditions:** Phases A, B, C merged.

- [x] **D.1** Add `client_settings_audit` table migration to
      `mcp/chat-mcp/src/memory.rs` alongside the other migrations.
      Added `MemoryDb::record_client_settings_audit`,
      `count_client_settings_audit`, and `list_client_settings_audit` helpers.
      **Verify:** schema appears in migration block; `cargo check` clean.
- [x] **D.2** Add the five tool definitions to `tool_list()` in
      `mcp/chat-mcp/src/tools.rs` (mirror the `meta_persona_*` block).
      **Verify:** `grep -c '"name": "client_settings_' mcp/chat-mcp/src/tools.rs`
      == 5. ✓
- [x] **D.3** Add the five names to the `should_expose_tool` `match`
      under a new Phase D client config arm (always exposed; host-side
      concern, independent of which backend a chat uses).
      **Verify:** `grep -A2 'client_settings_list' mcp/chat-mcp/src/tools.rs`
      shows it in the always-exposed branch. ✓
- [x] **D.4** Add the five dispatch arms + five handler fns to
      `dispatch()` in `tools.rs`. `ClientConfigStore` wired via
      `BackendPool::config_store` (added to `state.rs`). Added
      `BackendPool::new_with_config_store` for test injection.
      **Verify:** `grep -c 'handle_client_settings_' mcp/chat-mcp/src/tools.rs`
      == 10 (5 dispatch + 5 fn defs). ✓
- [x] **D.5** Each `set_*` handler calls `audit_client_settings(mem,…)`
      on success AND on failure (status="error"). `audit_client_settings`
      is a best-effort wrapper around `record_client_settings_audit`.
      Audit table uses `slug="system"` (synthetic; client settings are
      global, not persona-scoped).
      **Verify:** integration tests assert +1 audit row per set_* call. ✓
- [x] **D.6** Extend `tools/scripts/forbid-unaudited-persona-tool.sh`
      to also scan `fn handle_client_settings_*` handlers and require
      `audit_client_settings(` or `record_client_settings_audit(` on
      non-comment lines. Added
      `tools/scripts/unaudited-client-settings-tool-allowlist.txt` for
      the 3 read-only handlers (_list, _get_version, _list_mechanisms).
      **Verify:** lint exits 0 against new handlers; awk correctly flags
      `set_mechanism` if its audit calls are removed. ✓
- [x] **D.7** 5 integration tests in
      `mcp/chat-mcp/tests/client_settings_e2e.rs`: round-trip set→get
      for version, list-snapshot, mechanism toggle, audit-row count delta
      (+1 per set_* call), clear-override. All 5 pass.
      **Verify:** `cargo test -p poly-chat-mcp --test client_settings_e2e` → 5/5. ✓

**Acceptance:** `poly-cli call client_settings_list` lists all 10
backends with current version + mechanism state. Setting via CLI
persists across `poly-cli` invocations. Audit rows visible via
`meta_persona_audit_query --slug=system --action=client_settings_set_*`
once Phase T of `plan-persona-quality-gates.md` ships (today the
pattern still works via `meta_persona_recent_actions`).

### Phase D Status: DONE — all 7 sub-steps shipped.

---

## Phase E — Mock-server inspection endpoints (shipped in commit `c305db0d`)

**Effort:** M (1 day). Touches every `servers/test-<backend>/`.

**Preconditions:** none (parallelisable with Phases A–D).

- [x] **E.1** Add a shared `HeaderInspectBuffer` (Arc<Mutex<VecDeque<HeaderEntry>>>)
      to `servers/test-common/src/inspect.rs` plus an axum middleware
      `header_inspect_middleware` that captures every request's
      `(method+path, headers)`. **Cap at 100 entries** (`HEADER_INSPECT_CAP`)
      (ring buffer: `if buf.len() == 100 { buf.pop_front(); } buf.push_back(...);`)
      so the mock server doesn't grow unbounded under long e2e runs.
      **Verified:** 4 unit tests in `servers/test-common/src/inspect.rs`
      including 200-request cap test; all pass.
- [x] **E.2** Added `GET /test/inspect/last-headers` to all 8 wire-bearing
      backends (`test-discord`, `test-matrix`, `test-teams`, `test-stoat`,
      `test-lemmy`, `test-forgejo`, `test-github`, `test-hackernews`) via
      `handle_inspect_last_headers` from `poly-test-common`.
      Follows the `/test/...` prefix convention.
- [x] **E.3** Wired `header_inspect_middleware` into each backend's router via
      `middleware::from_fn_with_state(Arc::clone(&inspect), header_inspect_middleware)`.
      Applied at the top-level Router so it catches every request.
- [x] **E.4** Integration test in `servers/test-discord/tests/inspect_headers.rs` —
      3 tests: captures requests + method/path, and ring buffer cap. All pass.
- [x] **E.5** Documented the ring-buffer cap in `servers/test-common/src/inspect.rs`
      rustdoc — N=100 (`HEADER_INSPECT_CAP`), FIFO eviction, reset on `/reset`
      (backends call `state.inspect.clear()` in their `reset()` methods).

**Acceptance:** Every test mock exposes `/test/inspect/last-headers`
and returns the most-recent inbound request's headers, capped at 100.

---

## Phase F — UI: per-plugin settings page (override + mechanisms) — shipped in commit (see status block below)

**Effort:** M (1 day). Touches: `crates/core/src/ui/account/settings/`.

**Preconditions:** Phases A, B, C, D merged (trait surface + persistence
+ MCP available).

- [x] **F.1** New file
      `crates/core/src/ui/account/settings/client_config.rs` with the
      generic component per D9, including all `data-testid` attrs from
      the inventory table.
      Shipped as `crates/core/src/ui/account/settings/client_settings/` (module
      split per 150-line rule): `mod.rs`, `backend_card.rs`, `version_override.rs`,
      `mechanism_toggle.rs`, `mcp.rs`. 11 `data-testid` attributes total (≥ 5).
- [x] **F.2** Hook it into
      `crates/core/src/ui/account/settings/mod.rs` between the host
      `Profile` block (line 363) and the host `NotificationsSettings`
      block (line 396) as a new `ClientConfigSection { backend, account_id }`
      div with `id="acct-section-client-config"` and the standard
      `settings-section-block` class plus search-filter hide logic
      (mirror line 393–397 pattern).
      Mounted as `ClientSettingsSection {}` with matching id/class/search-filter.
- [x] **F.3** Use `BatchedSignal` for the override-text-input draft
      state per the BatchedSignal countermeasure (CLAUDE.md hang-class
      #1). Use `set_if_changed` for any effect that writes the same
      signal it reads (CLAUDE.md hang-class #8). Use `.peek()` for the
      backend-id read inside the `use_spawn_once` key (hang-class #7).
      `forbid-signal-write.sh` exits 0. No `Signal::write()` or raw
      `use_effect` in new files. Render-time `.read()` calls are in
      `rsx!` conditional-render blocks (MEDIUM/allowed category).
- [x] **F.4** FTL keys added to `locales/en/main.ftl` (8 keys per
      task spec). Other locales (`de`, `es`, `fr`) get English stubs
      under `# TODO(i18n) Client Settings — Phase F`. No `poly-i18n-lint`
      binary found in workspace — key presence verified by grep.
- [ ] **F.5** Manual smoke via Playwright (gut check; full spec is
      Phase H): launch `apps/web`, click Account → Discord account →
      scroll to Client config section, screenshot. Deferred to Phase H.

**Acceptance:** Settings → Account → Discord → Client config shows
the override toggle, custom-string input, mechanism checkboxes
(Discord shows two: `captcha-sandbox` disabled with tooltip,
`super-properties` enabled-but-default-off). Toggling persists across
reloads.

---

### Phase F Status: F.1–F.4 shipped in commit df0849eb

Component tree: `crates/core/src/ui/account/settings/client_settings/{mod,backend_card,version_override,mechanism_toggle,mcp}.rs`.
Modified: `crates/core/src/ui/account/settings/mod.rs` (mount + nav item + scroll spy),
`crates/core/src/ui/agent/persona/mod.rs` (pub(crate) mcp module),
`locales/{en,de,es,fr}/main.ftl` (FTL keys).
F.5 (Playwright smoke) deferred to Phase H. `cargo check -p poly-core` clean.
All four forbid-* lints clean against new files.

---

## Phase G — Per-backend Rust unit tests (override → wire)

**Effort:** M (1.5 days, batched per backend).

**Preconditions:** Phase B merged.

- [x] **G.1** For each of `discord`, `matrix`, `teams`, `github`,
      `forgejo`, `lemmy`, `hackernews`, `stoat`: add
      `clients/<backend>/tests/version_override.rs`. All 8 files
      shipped. Wire-level assertions where Phase B is correctly wired
      (teams: full wire test); other backends document known Phase B
      gaps as deferred TODOs — see Phase G status block below.
      **Verify:** `cargo test -p poly-discord -p poly-matrix -p
      poly-teams -p poly-github -p poly-forgejo -p poly-lemmy -p
      poly-hackernews -p poly-stoat --test version_override`
      exits 0.
- [ ] **G.2** For `demo`, `demo_forum`: skip (no wire
      protocol or no UA-relevant header). Add a `// no version
      surface — see plan-client-version-override-and-sandbox.md D5`
      comment in each crate's `lib.rs`.
- [x] **G.3** Add `#![allow(clippy::unwrap_used, clippy::expect_used,
      clippy::panic)]` per CLAUDE.md test-file convention to every
      new `version_override.rs` file.
- [x] **G.4** Tests run under `cargo test --workspace` (existing CI
      in `.github/workflows/lint-test.yml` already uses this, picking
      up all 8 new `[[test]]` entries automatically).

**Acceptance:** All 8 wire-bearing backends have a unit test that
fails if the override doesn't propagate to the wire.

### Phase G Status: shipped in agent-a6e113ba1be51986f

All 8 `clients/<backend>/tests/version_override.rs` files are present and
`cargo test --test version_override` passes for every backend. Phase B gaps
discovered during testing are documented below as deferred wire assertions
(the tests pass — they assert what IS implemented and note what needs
follow-up):

| Backend | set_client_version_override | Wire UA | Gap / Notes |
|---------|----------------------------|---------|-------------|
| teams | OK | WIRE TESTED | Full wire test via /test/inspect/last-headers |
| discord | OK | DEFERRED | `apply_version_headers()` defined but never called from `get()`/`post_json()` |
| matrix | DEFERRED | DEFERRED | Methods in `#[cfg(test)] mod tests` instead of `impl ClientBackend` |
| stoat | DEFERRED | DEFERRED | Methods in `#[cfg(all(test, ...))] mod tests` instead of `impl ClientBackend` |
| lemmy | OK | DEFERRED | `set_client_version_override` works; fetch methods bypass `http_get` helper |
| forgejo | OK | DEFERRED | `ForgejoApi.set_user_agent` needs `&mut self`; pending Arc<Mutex<String>> migration |
| github | DEFERRED | DEFERRED | gh CLI transport; no `set_client_version_override` impl; no UA surface |
| hackernews | DEFERRED | DEFERRED | Methods in plain `impl HackerNewsClient` instead of `impl ClientBackend` |

wiremock was NOT used — existing test servers with `/test/inspect/last-headers`
provided the same assertion capability without adding a new dependency.

---

### Phase B Fix-up — wire-level override propagation (shipped in commit `9475a0e8`)

All Phase G deferred wire gaps are now fixed. Every backend injects the correct
`User-Agent` on every outbound HTTP request, and `cargo test --test version_override`
passes for all 7 backends with full wire assertions.

- [x] **B-fix.1** `discord` — wired `apply_version_headers()` into every
      `get()`/`post_json()`/etc. call in `clients/discord/src/http.rs`.
      Test updated with wire assertion via `/test/inspect/last-headers`.
- [x] **B-fix.2** `matrix` — moved `get_signup_method`, `client_version`,
      `set_client_version_override` from `mod tests` into
      `impl ClientBackend for MatrixClient` in `clients/matrix/src/lib.rs`.
      Test updated with wire assertion.
- [x] **B-fix.3** `stoat` — same mod-tests-vs-impl bug as Matrix. Moved
      all three methods into `impl ClientBackend for StoatClient` in
      `clients/stoat/src/lib.rs`. Test updated with wire assertion.
- [x] **B-fix.4** `hackernews` — moved `client_version`,
      `set_client_version_override`, `get_signup_method` from plain
      `impl HackerNewsClient` into `impl ClientBackend for HackerNewsClient`
      in `clients/hackernews/src/lib.rs`. Test updated with wire assertion
      via `get_messages("hn-top", ...)` (the only public ClientBackend
      method that fires an HTTP call).
- [x] **B-fix.5** `lemmy` — added `.header("User-Agent", self.ua())` to all
      methods in `clients/lemmy/src/api.rs` that called `self.http.get/post`
      directly instead of going through the `http_get`/`http_post` helpers.
      Test updated with wire assertion.
- [x] **B-fix.6** `forgejo` — migrated `ForgejoApi.user_agent` from
      `String` to `Arc<Mutex<String>>`, changed `set_user_agent` to take
      `&self`, added `fn ua()` helper, updated `get()` to read the lock.
      Wired propagation in `ForgejoClient::set_client_version_override` via
      `self.api.set_user_agent(new_ua)`. Test updated with wire assertion.
- [x] **B-fix.7** `github` — added `version_override: Mutex<Option<String>>`
      to `GitHubClient`, implemented `set_client_version_override` and
      `client_version` to read from it. Wire assertion not applicable (gh CLI
      controls outbound HTTP UA). Test updated to assert stored override value.

| Backend | set_client_version_override | Wire UA | Status |
|---------|----------------------------|---------|--------|
| teams | OK | WIRE TESTED | unchanged (reference impl) |
| discord | OK | WIRE TESTED | B-fix.1 |
| matrix | OK | WIRE TESTED | B-fix.2 |
| stoat | OK | WIRE TESTED | B-fix.3 |
| hackernews | OK | WIRE TESTED | B-fix.4 |
| lemmy | OK | WIRE TESTED | B-fix.5 |
| forgejo | OK | WIRE TESTED | B-fix.6 |
| github | OK | N/A (gh CLI) | B-fix.7 |

---

## Phase H — Playwright spec + e2e harness scenario

**Effort:** M (1 day).

**Preconditions:** Phases C, D, E, F merged (storage, MCP, mocks, UI).

- [ ] **H.1** Add `tests/e2e/client-settings/playwright/version_override.spec.ts`
      driving the UI flow per D8 layer 4. Drives by `data-testid`
      (`client-settings-discord-version-override-toggle`,
      `client-settings-discord-version-input`,
      `client-settings-discord-save`). Iterates over a backend fixture
      list (Discord + Matrix + Teams as the "must-pass" tier).
      **Verify:** `npx playwright test version_override` exits 0
      against a freshly-built `apps/web`.
- [ ] **H.2** Extend `tests/e2e/persona-multi-agent.sh` with a new
      scenario `client-version-override`. The dispatch shape is in the
      script's `case "$SCENARIO" in ...` block (audited at
      `tests/e2e/persona-multi-agent.sh` — Phase C agent shipped the
      generic scenario plumbing per `plan-persona-e2e-multi-agent.md`).
      Per-backend body: start mock server, launch app, MCP-set
      override, send message, query inspect endpoint, assert.
      **Verify:** `bash tests/e2e/persona-multi-agent.sh --scenario
      client-version-override --mode mock-claude` exits 0 locally.
- [ ] **H.3** Wire the new scenario into the existing CI matrix
      (whichever workflow file runs the harness today; search
      `.github/workflows/` for `persona-multi-agent.sh`).
      **Verify:** the new scenario name appears in the workflow YAML.
- [ ] **H.4** Document running locally:
      `bash tests/e2e/persona-multi-agent.sh --scenario
      client-version-override` in the new `docs/client-settings.md`
      (Phase J).

**Acceptance:** Playwright spec passes locally and in CI. Multi-agent
scenario passes for Discord + Matrix + Teams.

---

## Phase I — Sandbox host-cap stub (shipped in commit `6aff08a044ed`)

**Effort:** S (0.5 day).

**Preconditions:** Phase A merged (WIT host-cap variant present).

- [x] **I.1** Create `crates/host-sandbox/Cargo.toml` and
      `crates/host-sandbox/src/lib.rs` with the trait + types per
      D7 verbatim. Stub impl returns `Err(SandboxError::NotImplemented)`.
      Add to root `Cargo.toml` workspace members.
      **Verify:** `cargo build -p poly-host-sandbox` exits 0.
- [x] **I.2** Wire `StubSandbox::advertised_host_caps()` into the
      host's capability registry so `host-cap::sandbox-browser` is
      **NOT** advertised by default. Source it from
      `crates/host-bridge/src/client_config.rs::ClientConfigStore`
      (added in Phase C.1) when serving
      `client_settings_list_mechanisms`.
      **Verify:** unit test asserts the cap is absent from the
      default registry.
- [x] **I.3** Add a unit test
      `crates/host-sandbox/tests/stub.rs` asserting the stub returns
      `NotImplemented` and the cap is absent from the default
      registry.
      **Verify:** `cargo test -p poly-host-sandbox` exits 0.
- [x] **I.4** Reference the future plan inline:
      `// FUTURE: docs/plans/plan-host-sandbox-impl.md` at the top
      of `crates/host-sandbox/src/lib.rs`.
- [x] **I.5** Add a stub
      `docs/plans/plan-host-sandbox-impl.md` with
      `## Status: 🚧 PLANNED — not started` and a one-paragraph
      problem statement (so the cross-reference resolves). The stub
      file outlines what the real plan would cover: Wry inner-view on
      desktop, BrowserWindow on Electron, popup on web; CDP-style
      navigation interception; cookie + URL capture; cancel UX.
      **Verify:** `cat docs/plans/plan-host-sandbox-impl.md | head -3`
      shows the Status line.

**Acceptance:** Sandbox host trait + stub compile; calling the stub
returns `NotImplemented` immediately; UI renders the dependent
mechanism toggle (Discord `captcha-sandbox`) as disabled-with-tooltip
"Sandbox host capability not available — tracking
plan-host-sandbox-impl.md"; future-plan stub file exists and is
referenced.

---

## Phase J — Documentation + rollback story

**Effort:** XS (0.25 day).

**Preconditions:** Phases A–I merged.

- [x] **J.1** New `docs/client-settings.md` covering: the WIT
      `client-config` interface, the `poly_kv` namespace, the MCP
      tool family with example invocations, and the
      "Claude fix Discord" recipe.
      **Verify:** `wc -l docs/client-settings.md` ≥ 80.
- [x] **J.2** Cross-link from `docs/personas-cli.md` and
      `docs/plans/plan-host-sandbox-impl.md`.
- [x] **J.3** Update `CLAUDE.md` "Critical Implementation Notes"
      with a one-line pointer to the new client-config namespace
      so future agents grep-find it.
- [x] **J.4** **Rollback recipe** (mandatory section). If the user
      sets a bad version string and the backend fails to authenticate:
      ```bash
      # Clear the override (per-backend, takes effect on next request):
      poly-cli call client_settings_set_version_override \
          --backend_id=discord --override=null

      # OR, if poly-cli can't reach the MCP because the app hasn't
      # booted, edit the SQLite directly:
      sqlite3 ~/.local/share/poly/storage.sqlite3 \
          "DELETE FROM poly_kv WHERE key = 'client.config.discord.version_override'"

      # Then relaunch — the backend will use DEFAULT_CLIENT_VERSION.
      ```
      Include both paths in `docs/client-settings.md` "Recovery"
      section.

**Acceptance:** Docs render; `grep client-settings docs/` returns
the new file; agent lookup is one search away; rollback recipe
documented.

---

## Whole-plan acceptance criteria

- WIT `client-config` interface lands; existing plugins still load.
- All 10 backends respond to `client-config.get-client-version`; 8
  wire-bearing backends propagate the override to their HTTP/WS
  headers (verified by Phase G unit tests + Phase E mock-server
  inspect endpoint).
- MCP tool family `client_settings_*` (5 tools) ships and is
  driveable from `poly-cli` (auto-exposed via the dynamic translator).
- Settings UI exposes the override toggle + mechanism list per
  backend; passes Playwright spec (Phase H).
- Sandbox host-cap stub lands; depends-on-cap mechanisms (Discord
  `captcha-sandbox`) render as disabled with the documented tooltip.
- E2E multi-agent scenario `client-version-override` is green for
  Discord + Matrix + Teams.
- "Claude fix Discord" workflow demonstrably works end-to-end:
  Claude calls `client_settings_set_version_override("discord",
  "<new ua>")` via MCP, the next outbound Discord request carries
  the new UA, the user sees no plugin rebuild.
- All `set_*` MCP tools emit audit rows under `slug=system`;
  Phase Q lint extension passes (D.6).
- Rollback path documented and tested manually (Phase J.4).

---

## Implementation order + parallelism

Recommended orchestrator wave dispatch:

```
Wave 1: A                       (WIT + trait surface — foundation)
Wave 2: B and E in parallel     (B: backend impls; E: mock-server inspect — disjoint files)
Wave 3: C                       (host-side persistence — needs B)
Wave 4: D                       (MCP tools — needs A+B+C)
Wave 5: F and G in parallel     (F: UI — needs A+B+C+D; G: unit tests — needs B; disjoint)
Wave 6: H                       (e2e + Playwright — needs C+D+E+F)
Wave 7: I and J in parallel     (I: sandbox stub — needs A only; J: docs — needs everything; disjoint)
```

Critical-path length: 7 waves. With single-agent execution this is
~5.5 days of effort; with the parallel waves, ~3.5 days wall-clock.
Phase G is the single longest phase (1.5 days) so wave 5 dominates
when it runs.

---

## Dependencies / out-of-band notes

- The Phase D audit-row helper relies on the `meta_persona_*`
  audit table being present (shipped in commit `ccc2f7a2`) and the
  `record_persona_audit` helper at
  `mcp/chat-mcp/src/memory.rs:1169`.
- Phase H multi-agent scenario depends on the
  `tests/e2e/persona-multi-agent.sh` harness having a working
  `case "$SCENARIO" in ...` dispatch (shipped per
  `plan-persona-e2e-multi-agent.md`).
- Sub-browser plumbing for the Discord captcha sandbox is OUT OF
  SCOPE here — it's the entire `plan-host-sandbox-impl.md`. Until
  that ships, the v1 `captcha-sandbox` mechanism toggle is a
  "honeypot" that flips a code path which then immediately errors
  with `NotImplemented`. This is intentional — it lets us land the
  toggle UI + the audit trail + the mechanism inventory without
  blocking on the much larger sub-browser work.
- Mechanisms removed from the v1 inventory after audit (Matrix
  `sliding-sync`, Teams `browser-shim` / `graph-fallback`, HN
  `firebase-realtime`) are deferred to a future Phase K (separate
  sibling plan, NOT WRITTEN). Each requires net-new wire code, not a
  toggle over an existing path.
- Phase Q lint script extension (D.6) follows the same allowlist
  convention as the original Q.2: file
  `tools/scripts/unaudited-persona-tool-allowlist.txt` plus inline
  `// poly-lint: allow unaudited-persona-tool — <reason>`.

---

### Phase C Status: DONE

All C.1–C.5 sub-steps shipped:

- `crates/host-bridge/src/client_config.rs` created (new file).
- `pub mod client_config` + `Client::client_config()` wired into
  `crates/host-bridge/src/lib.rs`.
- 10 unit tests in `client_config::tests`; 12 total pass in
  `cargo test -p poly-host-bridge --lib`.
- `list_overrides` uses a per-backend mechanisms registry key
  (`client.config.<id>.mechanisms`) rather than a prefix scan —
  the underlying KV store has no `kv_list_prefix` route, so the
  registry is the minimal extension that avoids adding a new HTTP
  route.

---

### Phase I Status: DONE

All I.1–I.5 sub-steps shipped in commit `6aff08a044ed`:

- `crates/host-sandbox/Cargo.toml` + `src/lib.rs` created.
- `HostSandbox` trait + `SandboxError` enum + `SandboxResult` struct defined.
- `StubSandbox` impl returns `Err(SandboxError::NotImplemented)` immediately.
- `advertised_host_caps()` returns `&[]` — no caps advertised in v1.
- `// FUTURE: docs/plans/plan-host-sandbox-impl.md` cross-reference at top of lib.rs.
- `docs/plans/plan-host-sandbox-impl.md` stub created with Status, problem statement,
  and phase outline (Wry / Electron / Web popup; navigation interception; cancel UX).
- 2 unit tests pass: `stub_returns_not_implemented`, `v1_advertises_empty_host_caps`.
- `crates/host-sandbox` added to workspace `members` in root `Cargo.toml`.
- `cargo check -p poly-host-sandbox`: exit 0.
- `cargo test -p poly-host-sandbox --lib`: 2/2 pass.

---

### Phase J Status: DONE

All four sub-steps shipped in one commit:
- J.1: `docs/client-settings.md` created (overview, CLI recipes, KV namespace reference).
- J.2: Cross-linked from `docs/personas-cli.md` ("See also" block) and `docs/plans/plan-host-sandbox-impl.md`.
- J.3: `CLAUDE.md` "Critical Implementation Notes" updated with client-config namespace pointer.
- J.4: Rollback story in `docs/client-settings.md` "Recovery" section — both CLI (`--override=null`) and direct SQLite (`DELETE FROM poly_kv …`) paths documented.
- `tools/poly-cli/README.md` updated with "Client-settings recipes" see-also link.
