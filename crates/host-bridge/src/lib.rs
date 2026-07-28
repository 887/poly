//! # poly-host-bridge
//!
//! Host-API bridge for the **dioxus WASM target**.
//!
//! ## Why this crate exists
//!
//! Poly's WIT [`host-api`](../../../../wit/messenger-plugin.wit) defines the
//! syscall-like operations that messenger backends need: `exec-command`,
//! `http-request`, `websocket-*`, `storage-*`, `log`. WASM components running
//! inside `wasmtime` get these for free via [`crates/plugin-host`]'s
//! [`host_impl`](../../plugin-host/src/host_impl.rs).
//!
//! But Poly also ships as a **dioxus WASM** app loaded inside thin native
//! shells (Wry on desktop-web, Electron on desktop-electron-web, WKWebView on
//! iOS, Android WebView on android). That WASM target is *not* a wasm
//! component — it cannot import WIT functions, and the browser sandbox
//! forbids subprocess / unrestricted FS / arbitrary sockets. To give it the
//! same capability surface, each native shell binds a small HTTP endpoint
//! ([`BRIDGE_PATH`]) on [`BRIDGE_PORT`] that speaks the JSON protocol defined
//! in this crate. WASM code calls the bridge through [`Client`].
//!
//! The protocol mirrors the WIT host-api one-to-one. New operations are
//! added here whenever the host-api gains new functions, so the same client
//! code works no matter which side of the boundary it lives on.
//!
//! ## Per-shell support
//!
//! | Shell                                 | Bridge implementation              | Status |
//! |---------------------------------------|------------------------------------|--------|
//! | `apps/desktop-web` (Wry)              | Rust [`dispatch`] in axum          | ✅     |
//! | `apps/desktop-electron-web` (Electron)| Node `http` + `child_process`      | ✅     |
//! | `apps/web` (browser)                  | none — no native side              | n/a    |
//! | iOS (WKWebView shell)                 | future — needs native shell crate  | ⏳     |
//! | Android (WebView shell)               | future — needs subprocess (Termux/terminal) | ⏳ |
//!
//! Until iOS / Android shells expose the bridge, [`Client::call`] returns
//! [`BridgeError::Unreachable`] on those targets and callers should degrade
//! gracefully (e.g. show "this backend needs the desktop shell").

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod client_config;
pub mod http;
pub mod route;

// Caller identity for the `/host/*` surface (Phase A of
// docs/plans/plan-host-substrate-capability-gating.md). Compiles on every
// target: the server half (`HostAuth`) is cfg-gated inside the module, the
// client half (token cache + `send_authorized`) is what the WASM bundle uses.
pub mod host_auth;

// `/host/exec` program allowlist + user consent (Phase B). Native-only —
// there is no subprocess to gate on wasm32.
#[cfg(not(target_arch = "wasm32"))]
pub mod exec_policy;

// At-rest sealing for credential-bearing `poly_kv` rows (Phase C.3).
// Native-only: only the shell that owns the SQLite file seals anything.
#[cfg(not(target_arch = "wasm32"))]
pub mod secret_seal;

// Outbound-destination guard (SSRF filter) shared by `/host/http` and
// `/host/udp/*`. Native-only — it resolves DNS, which wasm32 cannot do.
#[cfg(not(target_arch = "wasm32"))]
pub mod net_guard;

// Video H.264 encode/decode service — server-side handlers (non-wasm, feature-gated).
// WASM targets call the endpoint via HTTP; openh264-rs never links into the WASM bundle.
#[cfg(all(not(target_arch = "wasm32"), feature = "video"))]
pub mod video;

// Typed client for /host/video/* — usable from native callers (NativeVideoBackend, etc.).
// Not available on wasm32 (WASM callers use their HTTP stack directly against the endpoint).
#[cfg(all(not(target_arch = "wasm32"), feature = "video"))]
pub mod video_client;

// Voice bridge wire types — SSE event enum shared by clients.
// Available on ALL targets including wasm32 (no native deps).
pub mod voice_wire;

// ── Generic host primitives (udp, codec-opus, aead) ─────────────────────────
//
// These three route sets replace the old Discord-coupled voice.rs.
// They are reusable by any plugin (Discord, Stoat, Matrix, etc.).

// UDP socket service — server-side handlers. Non-wasm + "udp" feature only.
#[cfg(all(not(target_arch = "wasm32"), feature = "udp"))]
pub mod udp;

// Typed client for /host/udp/* — all targets including wasm32.
pub mod udp_client;

// Opus encode/decode service — server-side handlers. Non-wasm + "codec-opus" feature only.
#[cfg(all(not(target_arch = "wasm32"), feature = "codec-opus"))]
pub mod codec_opus;

// Typed client for /host/codec/opus/* — all targets including wasm32.
pub mod codec_opus_client;

// AEAD encrypt/decrypt service — server-side handlers. Non-wasm + "aead" feature only.
#[cfg(all(not(target_arch = "wasm32"), feature = "aead"))]
pub mod aead;

// Typed client for /host/aead/* — all targets including wasm32.
pub mod aead_client;

