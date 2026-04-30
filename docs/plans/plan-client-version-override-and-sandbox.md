# Plan: Client Version Override + Per-Mechanism Toggles + Sandbox Host-Cap Stub

## Status: 🚧 PLANNED — not started

> Sibling future plan referenced from Phase I:
> `docs/plans/plan-host-sandbox-impl.md` (NOT YET WRITTEN — captures the
> actual sub-browser plumbing for Discord captcha-style challenges).

---

## Problem

When a backend's wire protocol checks an "advertised client version"
(Discord User-Agent + `X-Discord-Locale` + super-properties; Matrix
`User-Agent`; Teams `client-version`/`x-ms-client-version` headers;
GitHub `User-Agent`; Forgejo `User-Agent`) and the upstream tightens
the accepted set, our plugins break until the next plugin/app release.
A user reporting "Discord stopped working" has no way to bump the
advertised string themselves; they must wait for us to ship.

Likewise, mechanisms inside a backend that the upstream gates on
(Discord captcha-sandbox, Matrix sliding-sync vs `/sync` v2, Teams
browser-mode shim) are currently hardcoded per plugin. The user can't
toggle them without a plugin rebuild.

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
   are landed; the actual sandbox impl returns `Err(NotSupported)` and
   is deferred to `plan-host-sandbox-impl.md`.

---

## Non-goals

- Implementing the sandbox itself (Phase I lands the stub only).
- Per-account overrides (version is a per-backend setting; mechanisms
  are per-backend in v1, per-account is future work).
- Breaking the WIT for plugins built against the older interface — all
  new methods carry default impls so existing plugins still load.
- Adding fields to existing MCP tools — `client_settings_*` is its own
  family.
- Refactoring the existing `client-settings` WIT interface — the new
  surface lives in a new `client-config` WIT interface (justified
  below) so the storage-vs-config split stays clean.

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

### D2 — Storage namespace

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

`<backend_id>` is the BackendId slug (`discord`, `matrix`, `teams`, …)
already used in `clients/client/src/types.rs`. Justification: the
`client.config.` prefix is disjoint from existing `plugin.` /
`account.` / `kv.` prefixes (audited via grep on commit ancestry);
nesting under `<backend_id>` makes per-backend cleanup trivial when a
backend is removed.

### D3 — WIT extension shape (concrete)

New `client-config` interface to add in `wit/messenger-plugin.wit`,
exported by the `messenger-plugin` world:

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

### D4 — MCP tool family

Mirror the `meta_persona_*` shape from `mcp/chat-mcp/src/tools.rs`
(see lines 957+). Five new tools:

| Tool name | Args | Returns |
|---|---|---|
| `client_settings_list` | `backend_id?: string` | List of `{backend_id, version, version_override, mechanisms[]}` for one or all backends |
| `client_settings_get_version` | `backend_id: string` | `{version, override_active: bool, default_version: string}` |
| `client_settings_set_version_override` | `backend_id: string, override?: string` | Success or `client-error` |
| `client_settings_list_mechanisms` | `backend_id: string` | List of `mechanism` records |
| `client_settings_set_mechanism` | `backend_id: string, mechanism_id: string, enabled: bool` | Success or `client-error` |

Every `set_*` writes a row to the existing audit table used by
`meta_persona_*` set-actions (see `meta_persona_recent_actions`) so
"Claude fix Discord" leaves a paper trail.

### D5 — Per-backend mechanism inventory (v1)

Initial mechanism set per backend. "none" is fine — adds in later PRs.

| Backend | Backend ID | v1 mechanisms | Notes |
|---|---|---|---|
| Demo | `demo` | none | Reference impl; serves as template |
| Discord | `discord` | `captcha-sandbox` (requires `sandbox-browser` host-cap), `super-properties` | First needs the sandbox host-cap stub of Phase I; second toggles the X-Super-Properties header that real Discord checks |
| Forgejo | `forgejo` | none | Plain REST + token; no client-side mechanism |
| GitHub | `github` | none | Same |
| HackerNews | `hackernews` | `firebase-realtime` | Currently always on; toggle to fall back to plain REST polling |
| Lemmy | `lemmy` | none | |
| Matrix | `matrix` | `sliding-sync`, `e2ee-disabled` | First swaps `/sync` v2 for sliding-sync endpoint; second is a debug toggle |
| Server-client (poly-server) | `poly-server` | none | Internal protocol; no advertised version |
| Stoat | `stoat` | none | Local-only test backend |
| Teams | `teams` | `browser-shim`, `graph-fallback` | First sends the User-Agent that the real Teams web client sends; second falls back to MS Graph when MTC is rate-limited |