// Microsoft Graph change-notification webhook relay — server-side handlers.
// Non-wasm + "teams-webhook" feature only. WASM callers reach the relay via
// their /host bridge transparently; no typed client needed (the relay's
// consumers are Microsoft Graph + an injected NotificationSink, not WASM code).
//
// Phase C of `docs/plans/plan-teams-graph-subscriptions.md`.
#[cfg(all(not(target_arch = "wasm32"), feature = "teams-webhook"))]
pub mod teams_webhook;

// Microsoft Graph rich-notification encryption helpers — Phase D of
// `docs/plans/plan-teams-graph-subscriptions.md`. Generates the RSA
// keypair the subscription registers and decrypts incoming
// `encryptedContent` payloads. Gated behind the same `teams-webhook`
// feature so it doesn't compile into shells that don't host the relay.
#[cfg(all(not(target_arch = "wasm32"), feature = "teams-webhook"))]
pub mod teams_encryption;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Loopback port that every native shell binds for the host bridge.
///
/// Distinct from the dev-MCP eval bridge ports (9222 / 9223 / 9224) so the
/// runtime host bridge is unaffected by whether dev tooling is loaded.
pub const BRIDGE_PORT: u16 = 9333;

/// HTTP path of the legacy tagged-union host bridge endpoint.
///
/// Kept for one release cycle so existing WASM builds that POST a
/// `HostCall` tagged union to `/host` keep working. New code should use
/// the per-category sub-routes under [`BRIDGE_PREFIX`] instead.
pub const BRIDGE_PATH: &str = "/host";

/// Path prefix for the per-category multi-route layout introduced in
/// phase 2.21. See `docs/plans/phase-2.21-host-bridge-unification-plan.md`.
pub const BRIDGE_PREFIX: &str = "/host";

/// Sub-route for `ExecCommand`.
pub const ROUTE_EXEC: &str = "/host/exec";
/// Sub-route for recording user consent for one `(caller, program)` pair.
/// Shell-only — see `exec_policy` and Phase B.4 of
/// `docs/plans/plan-host-substrate-capability-gating.md`.
pub const ROUTE_EXEC_CONSENT: &str = "/host/exec/consent";
/// Sub-route for `HttpRequest`.
pub const ROUTE_HTTP: &str = "/host/http";
/// Sub-route for app KV `get`.
pub const ROUTE_KV_GET: &str = "/host/kv/get";
/// Sub-route for app KV `set`.
pub const ROUTE_KV_SET: &str = "/host/kv/set";
/// Sub-route for app KV `delete`.
pub const ROUTE_KV_DELETE: &str = "/host/kv/delete";
/// Sub-route for app KV `clear_all`.
pub const ROUTE_KV_CLEAR: &str = "/host/kv/clear";
/// Sub-route for the bridge liveness ping.
pub const ROUTE_STATUS: &str = "/host/status";
/// Sub-route for `POST /host/open-external` — open a URL in the system browser.
pub const ROUTE_OPEN_EXTERNAL: &str = "/host/open-external";

/// Full default URL of the legacy `/host` dispatch endpoint.
pub const BRIDGE_URL: &str = "http://127.0.0.1:9333/host";

/// Base URL every native shell binds for the new route layout.
pub const BRIDGE_BASE_URL: &str = "http://127.0.0.1:9333";

// ─── Protocol — request side ─────────────────────────────────────────────────

/// One host-api call. Tagged-union JSON: `{"call": "<kebab-case>", ...fields}`.
///
/// Mirrors the operations defined in `wit/messenger-plugin.wit` under
/// `interface host-api`. New variants land here whenever WIT gains a new
/// host function.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "call", rename_all = "kebab-case")]
pub enum HostCall {
    /// Spawn a subprocess and wait for it to exit.
    ///
    /// `program` and `args` go straight to the OS exec — no shell — so argv
    /// metacharacters (`&&`, `|`, `$`, backticks) stay inert.
    ExecCommand {
        program: String,
        args: Vec<String>,
    },
    /// Make a one-shot HTTP request via the host's network stack.
    HttpRequest {
        method: String,
        url: String,
        #[serde(default)]
        headers: Vec<(String, String)>,
        /// Base64-encoded request body, or `None` for an empty body.
        #[serde(default)]
        body_b64: Option<String>,
    },
}

// ─── Protocol — response side ────────────────────────────────────────────────

/// Successful response payload, tagged by which call produced it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum HostOk {
    /// Result of [`HostCall::ExecCommand`].
    ExecOutput {
        exit_code: i32,
        /// Base64-encoded process stdout bytes.
        stdout_b64: String,
        /// Base64-encoded process stderr bytes.
        stderr_b64: String,
    },
    /// Result of [`HostCall::HttpRequest`].
    HttpResponse {
        status: u16,
        headers: Vec<(String, String)>,
        body_b64: String,
    },
}

/// Bridge response: either a typed [`HostOk`] or an error string.
///
/// Wire shape: `{"ok": {...}}` or `{"err": "..."}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostResponse {
    Ok(HostOk),
    Err(String),
}

// ─── KV sub-route payloads ───────────────────────────────────────────────────

/// Body for `POST /host/kv/get`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvGetRequest {
    pub key: String,
}

/// Body for `POST /host/kv/set`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvSetRequest {
    pub key: String,
    pub value: serde_json::Value,
}

/// Body for `POST /host/kv/delete`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvDeleteRequest {
    pub key: String,
}

/// Response body for `POST /host/kv/get`. `None` means the key wasn't set.
///
/// Wire shape: `{"ok": true, "value": <json-or-null>}` on success,
/// `{"ok": false, "err": "..."}` on failure. Using a flat struct keeps
/// the new routes easy to debug with curl.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvGetResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// Plain OK/err response shared by `set` / `delete` / `clear`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvVoidResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

// ─── Open-external wire types ─────────────────────────────────────────────────

/// Body for `POST /host/open-external`.
///
/// The host validates that `url` begins with `http://` or `https://` before
/// forwarding to the system browser. Non-HTTP(S) schemes (`javascript:`,
/// `file:`, etc.) are rejected with HTTP 400 to prevent protocol-handler abuse.
///
/// Only native shells (Wry/desktop, poly-host daemon) register this route.
/// Web and Electron shells do not need it: web uses a plain `<a target="_blank">`,
/// and Electron's `setWindowOpenHandler` already forwards `window.open` calls to
/// `shell.openExternal` (verified at `apps/desktop-electron-web/electron/main.js:115-118`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenExternalRequest {
    pub url: String,
}

/// Response body for `POST /host/open-external`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenExternalResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

// ─── Plugin-KV wire types ─────────────────────────────────────────────────────
//
// Plugin-scoped KV separates plugin state from app state. Keys are namespaced
// by `plugin` (always) and optionally `account` (for per-account credentials
// etc.), so two plugins can use the same user-facing key without colliding.
// Values are opaque bytes transported as base64 — the server stores the base64
// string as a JSON string inside the existing `poly_kv` TEXT payload column,
// so no schema migration is needed.

/// Body for `POST /host/plugin-kv/get`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginKvGetRequest {
    pub plugin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub key: String,
}

/// Body for `POST /host/plugin-kv/set`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginKvSetRequest {
    pub plugin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub key: String,
    /// Base64-encoded bytes (standard alphabet).
    pub value_b64: String,
}

/// Body for `POST /host/plugin-kv/delete`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginKvDeleteRequest {
    pub plugin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub key: String,
}

/// Response body for `POST /host/plugin-kv/get`.
///
/// `value_b64 = None` means the key was unset. `ok = false` means the
/// backend errored (details in `err`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginKvGetResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

// ─── Bundled-plugin constants (shared across crates) ─────────────────────────
//
// Mirrors the canonical list in `crates/core/src/bundled_plugins.rs`.
// Shipped here so non-UI crates (`poly-host`) can recognise bundled URLs
// without depending on the heavier `poly-core` crate. Keep in sync.

/// URL scheme used to identify bundled plugins (e.g. `bundled://discord`).
pub const BUNDLED_URL_SCHEME: &str = "bundled://";

/// Slugs of every plugin auto-injected into `wasm_plugins` at startup.
/// Discord and Teams are bundled but never built-in (app-store policy).
pub const BUNDLED_PLUGIN_SLUGS: &[&str] = &["discord", "teams"];

/// Returns `true` if `url` targets a bundled plugin.
#[must_use]
pub fn is_bundled_url(url: &str) -> bool {
    url.starts_with(BUNDLED_URL_SCHEME)
}

/// Strip the `bundled://` prefix and return the slug, if `url` matches.
#[must_use]
pub fn bundled_slug_from_url(url: &str) -> Option<&str> {
    url.strip_prefix(BUNDLED_URL_SCHEME)
}

// ─── Plugin admin sub-route payloads ─────────────────────────────────────────
//
// These wire types let an external AI agent (or any HTTP client) drive
// the plugin / account-token administration surface that the settings UI
// already provides. The native shell handles the call by mutating the
// shared `app_settings` and `account_tokens` rows in `poly_kv`.

/// Sub-route for `POST /host/plugins/add` — sideload a plugin from a URL.
pub const ROUTE_PLUGINS_ADD: &str = "/host/plugins/add";
/// Sub-route for `POST /host/plugins/remove` — drop a plugin by URL or slug.
pub const ROUTE_PLUGINS_REMOVE: &str = "/host/plugins/remove";
/// Sub-route for `POST /host/plugins/set-enabled` — toggle a plugin on/off.
pub const ROUTE_PLUGINS_SET_ENABLED: &str = "/host/plugins/set-enabled";
/// Sub-route for `GET /host/plugins/list` — enumerate all known plugins.
pub const ROUTE_PLUGINS_LIST: &str = "/host/plugins/list";
/// Sub-route for `POST /host/accounts/add` — persist an account token.
pub const ROUTE_ACCOUNTS_ADD: &str = "/host/accounts/add";
/// Sub-route for `POST /host/accounts/remove` — drop an account token.
pub const ROUTE_ACCOUNTS_REMOVE: &str = "/host/accounts/remove";
/// Sub-route for `GET /host/accounts/list` — enumerate stored account tokens.
pub const ROUTE_ACCOUNTS_LIST: &str = "/host/accounts/list";