### D6 — Sandbox host-cap stub

The `host-cap::sandbox-browser` variant lands in WIT in Phase A. The
host-side trait + impl-stub lands in Phase I:

```rust
// In a new crate `crates/host-sandbox/src/lib.rs`:

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
        // INTENTIONAL STUB — sub-browser plumbing is not in v1.
        // Tracking plan: docs/plans/plan-host-sandbox-impl.md
        // The Discord captcha-sandbox mechanism toggle in v1 only
        // gates a code path; this stub returning NotImplemented
        // is the expected response when that code path actually
        // tries to open the browser.
        Err(SandboxError::NotImplemented)
    }
}
```

### D7 — Test strategy (per-backend, four layers)

For every backend that has a wire protocol (i.e. all except `demo`,
`stoat`, `server-client`), the test pyramid is:

1. **Unit** (`clients/<backend>/tests/version_override.rs`) — build a
   client with a custom version, call any HTTP-bearing method, assert
   the captured outgoing request carries the override in the right
   header.
2. **Mock-server log** (`servers/test-<backend>/src/lib.rs` adds
   `GET /test/inspect/last-headers` that returns the most recent
   inbound request's headers as JSON). Tests in
   `servers/test-<backend>/tests/version_advertise.rs` set the override
   via the WIT, exercise a real client method, query the inspect
   endpoint, assert the override propagated.
3. **e2e** (`tests/e2e/client-settings/`, orchestrated by the existing
   harness from `plan-persona-e2e-multi-agent.md`) — adds a
   `--scenario client-version-override` that for each backend: starts
   the matching mock server, launches the app, sets the override via
   the MCP, sends a message, scrapes the mock's
   `/test/inspect/last-headers`, asserts the wire string matches.
4. **Playwright** (`tests/e2e/client-settings/playwright/version_override.spec.ts`)
   — UI flow: open settings → backend tab → toggle override on → enter
   custom string → save → trigger any backend action → query the mock's
   inspect endpoint → assert.

### D8 — UI surface

A new generic page `crates/core/src/ui/account/settings/client_config.rs`
that renders the WIT-discovered list:

- Top section: "Advertised client version"
  - Toggle: "Override default" (off by default)
  - Text input: enabled when toggle is on; pre-filled with the current
    default; placeholder = the default
  - "Reset to default" button
- Second section: "Mechanisms" (only rendered if the backend reports
  a non-empty list)
  - One row per mechanism; checkbox; label; description on hover
  - Disabled with tooltip when `requires-host-cap` is unmet

Wired from the existing per-backend account-settings entry points
(`crates/core/src/ui/account/settings/mod.rs` already routes to
backend-specific tabs); each backend tab gets a "Client config" sub-
tab that mounts the generic component.

---

## Phase A — WIT extension + ClientBackend trait surface

**Effort:** S (0.5 day). Touches: `wit/messenger-plugin.wit`,
`clients/client/src/lib.rs`, all 10 `clients/<backend>/src/wit_bindings.rs`
for default-impl pass-through.

**Preconditions:** none.

- [ ] **A.1** Add the `client-config` interface block to
      `wit/messenger-plugin.wit` exactly as shown in D3.
- [ ] **A.2** Add `export client-config;` to the
      `world messenger-plugin` block (line 1540+).
- [ ] **A.3** Add four trait methods to `ClientBackend` in
      `clients/client/src/lib.rs` with default impls:
      `client_version()` returns a per-backend const,
      `set_client_version_override(opt)` returns
      `Err(ClientError::NotSupported(...))`,
      `client_mechanisms()` returns `Ok(vec![])`,
      `set_client_mechanism(id, on)` returns `NotSupported`.
- [ ] **A.4** Add `Mechanism` and `HostCap` types to
      `clients/client/src/types.rs` matching the WIT records.
- [ ] **A.5** Regenerate WIT bindings for every backend
      (`wit_bindings.rs`); confirm clean build of
      `cargo build -p poly-client -p poly-discord -p poly-matrix
      -p poly-teams -p poly-github -p poly-forgejo -p poly-lemmy
      -p poly-stoat -p poly-hackernews -p poly-server-client
      -p poly-demo`.

**Acceptance:** `cargo build` clean for all 11 client crates with new
WIT and trait methods present. `wit/messenger-plugin.wit` shows the
new interface and world export. Default-impl plugins still load.

---

## Phase B — Per-backend impls (`get-client-version` + override)

**Effort:** M (1 day). Touches every `clients/<backend>/src/lib.rs`
and `http.rs`.

**Preconditions:** Phase A merged.

- [ ] **B.1** Define a per-backend `DEFAULT_CLIENT_VERSION` const in
      each `lib.rs` (Discord: matches current real-Discord web build;
      Matrix: matches Element web; Teams: matches Teams web; others:
      something sensible).
- [ ] **B.2** Implement `client_version()` on each backend to read
      override-or-default from a `Mutex<Option<String>>` field on the
      backend struct.
- [ ] **B.3** Implement `set_client_version_override(opt)` to write
      the field AND call `host-api.storage-set` with key
      `version_override` (JSON-encoded `string` or `null`); on init,
      every backend's `new()` reads `host-api.storage-get` for
      `version_override` and seeds the field.
- [ ] **B.4** In each backend's `http.rs`, add a `User-Agent` header
      (or backend-equivalent — Teams `x-ms-client-version`, Discord
      `User-Agent` + `X-Super-Properties` if `super-properties`
      mechanism is on) to every outbound request, sourced from
      `client_version()`. Use a single `apply_version_headers(req)`
      helper per backend so call sites can't forget.
- [ ] **B.5** Smoke-test manually with `dev-plugins` — the
      `apps/web` build should still load and connect at least one
      backend.

**Acceptance:** All 10 backends advertise a version on the wire by
default; setting an override via the trait method changes the
advertised string on the next request. Verified by logging a request
with `tracing::debug!(target: "client::http", ...)` and grepping the
log.

---

## Phase C — `poly_kv` storage + persistence

**Effort:** S (0.5 day). Touches: `poly_kv` host-side wrapper,
backend `new()` paths.

**Preconditions:** Phase B merged.

- [ ] **C.1** Add a host-side helper
      `crates/host-config/src/lib.rs::ClientConfigStore` that exposes
      `get_version_override(backend_id)`,
      `set_version_override(backend_id, opt)`,
      `get_mechanism(backend_id, mech_id)`,
      `set_mechanism(backend_id, mech_id, on)` — all backed by
      `poly_kv` under the `client.config.<backend_id>.*` namespace.
- [ ] **C.2** On host startup, the host iterates loaded backends and
      pushes any persisted override into the plugin via the new WIT
      `set-client-version-override` call. (Belt-and-braces: the plugin
      ALSO reads its own `storage-get` per Phase B, but the host
      authoritative key wins.)
- [ ] **C.3** Add a unit test in `crates/host-config/tests/persist.rs`
      that round-trips `set → restart sim → get` for each backend ID.
- [ ] **C.4** Wire `ClientConfigStore` into the existing host
      bootstrap (`apps/poly-host/src/main.rs` and the per-shell
      fullstack bootstrap in `apps/web`, `apps/desktop`,
      `apps/desktop-electron`).

**Acceptance:** Setting an override, killing the app, re-launching:
the override survives. `poly_kv` rows live under the documented
namespace.

---

## Phase D — MCP tool family + dispatch + audit

**Effort:** M (1 day). Touches: `mcp/chat-mcp/src/tools.rs`,
`mcp/chat-mcp/src/lib.rs`.

**Preconditions:** Phases A–C merged.

- [ ] **D.1** Add the five tool definitions (per D4 table) to the
      tool-list block in `mcp/chat-mcp/src/tools.rs` (mirror the
      `meta_persona_*` block at lines 957+).
- [ ] **D.2** Add the dispatch arms to the `match name` in the same
      file (mirror lines 168–183).
- [ ] **D.3** Each `set_*` tool emits an audit row via the same path
      `meta_persona_set_*` uses (look up the helper near
      `meta_persona_recent_actions`).
- [ ] **D.4** Add a `poly-cli call client_settings_list` smoke recipe
      to `docs/personas-cli.md` (or new `docs/client-settings.md`,
      see Phase J).
- [ ] **D.5** Unit test `mcp/chat-mcp/tests/client_settings_tools.rs`
      — invokes each tool against a mock backend registry, asserts
      the audit row landed.

**Acceptance:** `poly-cli call client_settings_list` lists all 10
backends with current version + mechanism state. Setting via CLI
persists across `poly-cli` invocations.

---

## Phase E — Mock-server inspection endpoints

**Effort:** M (1 day). Touches every `servers/test-<backend>/`.

**Preconditions:** none (parallelisable with Phases A–D).

- [ ] **E.1** Add a shared `LastInboundHeaders: Mutex<Option<HashMap<String,String>>>`
      to `servers/test-common/src/lib.rs` plus an axum middleware
      `record_inbound_headers` that captures every request.
- [ ] **E.2** Add `GET /test/inspect/last-headers` to every
      `servers/test-<backend>/src/lib.rs` returning the captured map
      as JSON.
- [ ] **E.3** Wire the middleware into each backend's router
      (one-line `.layer(record_inbound_headers())` per server).
- [ ] **E.4** Per-server smoke test
      (`servers/test-<backend>/tests/inspect_headers.rs`) — boot
      the server, send any request, GET the inspect endpoint, assert
      the request method + path landed.

**Acceptance:** Every test mock exposes `/test/inspect/last-headers`
and returns the most-recent inbound request's headers.

---

## Phase F — UI: per-plugin settings page (override + mechanisms)

**Effort:** M (1 day). Touches: `crates/core/src/ui/account/settings/`.

**Preconditions:** Phase A (trait surface) merged.

- [ ] **F.1** New file
      `crates/core/src/ui/account/settings/client_config.rs` with the
      generic component per D8.
- [ ] **F.2** Hook it into each backend's per-account settings tab
      from `crates/core/src/ui/account/settings/mod.rs` as a
      "Client config" sub-tab.
- [ ] **F.3** Use `BatchedSignal` for the override-text-input draft
      state per the BatchedSignal countermeasure (CLAUDE.md hang-class
      #1). Use `set_if_changed` for any effect that writes the same
      signal it reads (CLAUDE.md hang-class #8).
- [ ] **F.4** Add FTL keys (`plugin-discord-mechanism-captcha-sandbox-label`,
      etc.) to each backend's `clients/<backend>/locales/en/<backend>.ftl`.
- [ ] **F.5** Manual smoke via Playwright (gut check; full spec is
      Phase H).

**Acceptance:** Settings → Account → Discord → Client config shows
the override toggle, custom-string input, mechanism checkboxes
(Discord shows two: captcha-sandbox disabled with tooltip, super-
properties enabled). Toggling persists across reloads.

---

## Phase G — Per-backend Rust unit tests (override → wire)

**Effort:** M (1.5 days, batched per backend).

**Preconditions:** Phase B merged.

- [ ] **G.1** For each of `discord`, `matrix`, `teams`, `github`,
      `forgejo`, `lemmy`, `hackernews`: add
      `clients/<backend>/tests/version_override.rs` that:
      1. Builds a backend with `mock_http_client()` (already in
         `poly-host-bridge`).
      2. Calls `set_client_version_override(Some("9.99.99-test"))`.
      3. Calls a representative HTTP method (`get_user`, `list_dms`).
      4. Asserts the captured request carries the override in the
         right header.
- [ ] **G.2** For `demo`, `stoat`, `server-client`: skip (no wire
      protocol or no UA-relevant header). Add a `// no version
      surface` README note in each crate.
- [ ] **G.3** Add `#![allow(clippy::unwrap_used, clippy::expect_used,
      clippy::panic)]` per CLAUDE.md test-file convention.
- [ ] **G.4** Tests run under `cargo test -p poly-<backend>`; CI gate
      added.

**Acceptance:** All 7 wire-bearing backends have a unit test that
fails if the override doesn't propagate to the wire.

---

## Phase H — Playwright spec + e2e harness scenario

**Effort:** M (1 day).

**Preconditions:** Phases C, D, E, F merged (storage, MCP, mocks, UI).

- [ ] **H.1** Add `tests/e2e/client-settings/playwright/version_override.spec.ts`
      driving the UI flow per D7 layer 4. Iterates over a backend
      fixture list (Discord + Matrix + Teams as the "must-pass" tier).
- [ ] **H.2** Add `--scenario client-version-override` to the
      multi-agent harness from `plan-persona-e2e-multi-agent.md`.
      The scenario for each backend: start mock server, launch app,
      MCP-set override, send message, query inspect endpoint, assert.
- [ ] **H.3** Wire the new scenario into the existing CI matrix.
- [ ] **H.4** Document running locally:
      `poly-test-runner run --scenario client-version-override`.

**Acceptance:** Playwright spec passes locally and in CI. Multi-agent
scenario passes for Discord + Matrix + Teams.

---

## Phase I — Sandbox host-cap stub

**Effort:** S (0.5 day).

**Preconditions:** Phase A merged (WIT host-cap variant present).

- [ ] **I.1** Create `crates/host-sandbox/Cargo.toml` and
      `crates/host-sandbox/src/lib.rs` with the trait + types per
      D6. Stub impl returns `Err(SandboxError::NotImplemented)`.
- [ ] **I.2** Wire `StubSandbox` into the host's capability registry
      so `host-cap::sandbox-browser` is **NOT** advertised by default
      (the registry returns the absence of `sandbox-browser`, so the
      Discord `captcha-sandbox` mechanism toggle is rendered disabled
      with tooltip "Sandbox host capability not available — tracking
      plan-host-sandbox-impl.md").
- [ ] **I.3** Add a unit test
      `crates/host-sandbox/tests/stub.rs` asserting the stub returns
      `NotImplemented` and the cap is absent from the default
      registry.
- [ ] **I.4** Reference the future plan inline:
      `// FUTURE: docs/plans/plan-host-sandbox-impl.md` at the top
      of `crates/host-sandbox/src/lib.rs`.
- [ ] **I.5** Add a stub
      `docs/plans/plan-host-sandbox-impl.md` with
      `## Status: 🚧 PLANNED — not started` and a one-paragraph
      problem statement (so the cross-reference resolves).

**Acceptance:** Sandbox host trait + stub compile; calling the stub
returns `NotImplemented` immediately; UI renders the dependent
mechanism toggle as disabled-with-tooltip; future-plan stub file
exists and is referenced.

---

## Phase J — Documentation

**Effort:** XS (0.25 day).

**Preconditions:** Phases A–I merged.

- [ ] **J.1** New `docs/client-settings.md` covering: the WIT
      `client-config` interface, the `poly_kv` namespace, the MCP
      tool family with example invocations, and the
      "Claude fix Discord" recipe.
- [ ] **J.2** Cross-link from `docs/personas-cli.md` and
      `docs/plans/plan-host-sandbox-impl.md`.
- [ ] **J.3** Update `CLAUDE.md` "Critical Implementation Notes"
      with a one-line pointer to the new client-config namespace
      so future agents grep-find it.

**Acceptance:** Docs render; `grep client-settings docs/` returns
the new file; agent lookup is one search away.

---

## Whole-plan acceptance criteria

- WIT `client-config` interface lands; existing plugins still load.
- All 10 backends respond to `client-config.get-client-version`; 7
  wire-bearing backends propagate the override to their HTTP/WS
  headers (verified by Phase G unit tests + Phase E mock-server
  inspect endpoint).
- MCP tool family `client_settings_*` (5 tools) ships and is
  driveable from `poly-cli`.
- Settings UI exposes the override toggle + mechanism list per
  backend; passes Playwright spec (Phase H).
- Sandbox host-cap stub lands; depends-on-cap mechanisms (Discord
  captcha-sandbox) render as disabled with the documented tooltip.
- E2E multi-agent scenario `client-version-override` is green for
  Discord + Matrix + Teams.
- "Claude fix Discord" workflow demonstrably works end-to-end:
  Claude calls `client_settings_set_version_override("discord",
  "<new ua>")` via MCP, the next outbound Discord request carries
  the new UA, the user sees no plugin rebuild.

---

## Dependencies / out-of-band notes

- The Phase D audit-row helper relies on the `meta_persona_*`
  audit table being present (shipped in commit `ccc2f7a2`).
- Phase H multi-agent scenario depends on the
  `plan-persona-e2e-multi-agent.md` harness being far enough along
  to accept new scenarios — coordinate with whoever owns that plan.
- Sub-browser plumbing for the Discord captcha sandbox is OUT OF
  SCOPE here — it's the entire `plan-host-sandbox-impl.md`. Until
  that ships, the v1 `captcha-sandbox` mechanism toggle is a
  "honeypot" that flips a code path which then immediately errors
  with `NotImplemented`. This is intentional — it lets us land the
  toggle UI + the audit trail + the mechanism inventory without
  blocking on the much larger sub-browser work.