/// Body for `POST /host/plugins/add`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAddRequest {
    /// Source URL — `https://`, `http://`, `file://`, or `bundled://<slug>`.
    pub url: String,
    /// Optional display name. Defaults to the URL hostname when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Response body for `POST /host/plugins/add`.
///
/// `slug` is the bundled-slug for `bundled://<slug>` URLs, otherwise an
/// empty string. `added = false` means the plugin was already present
/// (idempotent re-adds).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAddResponse {
    pub ok: bool,
    #[serde(default)]
    pub added: bool,
    #[serde(default)]
    pub slug: String,
    /// The full URL as recorded in `app_settings.wasm_plugins`.
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// Body for `POST /host/plugins/remove`. Accepts either a full URL
/// (`https://example.com/p.wasm`) or a bare bundled slug (`discord`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRemoveRequest {
    pub url_or_slug: String,
}

/// Response body for `POST /host/plugins/remove`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRemoveResponse {
    pub ok: bool,
    #[serde(default)]
    pub removed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// Body for `POST /host/plugins/set-enabled`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSetEnabledRequest {
    pub url: String,
    pub enabled: bool,
}

/// Response body for `POST /host/plugins/set-enabled`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSetEnabledResponse {
    pub ok: bool,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// One entry in the `GET /host/plugins/list` response — the wire shape
/// matches the storage `WasmPluginEntry` plus a `kind` discriminator
/// for easy client-side branching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginListEntry {
    /// `"builtin"` (compiled-in backend) or `"sideloaded"` (WASM plugin
    /// from `wasm_plugins`). Bundled plugins (`bundled://<slug>`) report
    /// `"sideloaded"` with `bundled = true` so the client can branch.
    pub kind: String,
    pub slug: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub bundled: bool,
}

/// Response body for `GET /host/plugins/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginListResponse {
    pub ok: bool,
    #[serde(default)]
    pub plugins: Vec<PluginListEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// Body for `POST /host/accounts/add`. Mirrors the storage `AccountToken`
/// shape so callers can construct one without an extra translation step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountAddRequest {
    pub backend: String,
    pub account_id: String,
    pub token: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// Response body for `POST /host/accounts/add`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountAddResponse {
    pub ok: bool,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// Body for `POST /host/accounts/remove`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRemoveRequest {
    pub backend: String,
    pub account_id: String,
}

/// Response body for `POST /host/accounts/remove`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRemoveResponse {
    pub ok: bool,
    #[serde(default)]
    pub removed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// Response body for `GET /host/accounts/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountListResponse {
    pub ok: bool,
    #[serde(default)]
    pub accounts: Vec<AccountListEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// One entry in the `GET /host/accounts/list` response. Sensitive fields
/// (`token`, `refresh_token`) are deliberately **not** serialized — the
/// listing is meant for inventory / display, not key-material recovery.
///
/// # Why the redaction is load-bearing (Phase C.5)
///
/// It used to be cosmetic: anyone who could call this route could equally
/// call `POST /host/kv/get {"key":"account_tokens"}` and get the tokens
/// verbatim, so omitting them here changed nothing. After Phase C that
/// row is host-internal — the only callers who may name it are the shell
/// itself, and the payload is sealed at rest (`secret_seal`). This
/// inventory is therefore the widest account view that exists, and it is
/// the shape that flows into logs, MCP transcripts and agent tooling.
/// Adding `token` / `refresh_token` here would re-open the disclosure
/// path that C.1–C.3 closed, so the `serde` surface of this struct is a
/// security boundary, not a display preference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountListEntry {
    pub backend: String,
    pub account_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_expires_at: Option<String>,
}

// ─── Errors ──────────────────────────────────────────────────────────────────

/// Errors returned by [`Client::call`].
#[derive(Debug, Error)]
pub enum BridgeError {
    /// The native shell isn't running, or doesn't bind the bridge port on
    /// this platform yet (e.g. mobile shells without subprocess support).
    #[error("host bridge unreachable at {url}: {source}")]
    Unreachable {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    /// HTTP / transport-level error after the bridge accepted the request.
    #[error("host bridge transport error: {0}")]
    Transport(#[from] reqwest::Error),
    /// The bridge returned a JSON body we couldn't parse.
    #[error("host bridge response not valid JSON: {0}")]
    ParseResponse(String),
    /// The bridge returned an `Err` payload (the host operation itself failed).
    #[error("host operation failed: {0}")]
    Host(String),
    /// The response was the wrong variant for the call we made
    /// (e.g. we asked for `exec-command` and got an `http-response` back).
    #[error("host bridge returned mismatched variant for {call}: {got}")]
    VariantMismatch { call: &'static str, got: String },
}

// ─── Client ──────────────────────────────────────────────────────────────────

/// Typed client for the host bridge.
///
/// Compiles for both native and WASM (uses `reqwest`, which uses fetch on
/// `wasm32-unknown-unknown`). Cheap to construct — clone freely.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    /// Legacy single-dispatch URL (`…/host`).
    url: String,
    /// Base URL for the new per-category sub-routes (`…`). Appended
    /// with `/host/kv/...` etc. when building KV requests.
    base: String,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Build a client targeting the default bridge URL.
    ///
    /// On native, this is the loopback `http://127.0.0.1:9333` standalone
    /// daemon. On `wasm32`, it's the origin that served the current
    /// document (read from `window.location.origin`) — so a fullstack
    /// shell like `apps/web` on port 3000 gets same-origin `/host/*`
    /// requests hitting its own axum router instead of a cross-origin
    /// request to an unreachable sidecar.
    ///
    /// Falls back to [`BRIDGE_BASE_URL`] on wasm32 only if `window` or
    /// `location.origin()` is unavailable (non-browser WASM host).
    #[must_use]
    pub fn new() -> Self {
        let base = default_base_url();
        let url = format!("{base}{BRIDGE_PATH}");
        Self {
            http: reqwest::Client::new(),
            url,
            base,
        }
    }

    /// Build a client targeting an explicit bridge URL — useful for tests
    /// or for shells that bind a non-default port. Both the legacy
    /// single-dispatch URL and the sub-route base are derived from
    /// `url`: the last `/host` segment (if any) is stripped to form
    /// `base`.
    #[must_use]
    pub fn with_url(url: impl Into<String>) -> Self {
        let url = url.into();
        let base = url
            .strip_suffix("/host")
            .map_or_else(|| url.clone(), str::to_string);
        Self {
            http: reqwest::Client::new(),
            url,
            base,
        }
    }

    /// Send one [`HostCall`] and decode the response.
    ///
    /// Returns the typed [`HostOk`] payload on success, or [`BridgeError`]
    /// on transport / dispatch failure.
    pub async fn call(&self, call: HostCall) -> Result<HostOk, BridgeError> {
        self.call_at(&self.url, &call).await
    }

    /// Send a [`HostCall`] to an explicit endpoint under this client's
    /// base URL, with the shell session token attached.
    async fn call_at(&self, url: &str, call: &HostCall) -> Result<HostOk, BridgeError> {
        let resp = host_auth::send_authorized(
            &self.http,
            &self.base,
            self.http.post(url).json(call),
        )
        .await
        .map_err(|e| BridgeError::Unreachable {
            url: url.to_string(),
            source: e,
        })?;

        let body = resp.text().await?;
        let parsed: HostResponse =
            serde_json::from_str(&body).map_err(|e| BridgeError::ParseResponse(e.to_string()))?;
        match parsed {
            HostResponse::Ok(ok) => Ok(ok),
            HostResponse::Err(msg) => Err(BridgeError::Host(msg)),
        }
    }

    /// Ping the bridge liveness endpoint. Returns `Ok(())` if the shell
    /// is reachable. Used by callers that want to degrade gracefully
    /// before making real requests.
    pub async fn status(&self) -> Result<(), BridgeError> {
        let url = format!("{}{}", self.base, ROUTE_STATUS);
        let _resp =
            host_auth::send_authorized(&self.http, &self.base, self.http.get(&url))
                .await
                .map_err(|e| BridgeError::Unreachable {
                    url: url.clone(),
                    source: e,
                })?;
        Ok(())
    }

    /// `POST /host/kv/get` — read an app-level KV value.
    ///
    /// Returns `Ok(None)` for a missing key, `Ok(Some(value))` on hit,
    /// or `BridgeError::Host` if the shell reported a backend error.
    pub async fn kv_get(&self, key: &str) -> Result<Option<serde_json::Value>, BridgeError> {
        let url = format!("{}{}", self.base, ROUTE_KV_GET);
        let req = KvGetRequest {
            key: key.to_string(),
        };
        let resp: KvGetResponse = self.post_json(&url, &req).await?;
        if resp.ok {
            Ok(resp.value)
        } else {
            Err(BridgeError::Host(
                resp.err.unwrap_or_else(|| "unknown backend error".into()),
            ))
        }
    }

    /// `POST /host/kv/set` — write an app-level KV value.
    pub async fn kv_set(
        &self,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), BridgeError> {
        let url = format!("{}{}", self.base, ROUTE_KV_SET);
        let req = KvSetRequest {
            key: key.to_string(),
            value,
        };
        let resp: KvVoidResponse = self.post_json(&url, &req).await?;
        Self::expect_ok(resp)
    }

    /// `POST /host/kv/delete` — delete an app-level KV key.
    pub async fn kv_delete(&self, key: &str) -> Result<(), BridgeError> {
        let url = format!("{}{}", self.base, ROUTE_KV_DELETE);
        let req = KvDeleteRequest {
            key: key.to_string(),
        };
        let resp: KvVoidResponse = self.post_json(&url, &req).await?;
        Self::expect_ok(resp)
    }

    /// `POST /host/kv/clear` — wipe all app-level KV entries.
    pub async fn kv_clear(&self) -> Result<(), BridgeError> {
        let url = format!("{}{}", self.base, ROUTE_KV_CLEAR);
        let resp: KvVoidResponse = self.post_json(&url, &serde_json::json!({})).await?;
        Self::expect_ok(resp)
    }

    /// `POST /host/open-external` — ask the native shell to open `url` in the
    /// system browser.
    ///
    /// Only relevant for the **Wry desktop shell** (and the standalone
    /// `poly-host` daemon). Web and Electron shells open URLs directly via
    /// `<a target="_blank">` / `setWindowOpenHandler`; they don't register this
    /// route and will return `BridgeError::Unreachable` or 404.
    ///
    /// # Errors
    ///
    /// Returns `BridgeError::Host` if the shell rejects the URL (e.g. non-HTTP(S)
    /// scheme), and `BridgeError::Unreachable` if the shell is not running.
    pub async fn open_external(&self, url: &str) -> Result<(), BridgeError> {
        let endpoint = format!("{}{}", self.base, ROUTE_OPEN_EXTERNAL);
        let req = OpenExternalRequest {
            url: url.to_string(),
        };
        let resp: OpenExternalResponse = self.post_json(&endpoint, &req).await?;
        if resp.ok {
            Ok(())
        } else {
            Err(BridgeError::Host(
                resp.err.unwrap_or_else(|| "open-external failed".into()),
            ))
        }
    }

    /// Shared POST+decode helper for the sub-routes.
    async fn post_json<T: serde::de::DeserializeOwned + Send, B: Serialize + Sync>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T, BridgeError> {
        let resp = host_auth::send_authorized(
            &self.http,
            &self.base,
            self.http.post(url).json(body),
        )
        .await
        .map_err(|e| BridgeError::Unreachable {
            url: url.to_string(),
            source: e,
        })?;
        let text = resp.text().await?;
        serde_json::from_str(&text).map_err(|e| BridgeError::ParseResponse(e.to_string()))
    }

    fn expect_ok(resp: KvVoidResponse) -> Result<(), BridgeError> {
        if resp.ok {
            Ok(())
        } else {
            Err(BridgeError::Host(
                resp.err.unwrap_or_else(|| "unknown backend error".into()),
            ))
        }
    }

    /// Return a [`client_config::ClientConfigStore`] backed by this client.
    ///
    /// The store shares the same HTTP client and bridge URL as `self`, so
    /// there is no extra connection overhead. Callers that already hold a
    /// [`Client`] should use this rather than constructing a new store.
    #[must_use]
    pub fn client_config(&self) -> client_config::ClientConfigStore {
        client_config::ClientConfigStore::from_client(self.clone())
    }

    /// Convenience: run an [`HostCall::ExecCommand`] and decode the
    /// `ExecOutput` variant. Returns `(exit_code, stdout, stderr)`.
    ///
    /// Posts to [`ROUTE_EXEC`], **not** the legacy `/host` dispatch
    /// endpoint — the latter no longer executes anything (Phase B.5).
    /// `program` must be an absolute path that the calling identity
    /// declared and the user consented to, or the shell answers with
    /// [`BridgeError::Host`].
    pub async fn exec(
        &self,
        program: impl Into<String>,
        args: Vec<String>,
    ) -> Result<(i32, Vec<u8>, Vec<u8>), BridgeError> {
        let url = format!("{}{}", self.base, ROUTE_EXEC);
        let ok = self
            .call_at(
                &url,
                &HostCall::ExecCommand {
                    program: program.into(),
                    args,
                },
            )
            .await?;
        match ok {
            HostOk::ExecOutput {
                exit_code,
                stdout_b64,
                stderr_b64,
            } => {
                let stdout = b64_decode(&stdout_b64)
                    .map_err(|e| BridgeError::ParseResponse(format!("stdout_b64: {e}")))?;
                let stderr = b64_decode(&stderr_b64)
                    .map_err(|e| BridgeError::ParseResponse(format!("stderr_b64: {e}")))?;
                Ok((exit_code, stdout, stderr))
            }
            other @ HostOk::HttpResponse { .. } => Err(BridgeError::VariantMismatch {
                call: "exec-command",
                got: variant_name(&other).to_string(),
            }),
        }
    }
}

const fn variant_name(ok: &HostOk) -> &'static str {
    match ok {
        HostOk::ExecOutput { .. } => "exec-output",
        HostOk::HttpResponse { .. } => "http-response",
    }
}

/// Default bridge base URL. On native, the loopback daemon. In a real
/// browser, the origin that served the WASM bundle.
#[cfg(not(target_arch = "wasm32"))]
fn default_base_url() -> String {
    BRIDGE_BASE_URL.to_string()
}

#[cfg(target_arch = "wasm32")]
fn default_base_url() -> String {
    web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_else(|| BRIDGE_BASE_URL.to_string())
}

// ─── Server-side dispatcher (Rust shells only) ───────────────────────────────

/// Run a [`HostCall`] using the host's real OS capabilities and produce a
/// [`HostResponse`]. This is the function Rust shells (apps/desktop-web)
/// hand to their HTTP framework.
///
/// **Not available on WASM** — the dispatcher needs subprocess / network
/// access that wasm32-unknown-unknown doesn't have. WASM-side callers use
/// [`Client`] instead, which routes through whichever shell *is* native.
///
/// # Exec is not reachable from here
///
/// [`HostCall::ExecCommand`] is answered with an error rather than being
/// executed. Spawning a subprocess now requires a verified
/// [`CallerId`](host_auth::CallerId) and an
/// [`ExecPolicy`](exec_policy::ExecPolicy) decision, neither of which a
/// bare `HostCall` carries — see [`dispatch_exec`]. This is Phase B.5 of
/// `docs/plans/plan-host-substrate-capability-gating.md`: the legacy
/// `POST /host` tagged-union endpoint used to be a second, ungated way
/// into `exec_command`, and there is now exactly one gated entry point.
#[cfg(not(target_arch = "wasm32"))]
pub async fn dispatch(call: HostCall) -> HostResponse {
    match call {
        HostCall::ExecCommand { .. } => HostResponse::Err(
            "exec is not available through the legacy /host dispatch endpoint; \
             POST /host/exec with an authenticated caller identity instead"
                .to_string(),
        ),
        HostCall::HttpRequest {
            method,
            url,
            headers,
            body_b64,
        } => http_request(method, url, headers, body_b64).await,
    }
}

/// The single gated entry point for subprocess execution.
///
/// `caller` must come from [`host_auth::HostAuth::verify`] — never from a
/// request body — and `policy` decides what that caller declared and
/// whether the user consented. The program actually spawned is the
/// canonicalised path [`exec_policy::check_exec`] returned, not the string
/// the caller sent, so the check and the spawn cannot disagree.
#[cfg(not(target_arch = "wasm32"))]
pub async fn dispatch_exec(
    policy: &dyn exec_policy::ExecPolicy,
    caller: &host_auth::CallerId,
    program: &str,
    args: &[String],
) -> Result<HostResponse, exec_policy::ExecDenied> {
    let resolved = exec_policy::check_exec(policy, caller, program)?;
    Ok(exec_command(&resolved, args).await)
}

#[cfg(not(target_arch = "wasm32"))]
async fn exec_command(program: &std::path::Path, args: &[String]) -> HostResponse {
    use std::process::Stdio;
    use tokio::process::Command;

    let mut cmd = Command::new(program);
    let _c = cmd.args(args);
    let _c = cmd.stdin(Stdio::null());
    let _c = cmd.stdout(Stdio::piped());
    let _c = cmd.stderr(Stdio::piped());

    match cmd.output().await {
        Ok(output) => HostResponse::Ok(HostOk::ExecOutput {
            exit_code: output.status.code().unwrap_or(-1_i32),
            stdout_b64: b64_encode(&output.stdout),
            stderr_b64: b64_encode(&output.stderr),
        }),
        Err(e) => HostResponse::Err(format!("failed to spawn `{}`: {e}", program.display())),
    }
}

/// Maximum number of redirects the guarded proxy will follow. Matches
/// reqwest's own default; each hop is re-validated by [`net_guard::check_url`].
#[cfg(not(target_arch = "wasm32"))]
const MAX_REDIRECT_HOPS: u8 = 10;

/// Headers that must never survive a cross-origin redirect.
#[cfg(not(target_arch = "wasm32"))]
const CREDENTIAL_HEADERS: &[&str] = &["authorization", "cookie", "proxy-authorization"];

/// One outbound request, re-buildable across redirect hops.
#[cfg(not(target_arch = "wasm32"))]
struct OutboundRequest {
    method: reqwest::Method,
    url: reqwest::Url,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}

#[cfg(not(target_arch = "wasm32"))]
async fn http_request(
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body_b64: Option<String>,
) -> HostResponse {
    let body = match body_b64.as_deref() {
        Some(b64) => match b64_decode(b64) {
            Ok(bytes) => Some(bytes),
            Err(e) => return HostResponse::Err(format!("invalid body_b64: {e}")),
        },
        None => None,
    };

    let method_parsed = match reqwest::Method::from_bytes(method.as_bytes()) {
        Ok(m) => m,
        Err(e) => return HostResponse::Err(format!("invalid method: {e}")),
    };

    let parsed_url = match reqwest::Url::parse(&url) {
        Ok(u) => u,
        Err(e) => return HostResponse::Err(format!("invalid url: {e}")),
    };

    // Redirects are followed by hand so every hop passes the SSRF guard —
    // reqwest's built-in follower would only ever validate the first URL.
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(c) => c,
        Err(e) => return HostResponse::Err(format!("http client build failed: {e}")),
    };

    let request = OutboundRequest {
        method: method_parsed,
        url: parsed_url,
        headers,
        body,
    };
    let policy = net_guard::PrivateNetworkPolicy::from_env();

    match send_guarded(&client, request, policy).await {
        Ok(resp) => collect_response(resp).await,
        Err(e) => HostResponse::Err(e),
    }
}

/// Send `request`, following redirects manually and re-validating the
/// destination of every hop.
#[cfg(not(target_arch = "wasm32"))]
async fn send_guarded(
    client: &reqwest::Client,
    mut request: OutboundRequest,
    policy: net_guard::PrivateNetworkPolicy,
) -> Result<reqwest::Response, String> {
    let mut hops: u8 = 0;
    loop {
        net_guard::check_url(&request.url, policy).await?;

        let mut builder = client.request(request.method.clone(), request.url.clone());
        for (k, v) in &request.headers {
            builder = builder.header(k, v);
        }
        if let Some(bytes) = request.body.clone() {
            builder = builder.body(bytes);
        }
        let resp = builder
            .send()
            .await
            .map_err(|e| format!("http request failed: {e}"))?;

        match next_hop(&resp, &request)? {
            None => return Ok(resp),
            Some(next) => {
                hops = hops
                    .checked_add(1)
                    .ok_or_else(|| "redirect counter overflowed".to_string())?;
                if hops > MAX_REDIRECT_HOPS {
                    return Err(format!("too many redirects (> {MAX_REDIRECT_HOPS})"));
                }
                request = next;
            }
        }
    }
}

/// Build the follow-up request for a 3xx response, or `None` when the
/// response is final.
#[cfg(not(target_arch = "wasm32"))]
fn next_hop(
    resp: &reqwest::Response,
    current: &OutboundRequest,
) -> Result<Option<OutboundRequest>, String> {
    let status = resp.status();
    if !status.is_redirection() {
        return Ok(None);
    }
    let Some(location) = resp.headers().get(reqwest::header::LOCATION) else {
        return Ok(None);
    };
    let location = location
        .to_str()
        .map_err(|e| format!("redirect Location header is not valid ASCII: {e}"))?;
    let target = current
        .url
        .join(location)
        .map_err(|e| format!("invalid redirect Location `{location}`: {e}"))?;

    // 303 (and, per browser behaviour, 301/302) degrade to GET without a
    // body; 307/308 replay the original method and body verbatim.
    let replay = status == reqwest::StatusCode::TEMPORARY_REDIRECT
        || status == reqwest::StatusCode::PERMANENT_REDIRECT;
    let (method, body) = if replay || current.method == reqwest::Method::HEAD {
        (current.method.clone(), current.body.clone())
    } else {
        (reqwest::Method::GET, None)
    };

    let same_origin = target.origin() == current.url.origin();
    let headers = current
        .headers
        .iter()
        .filter(|(k, _)| same_origin || !CREDENTIAL_HEADERS.contains(&k.to_ascii_lowercase().as_str()))
        .cloned()
        .collect();

    Ok(Some(OutboundRequest {
        method,
        url: target,
        headers,
        body,
    }))
}

#[cfg(not(target_arch = "wasm32"))]
async fn collect_response(resp: reqwest::Response) -> HostResponse {
    let status = resp.status().as_u16();
    let resp_headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    match resp.bytes().await {
        Ok(bytes) => HostResponse::Ok(HostOk::HttpResponse {
            status,
            headers: resp_headers,
            body_b64: b64_encode(&bytes),
        }),
        Err(e) => HostResponse::Err(format!("read body: {e}")),
    }
}

// ─── base64 helpers ──────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
fn b64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(s.as_bytes())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn bundled_url_helpers_round_trip() {
        for slug in BUNDLED_PLUGIN_SLUGS {
            let url = format!("{BUNDLED_URL_SCHEME}{slug}");
            assert!(is_bundled_url(&url));
            assert_eq!(bundled_slug_from_url(&url), Some(*slug));
        }
        assert!(!is_bundled_url("https://x.test/p.wasm"));
        assert_eq!(bundled_slug_from_url("https://x.test/p.wasm"), None);
    }

    /// Lock-in: the bundled-plugin contract is Discord + Teams. Adding a
    /// new slug requires a coordinated update to
    /// `crates/core/src/bundled_plugins.rs` (which the UI uses) and
    /// every consumer of `BUNDLED_PLUGIN_SLUGS`. Don't bump this without
    /// updating both sides.
    #[test]
    fn bundled_plugin_slug_set_is_locked() {
        assert_eq!(BUNDLED_PLUGIN_SLUGS, &["discord", "teams"]);
    }

    /// `/host/http` must not be usable as an SSRF proxy into the host's own
    /// loopback services or the cloud metadata endpoint. The default policy
    /// is deny; only `POLY_ALLOW_PRIVATE_NETWORK=1` re-opens it.
    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn dispatch_blocks_internal_http_targets() {
        for url in [
            "http://127.0.0.1:3000/host/kv/get",
            "http://[::1]:9333/host/exec",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.1/admin",
        ] {
            let resp = dispatch(HostCall::HttpRequest {
                method: "GET".into(),
                url: url.into(),
                headers: Vec::new(),
                body_b64: None,
            })
            .await;
            match resp {
                HostResponse::Err(msg) => assert!(
                    msg.contains("loopback/private/reserved"),
                    "{url} rejected for the wrong reason: {msg}"
                ),
                HostResponse::Ok(_) => panic!("{url} must not be proxied"),
            }
        }
    }

    /// Only http/https may be proxied — `file://` would turn the bridge into
    /// an arbitrary-file reader.
    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn dispatch_blocks_non_http_schemes() {
        let resp = dispatch(HostCall::HttpRequest {
            method: "GET".into(),
            url: "file:///etc/passwd".into(),
            headers: Vec::new(),
            body_b64: None,
        })
        .await;
        match resp {
            HostResponse::Err(msg) => {
                assert!(msg.contains("blocked URL scheme"), "unexpected: {msg}");
            }
            HostResponse::Ok(_) => panic!("file:// must not be proxied"),
        }
    }
}
