#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]
//! # poly-host (library surface)
//!
//! Reusable axum router + SQLite KV backend for the `/host/*` host-bridge
//! routes. Used by two processes:
//!
//! - `poly-host` binary (`src/main.rs`) — standalone daemon bound to
//!   `127.0.0.1:9333` so `apps/web` (running in a real browser) has a
//!   native side to talk to.
//! - `apps/desktop-web` Wry shell — mounts the same router on its own
//!   listener so the WASM inside the Wry webview sees identical `/host/*`
//!   behaviour without shipping a second copy of the code.
//!
//! The protocol types come from `poly-host-bridge` so the client and
//! server can't drift apart.
//!
//! See `docs/plans/phase-2.21-host-bridge-unification-plan.md`.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{Extension, Path as AxumPath, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use poly_host_bridge::exec_policy::{
    ConsentPrompt, ExecDenied, ExecPolicy, TracingConsentPrompt,
};
use poly_host_bridge::host_auth::{
    AUTH_DISABLE_ENV, CallerId, HostAuth, HostSessionResponse, PLUGIN_HEADER, ROUTE_SESSION,
};
use poly_host_bridge::secret_seal::{SEAL_PREFIX, SecretSealer, XChaChaSealer};
use poly_host_bridge::{
    AccountAddRequest, AccountAddResponse, AccountListEntry, AccountListResponse,
    AccountRemoveRequest, AccountRemoveResponse, HostCall, HostResponse, KvDeleteRequest,
    KvGetRequest, KvGetResponse, KvSetRequest, KvVoidResponse, OpenExternalRequest,
    OpenExternalResponse, PluginAddRequest, PluginAddResponse, PluginKvDeleteRequest,
    PluginKvGetRequest, PluginKvGetResponse, PluginKvSetRequest, PluginListEntry,
    PluginListResponse, PluginRemoveRequest, PluginRemoveResponse, PluginSetEnabledRequest,
    PluginSetEnabledResponse, dispatch,
};
#[cfg(feature = "video")]
use poly_host_bridge::video::{VideoState, close_session, decode_h264, encode_h264};
#[cfg(feature = "voice")]
use poly_host_bridge::{
    aead::{AeadState, router as aead_router},
    codec_opus::{OpusState, router as opus_router},
    udp::{UdpState, router as udp_router},
};
#[cfg(feature = "teams-webhook")]
use poly_host_bridge::teams_webhook::{
    ChangeNotification, ClientStateStore, NotificationSink, TeamsWebhookState,
    router as teams_webhook_router,
};
use sqlite::{Connection, ConnectionThreadSafe, State as SqlState};
use tower_http::cors::CorsLayer;

/// Shared daemon state — a SQLite handle plus the path we opened it from
/// (kept around so `GET /host/status` can report where storage lives).
///
/// Optionally holds the list of host capability strings advertised to the
/// WASM client via `GET /host/caps`. Call [`HostState::with_caps`] after
/// [`HostState::open`] to set them; defaults to an empty list.
///
/// Since `plan-host-substrate-capability-gating.md` it also carries the
/// three policy objects that make `/host/*` a confined surface rather than
/// an open one: the session-token verifier ([`HostAuth`]), the exec
/// allowlist/consent store ([`ExecPolicy`]) and the at-rest sealer for
/// credential rows ([`SecretSealer`]). All three are trait objects so a
/// test can substitute an in-memory implementation (SOLID item 7).
#[derive(Clone)]
pub struct HostState {
    db: Arc<Mutex<ConnectionThreadSafe>>,
    db_path: PathBuf,
    /// Capability strings returned by `GET /host/caps`.
    /// Each entry is a `HostCap` variant name (`"SandboxBrowser"` etc.).
    caps: Arc<Vec<String>>,
    /// Per-shell bearer-token verifier for every `/host/*` route.
    auth: Arc<HostAuth>,
    /// Origins the CORS layer and the token-mint route accept.
    origins: Arc<Vec<String>>,
    /// Declared-program + consent store backing `/host/exec`.
    exec_policy: Arc<dyn ExecPolicy>,
    /// Where a missing-consent denial is surfaced.
    consent_prompt: Arc<dyn ConsentPrompt>,
    /// Seals credential-bearing rows before they hit SQLite.
    sealer: Arc<dyn SecretSealer>,
}

impl HostState {
    /// Open (or create) the shared SQLite KV file.
    ///
    /// Mirrors `crates/core/src/storage/native.rs` exactly: one
    /// `poly_kv(key TEXT PK, payload TEXT)` table, 5s busy timeout. Using
    /// the same schema means the daemon and a locally-run apps/desktop
    /// native build can point at the same file.
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create data dir {}", parent.display()))?;
        }
        let mut db = Connection::open_thread_safe(&db_path)
            .with_context(|| format!("open sqlite at {}", db_path.display()))?;
        db.set_busy_timeout(5_000).context("set busy timeout")?;
        db.execute(
            "CREATE TABLE IF NOT EXISTS poly_kv (key TEXT PRIMARY KEY NOT NULL, payload TEXT NOT NULL)",
        )
        .context("create poly_kv table")?;
        let db = Arc::new(Mutex::new(db));
        let sealer = XChaChaSealer::from_key_file(&XChaChaSealer::key_path_for(&db_path))
            .context("open at-rest sealing key")?;
        let auth = HostAuth::mint().context("mint host session token")?;
        if !auth.enforced() {
            tracing::error!(
                "{AUTH_DISABLE_ENV}=1 — /host/* bearer authentication is DISABLED. \
                 Every local process (and every web page in the user's browser) can \
                 reach exec, KV and the plugin admin routes. Never ship with this set."
            );
        }
        Ok(Self {
            db: Arc::clone(&db),
            db_path,
            caps: Arc::new(Vec::new()),
            auth: Arc::new(auth),
            origins: Arc::new(default_shell_origins()),
            exec_policy: Arc::new(SqliteExecPolicy::new(db)),
            consent_prompt: Arc::new(TracingConsentPrompt),
            sealer: Arc::new(sealer),
        })
    }

    /// Set the host capabilities advertised by `GET /host/caps`.
    ///
    /// Call this after [`open`] with the caps from `poly_host_sandbox::advertised_host_caps()`.
    /// Each cap is a string variant name such as `"SandboxBrowser"`. Returns `self`
    /// for chaining.
    ///
    /// ```no_run
    /// # use poly_host::HostState;
    /// let state = HostState::open("/tmp/test.sqlite3").unwrap()
    ///     .with_caps(vec!["SandboxBrowser".to_string()]);
    /// ```
    #[must_use]
    pub fn with_caps(mut self, caps: Vec<String>) -> Self {
        self.caps = Arc::new(caps);
        self
    }

    /// Path to the SQLite file backing this handle. Useful for log output.
    #[must_use]
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// The bearer token this shell's own WASM bundle must present on every
    /// `/host/*` request.
    ///
    /// A shell that composes [`router`] into its own axum server shares a
    /// process with the client, so it can inject this value into the page
    /// instead of paying for a [`ROUTE_SESSION`] round trip.
    #[must_use]
    pub fn session_token(&self) -> &str {
        self.auth.token()
    }

    /// Derived token to hand a specific plugin. Confines that plugin to
    /// its own `plugin:{id}:` KV namespace and its own exec declarations.
    #[must_use]
    pub fn plugin_token(&self, plugin_id: &str) -> String {
        self.auth.derive_plugin_token(plugin_id)
    }

    /// Add `port` (on both `127.0.0.1` and `localhost`) to the accepted
    /// origin list.
    ///
    /// [`serve`] calls this with the address it actually bound, which is
    /// what Phase A.4 means by "derived from the bound port": the
    /// hard-coded defaults only cover the three fullstack shells, which
    /// mount [`router`] directly and never call [`serve`].
    #[must_use]
    pub fn with_bound_port(mut self, port: u16) -> Self {
        let mut origins = (*self.origins).clone();
        for host in ["127.0.0.1", "localhost", "[::1]"] {
            let origin = format!("http://{host}:{port}");
            if !origins.contains(&origin) {
                origins.push(origin);
            }
        }
        self.origins = Arc::new(origins);
        self
    }

    /// Replace the whole accepted-origin list. Empty means "no browser
    /// origin is accepted", which is the right setting for a headless
    /// deployment.
    #[must_use]
    pub fn with_origins(mut self, origins: Vec<String>) -> Self {
        self.origins = Arc::new(origins);
        self
    }

    /// Substitute the exec allowlist/consent store — the seam tests use.
    #[must_use]
    pub fn with_exec_policy(mut self, policy: Arc<dyn ExecPolicy>) -> Self {
        self.exec_policy = policy;
        self
    }

    /// Substitute where consent prompts are surfaced.
    #[must_use]
    pub fn with_consent_prompt(mut self, prompt: Arc<dyn ConsentPrompt>) -> Self {
        self.consent_prompt = prompt;
        self
    }

    /// Substitute the at-rest sealer.
    #[must_use]
    pub fn with_sealer(mut self, sealer: Arc<dyn SecretSealer>) -> Self {
        self.sealer = sealer;
        self
    }

    /// Replace the token verifier — used by tests that need a known token.
    #[must_use]
    pub fn with_auth(mut self, auth: HostAuth) -> Self {
        self.auth = Arc::new(auth);
        self
    }
}

/// Loopback origins the shells are known to bind (see the platform table
/// in `CLAUDE.md`). Every entry is `http://` on loopback: a `https://`
/// or non-loopback origin is never one of ours.
fn default_shell_origins() -> Vec<String> {
    let mut out = Vec::new();
    for port in [3000_u16, 3001, 3002, poly_host_bridge::BRIDGE_PORT] {
        for host in ["127.0.0.1", "localhost", "[::1]"] {
            out.push(format!("http://{host}:{port}"));
        }
    }
    out
}

/// Build the full `/host/*` router over an already-open [`HostState`].
///
/// The caller is responsible for picking the listener address and for
/// deciding whether the router should be composed with additional routes
/// (the Wry shell does this to keep its MCP eval bridge on the same port
/// as before).
#[must_use = "the Router must be merged into the Dioxus router or served directly"]
pub fn router(state: HostState) -> Router {
    let cors = cors_layer(&state);

    let base = Router::new()
        .route("/host/status", get(status))
        // Phase A.2: the one route exempt from bearer verification — it is
        // how the shell's own bundle *learns* the bearer token. Gated on
        // request provenance instead; see `host_session`.
        .route(ROUTE_SESSION, get(host_session))
        // D.3: Host capabilities — lets the WASM UI ask which sandbox/host-cap
        // features the running shell supports. Response: `{ "caps": [...] }`.
        .route("/host/caps", get(host_caps))
        .route("/host/kv/get", post(kv_get))
        .route("/host/kv/set", post(kv_set))
        .route("/host/kv/delete", post(kv_delete))
        .route("/host/kv/clear", post(kv_clear))
        .route("/host/plugin-kv/get", post(plugin_kv_get))
        .route("/host/plugin-kv/set", post(plugin_kv_set))
        .route("/host/plugin-kv/delete", post(plugin_kv_delete))
        .route("/host/exec", post(host_exec))
        .route("/host/exec/declare", post(host_exec_declare))
        .route("/host/exec/consent", post(host_exec_consent))
        .route("/host/http", post(host_http))
        .route("/host/plugins/add", post(plugins_add))
        .route("/host/plugins/remove", post(plugins_remove))
        .route("/host/plugins/set-enabled", post(plugins_set_enabled))
        .route("/host/plugins/list", get(plugins_list))
        .route("/host/accounts/add", post(accounts_add))
        .route("/host/accounts/remove", post(accounts_remove))
        .route("/host/accounts/list", get(accounts_list))
        .route("/host/open-external", post(open_external))
        .route("/host", post(host_legacy))
        // C.1: Sandbox redirect shim — OAuth providers redirect to this URL;
        // the shim postMessages the captured URL back to the opener popup.
        // The OAuth provider MUST be configured with `<origin>/sandbox/<id>`
        // as the redirect target (see docs/plans/plan-host-sandbox-impl.md C.4).
        .route("/sandbox/{id}", get(sandbox_shim))
        .route("/poly-service-worker.js", get(poly_service_worker))
        .with_state(state.clone());

    // Mount video H.264 encode/decode routes when the `video` feature is enabled.
    // Video state is separate from HostState (no SQLite needed — it's all in-memory
    // encoder/decoder maps) so we use .merge() with its own with_state call.
    #[cfg(feature = "video")]
    let base = {
        let video_router = Router::new()
            .route("/host/video/encode_h264", post(encode_h264))
            .route("/host/video/decode_h264", post(decode_h264))
            .route("/host/video/close_session", post(close_session))
            .with_state(VideoState::new());
        base.merge(video_router)
    };

    // Mount generic voice transport primitives when the `voice` feature is enabled.
    // voice = voice-primitives = udp + codec-opus + aead. Discord-specific protocol
    // (WS handshake, RTP framing) runs in the discord plugin, not here.
    #[cfg(feature = "voice")]
    let base = {
        let udp_r = udp_router(UdpState::new());
        let opus_r = opus_router(OpusState::new());
        let aead_r = aead_router(AeadState::new());
        base.merge(udp_r).merge(opus_r).merge(aead_r)
    };

    // Mount the Teams webhook relay when `teams-webhook` is on. Default
    // ClientStateStore / NotificationSink are in-memory + tracing-only —
    // production deployments swap them via direct teams_webhook_router(…)
    // mount in their own server crate. See
    // docs/plans/plan-teams-graph-subscriptions.md Phase C.
    #[cfg(feature = "teams-webhook")]
    let base = {
        let webhook_state = TeamsWebhookState::new(
            std::sync::Arc::new(InMemoryClientStateStore::default()),
            std::sync::Arc::new(TracingNotificationSink),
        );
        let teams_r = teams_webhook_router(webhook_state);
        base.merge(teams_r)
    };

    // Layer order matters and is load-bearing:
    //
    //   CORS (outermost) → bearer verification → routes
    //
    // A CORS preflight (`OPTIONS`, no `Authorization` header by
    // specification) must be answered by the CORS layer and must never
    // reach the verifier, or every cross-origin-looking request from the
    // shell's own page would 401 on the preflight. `Router::layer` makes
    // the last-applied layer outermost, so CORS is applied last.
    base.layer(axum::middleware::from_fn_with_state(
        state,
        require_host_auth,
    ))
    .layer(cors)
}

/// Explicit-origin CORS. `Any` must not appear anywhere in this crate
/// (Phase A.4): with `allow_origin(Any)` every page in the user's browser
/// could read `/host/*` replies cross-origin.
fn cors_layer(state: &HostState) -> CorsLayer {
    use axum::http::Method;

    let origins: Vec<HeaderValue> = state
        .origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
            axum::http::HeaderName::from_static(PLUGIN_HEADER),
        ])
}

/// Reject every `/host/*` request that does not carry a valid session
/// token, and stamp the verified [`CallerId`] into the request extensions
/// so downstream handlers scope on identity rather than on request fields.
///
/// Non-`/host` routes on this router (`/sandbox/{id}`, the service worker)
/// are top-level browser navigations that cannot carry an `Authorization`
/// header; they are deliberately untouched — neither reads or writes any
/// state.
async fn require_host_auth(
    State(state): State<HostState>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let path = req.uri().path().to_string();
    let guarded = path == "/host" || path.starts_with("/host/");
    if !guarded {
        return next.run(req).await;
    }
    // Anti-DNS-rebinding. A page served from `http://evil.test:3000` whose
    // DNS has been rebound to 127.0.0.1 is *same-origin* with itself, so
    // neither `Origin` nor `Sec-Fetch-Site` marks it as foreign — but its
    // `Host` header still says `evil.test`. Every legitimate caller reaches
    // this router over loopback by name or by literal.
    if let Some(host) = header_str(req.headers(), header::HOST.as_str())
        && !is_loopback_host(&host)
    {
        tracing::warn!(path = %path, host = %host, "rejected non-loopback Host header");
        return deny(
            StatusCode::FORBIDDEN,
            &format!("Host `{host}` is not a loopback name; refusing /host/* request"),
        );
    }
    if path == ROUTE_SESSION || UNAUTHENTICATED_ROUTES.contains(&path.as_str()) {
        return next.run(req).await;
    }
    match resolve_caller(&state, &path, req.headers()) {
        Ok(caller) => {
            let _prev = req.extensions_mut().insert(caller);
            next.run(req).await
        }
        Err(refusal) => *refusal,
    }
}

/// Verify the presented credentials and apply the shell-only route rule.
fn resolve_caller(
    state: &HostState,
    path: &str,
    headers: &HeaderMap,
) -> Result<CallerId, Box<axum::response::Response>> {
    let authorization = header_str(headers, header::AUTHORIZATION.as_str());
    let plugin = header_str(headers, PLUGIN_HEADER);
    let caller = state
        .auth
        .verify(authorization.as_deref(), plugin.as_deref())
        .map_err(|e| {
            tracing::warn!(path = %path, error = %e, "rejected unauthenticated /host/* request");
            Box::new(deny(StatusCode::UNAUTHORIZED, &e.to_string()))
        })?;
    if caller != CallerId::Shell && is_shell_only_route(path) {
        tracing::warn!(path = %path, caller = %caller.label(), "rejected non-shell caller");
        return Err(Box::new(deny(
            StatusCode::FORBIDDEN,
            &format!("{} may not call {path}", caller.label()),
        )));
    }
    Ok(caller)
}

fn deny(status: StatusCode, err: &str) -> axum::response::Response {
    (
        status,
        Json(serde_json::json!({ "ok": false, "err": err })),
    )
        .into_response()
}

/// Probe routes deliberately left outside bearer verification.
///
/// Both are read-only, carry no user data and have no side effect, and the
/// development MCPs (`mcp/*-devtools-mcp`) poll `/host/status` to decide
/// when a shell's server half is up — before any WASM has run, so before
/// any token could have been fetched. The cross-origin *read* of their
/// replies is still blocked by the origin allowlist in [`cors_layer`].
const UNAUTHENTICATED_ROUTES: &[&str] = &["/host/status", "/host/caps"];

/// Hostnames that can only ever resolve to this machine.
fn is_loopback_host(host: &str) -> bool {
    let name = host
        .rsplit_once(':')
        .map_or(host, |(name, _port)| name)
        .trim_matches(|c| c == '[' || c == ']');
    matches!(name, "127.0.0.1" | "localhost" | "::1")
        || name.starts_with("127.")
        || name.ends_with(".localhost")
}

/// Routes only the shell itself may call.
///
/// Plugin administration, account-token mutation, opening a system
/// browser and recording exec declarations/consent are all decisions the
/// *user* makes through the app's own UI. A plugin holding a valid derived
/// token has no business in any of them — most obviously
/// `/host/accounts/*`, which would otherwise let a plugin enumerate or
/// delete another backend's credentials.
///
/// Note `/host/exec` itself is deliberately absent: plugins may ask to run
/// their own declared, consented programs. Only `/host/exec/declare` and
/// `/host/exec/consent` — the routes that *widen* that authority — are
/// shell-only.
fn is_shell_only_route(path: &str) -> bool {
    path.starts_with("/host/plugins/")
        || path.starts_with("/host/accounts/")
        || path.starts_with("/host/exec/")
        || path == "/host/open-external"
        || path == "/host/kv/clear"
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// `GET /host/session` — hand the caller this shell's bearer token.
///
/// Provenance gate (see the `host_auth` module docs for the full threat
/// model): a browser sets `Sec-Fetch-Site` itself and page script cannot
/// override it, and any cross-origin `fetch` also carries an `Origin`. We
/// require both to look like the shell's own page. A non-browser caller
/// (no `Origin`, no `Sec-Fetch-Site`) is a same-user local process, which
/// is outside this module's threat model and is allowed.
async fn host_session(
    State(state): State<HostState>,
    headers: HeaderMap,
) -> (StatusCode, Json<HostSessionResponse>) {
    if let Some(origin) = header_str(&headers, header::ORIGIN.as_str())
        && !state.origins.contains(&origin)
    {
        return refuse_session(&format!(
            "origin `{origin}` is not one of this shell's origins"
        ));
    }
    if let Some(site) = header_str(&headers, "sec-fetch-site")
        && !matches!(site.as_str(), "same-origin" | "same-site" | "none")
    {
        return refuse_session(&format!("cross-site fetch (`Sec-Fetch-Site: {site}`)"));
    }
    (
        StatusCode::OK,
        Json(HostSessionResponse {
            ok: true,
            token: state.auth.token().to_string(),
            err: None,
        }),
    )
}

fn refuse_session(reason: &str) -> (StatusCode, Json<HostSessionResponse>) {
    tracing::warn!(reason = %reason, "refused to mint a host session token");
    (
        StatusCode::FORBIDDEN,
        Json(HostSessionResponse {
            ok: false,
            token: String::new(),
            err: Some(format!("session token refused: {reason}")),
        }),
    )
}

// ─── Default ClientStateStore / NotificationSink for the daemon ─────────────
//
// These are the bare-minimum impls that let the daemon mount the webhook
// routes out-of-the-box. Real deployments inject SQLite-backed stores +
// per-account event-channel sinks via a custom Router::merge call in
// their fullstack server crate. See `plan-teams-graph-subscriptions.md`
// Phase C and the trait docs on `poly_host_bridge::teams_webhook` for the
// extension hook.

#[cfg(feature = "teams-webhook")]
#[derive(Default)]
struct InMemoryClientStateStore {
    map: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

#[cfg(feature = "teams-webhook")]
impl InMemoryClientStateStore {
    // lint-allow-unused: Phase C scaffolding — production deployments call this when they create a subscription (via TeamsHttpClient::create_subscription) to register the secret so the webhook handler can verify it. Kept here for API symmetry until the wiring lands.
    #[allow(dead_code)]
    fn insert(&self, sub_id: String, client_state: String) {
        if let Ok(mut map) = self.map.lock() {
            map.insert(sub_id, client_state);
        }
    }
}

#[cfg(feature = "teams-webhook")]
impl ClientStateStore for InMemoryClientStateStore {
    fn get(&self, sub_id: &str) -> Option<String> {
        self.map.lock().ok()?.get(sub_id).cloned()
    }
}

#[cfg(feature = "teams-webhook")]
struct TracingNotificationSink;

#[cfg(feature = "teams-webhook")]
impl NotificationSink for TracingNotificationSink {
    fn dispatch(&self, account_id: &str, n: ChangeNotification) {
        tracing::info!(
            account = account_id,
            subscription_id = %n.subscription_id,
            change_type = %n.change_type,
            resource = %n.resource,
            "teams webhook: notification (default tracing-only sink)"
        );
    }
}

/// ServiceWorker script — main-thread hang detector + auto-reload.
///
/// The main WASM app posts `{type:'poly-heartbeat'}` to this SW every 500ms.
/// If a client stops heartbeating for more than `HEARTBEAT_TIMEOUT_MS`, the
/// SW calls `client.navigate(client.url)` to force-reload that tab — which
/// works even when the main thread is stuck in an infinite WASM loop
/// (the navigation is executed at the browser level, not by main-thread JS).
const POLY_SERVICE_WORKER_JS: &str = r"// poly hang watchdog
const HEARTBEAT_TIMEOUT_MS = 25000;
const CHECK_INTERVAL_MS = 2000;
const lastBeat = new Map();

self.addEventListener('install', () => { self.skipWaiting(); });
self.addEventListener('activate', (e) => { e.waitUntil(self.clients.claim()); });
self.addEventListener('message', (e) => {
  if (e.data && e.data.type === 'poly-heartbeat' && e.source) {
    lastBeat.set(e.source.id, Date.now());
  }
});

setInterval(async () => {
  const now = Date.now();
  const clients = await self.clients.matchAll({ type: 'window', includeUncontrolled: true });
  for (const client of clients) {
    const beat = lastBeat.get(client.id);
    if (beat === undefined) continue;
    if (now - beat > HEARTBEAT_TIMEOUT_MS) {
      try {
        console.warn('[poly-sw] force-reloading client after ' + (now - beat) + 'ms silence');
        lastBeat.delete(client.id);
        await client.navigate(client.url);
      } catch (err) {
        console.error('[poly-sw] navigate failed', err);
      }
    }
  }
}, CHECK_INTERVAL_MS);
";

async fn poly_service_worker() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/javascript; charset=utf-8"),
    );
    // Scope="/" so the SW can control navigations initiated from any path.
    headers.insert(
        "service-worker-allowed",
        HeaderValue::from_static("/"),
    );
    // Don't cache the watchdog — we want edits to propagate on dev reload.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    (StatusCode::OK, headers, POLY_SERVICE_WORKER_JS)
}

/// Run the router on `addr` and block until the OS sends ctrl-c / SIGTERM.
///
/// Used by the `poly-host` binary. Shell processes (desktop-web) should
/// call [`router`] directly and wire the resulting `Router` into their
/// existing axum server instead.
pub async fn serve(addr: SocketAddr, state: HostState) -> Result<()> {
    let db_path_str = state.db_path().to_string_lossy().into_owned();
    // Phase A.4: the CORS allowlist tracks the port we actually bound, not
    // a hard-coded guess.
    let app = router(state.with_bound_port(addr.port()));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    tracing::info!("poly-host listening on http://{addr} (db: {db_path_str})");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum serve")?;
    Ok(())
}

/// Resolve Poly's canonical data dir. Same logic as
/// `crates/core/src/storage/mod.rs::poly_data_dir` so the daemon and the
/// native desktop app land on the same file.
///
/// `POLY_DATA_DIR` overrides everything for tests and isolated setups.
#[must_use]
pub fn resolve_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("POLY_DATA_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(target_os = "linux")]
    {
        let base: PathBuf = std::env::var("XDG_DATA_HOME").map_or_else(
            |_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".local").join("share")
            },
            PathBuf::from,
        );
        base.join("poly")
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("poly")
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(appdata).join("poly")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        PathBuf::from(".").join(".poly")
    }
}

// ─── Route handlers ──────────────────────────────────────────────────────────

async fn status(State(state): State<HostState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "service": "poly-host",
        "db": state.db_path.to_string_lossy(),
    }))
}

/// `GET /host/caps` — return the list of host capabilities advertised by this shell.
///
/// Response: `{ "caps": ["SandboxBrowser"] }` or `{ "caps": [] }`.
///
/// The capability strings are set at startup by the shell via
/// [`HostState::with_caps`]. Each string is a `HostCap` variant name.
///
/// All shells (Wry desktop, Electron, web fullstack) expose this endpoint
/// so the WASM UI can check at runtime whether `SandboxBrowser` is available
/// before rendering the sandbox-status row in plugin settings (Phase D.3).
async fn host_caps(State(state): State<HostState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "caps": *state.caps,
    }))
}

// ─── KV namespacing (Phase C.1 / C.2) ────────────────────────────────────────
//
// Every `poly_kv` key belongs to exactly one of three namespaces:
//
//   host:…       host-internal. No `/host/kv/*` or `/host/plugin-kv/*`
//                request may name one, whoever the caller is. The exec
//                declaration and consent records live here, which is what
//                makes B.4's "a key the KV surface cannot rewrite" true.
//   plugin:{id}: one plugin's private storage.
//   anything     the app's own rows (`app_settings`, `account_tokens`,
//   else         notification settings, …).
//
// A `CallerId::Plugin` is confined to its own `plugin:{id}:` prefix. The
// prefix comes from the *verified* identity, never from the request body,
// so forging the `plugin` field buys nothing.
//
// Deviation from the plan's literal C.2 wording, recorded here because the
// code disagreed with it: the plan asks that *no* `/host/kv/*` request be
// able to name `account_tokens` / `app_settings`. But the shell's own
// storage backend (`crates/core/src/storage/host_bridge.rs`) reads and
// writes exactly those two rows through `/host/kv/*` — they are the app's
// own data, and blocking the shell would delete the feature rather than
// secure it. The boundary that matters is caller-scoped: plugins and
// unauthenticated callers cannot name them, and the row is sealed at rest
// (C.3) so naming it without the key yields ciphertext.

/// Prefix reserved for rows only the host itself may touch.
const HOST_INTERNAL_PREFIX: &str = "host:";

/// Refuse `key` unless `caller` owns the namespace it lives in.
fn check_kv_key(caller: &CallerId, key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("kv key must not be empty".to_string());
    }
    if key.starts_with(HOST_INTERNAL_PREFIX) {
        return Err(format!(
            "kv key `{key}` is host-internal and cannot be named by any /host/kv/* caller"
        ));
    }
    match caller.kv_prefix() {
        None => Ok(()),
        Some(prefix) if key.starts_with(&prefix) => Ok(()),
        Some(prefix) => Err(format!(
            "kv key `{key}` is outside {}'s namespace `{prefix}`",
            caller.label()
        )),
    }
}

/// Resolve which plugin namespace a plugin-KV request actually addresses.
///
/// A verified plugin gets its own id, full stop; naming a different one is
/// an error rather than a silent redirect so the denial is visible in
/// logs. The shell may address any plugin's namespace: WASM guests run
/// in-process inside the shell, which proxies their storage calls, so
/// "shell acting for a plugin" is the normal path.
fn effective_plugin(caller: &CallerId, requested: &str) -> Result<String, String> {
    match *caller {
        CallerId::Shell => {
            if requested.trim().is_empty() {
                Err("plugin id is required".to_string())
            } else {
                Ok(requested.to_string())
            }
        }
        CallerId::Plugin { ref id } => {
            if requested.trim().is_empty() || requested == id {
                // `id` is verified (see `HostAuth::verify`), which also
                // rejects `:` — so it cannot forge a nested namespace.
                Ok(id.clone())
            } else {
                Err(format!(
                    "plugin `{id}` may not address plugin `{requested}`'s storage namespace"
                ))
            }
        }
    }
}

async fn kv_get(
    State(state): State<HostState>,
    Extension(caller): Extension<CallerId>,
    Json(req): Json<KvGetRequest>,
) -> Json<KvGetResponse> {
    if let Err(e) = check_kv_key(&caller, &req.key) {
        return Json(KvGetResponse {
            ok: false,
            value: None,
            err: Some(e),
        });
    }
    Json(match sqlite_get(&state, &req.key) {
        Ok(value) => KvGetResponse {
            ok: true,
            value,
            err: None,
        },
        Err(e) => KvGetResponse {
            ok: false,
            value: None,
            err: Some(e),
        },
    })
}

async fn kv_set(
    State(state): State<HostState>,
    Extension(caller): Extension<CallerId>,
    Json(req): Json<KvSetRequest>,
) -> Json<KvVoidResponse> {
    Json(void_response(
        check_kv_key(&caller, &req.key).and_then(|()| sqlite_set(&state, &req.key, &req.value)),
    ))
}

async fn kv_delete(
    State(state): State<HostState>,
    Extension(caller): Extension<CallerId>,
    Json(req): Json<KvDeleteRequest>,
) -> Json<KvVoidResponse> {
    Json(void_response(
        check_kv_key(&caller, &req.key).and_then(|()| sqlite_delete(&state, &req.key)),
    ))
}

/// Wiping the whole table is shell-only: a plugin clearing every other
/// plugin's storage (and the account tokens) is exactly the cross-namespace
/// write C.1 exists to stop, and there is no per-namespace `clear` in the
/// wire protocol to scope it to.
async fn kv_clear(
    State(state): State<HostState>,
    Extension(caller): Extension<CallerId>,
) -> Json<KvVoidResponse> {
    if caller != CallerId::Shell {
        return Json(KvVoidResponse {
            ok: false,
            err: Some(format!(
                "{} may not clear the shared KV table",
                caller.label()
            )),
        });
    }
    Json(void_response(sqlite_clear(&state)))
}

// ─── Plugin-KV route handlers ─────────────────────────────────────────────────

async fn plugin_kv_get(
    State(state): State<HostState>,
    Extension(caller): Extension<CallerId>,
    Json(req): Json<PluginKvGetRequest>,
) -> Json<PluginKvGetResponse> {
    let plugin = match effective_plugin(&caller, &req.plugin) {
        Ok(p) => p,
        Err(e) => {
            return Json(PluginKvGetResponse {
                ok: false,
                value_b64: None,
                err: Some(e),
            });
        }
    };
    let k = plugin_kv_key(&plugin, req.account.as_deref(), &req.key);
    match sqlite_get(&state, &k) {
        Ok(Some(serde_json::Value::String(s))) => Json(PluginKvGetResponse {
            ok: true,
            value_b64: Some(s),
            err: None,
        }),
        Ok(Some(other)) => Json(PluginKvGetResponse {
            ok: false,
            value_b64: None,
            err: Some(format!(
                "plugin_kv value for {k} was not a string (got {other})"
            )),
        }),
        Ok(None) => Json(PluginKvGetResponse {
            ok: true,
            value_b64: None,
            err: None,
        }),
        Err(e) => Json(PluginKvGetResponse {
            ok: false,
            value_b64: None,
            err: Some(e),
        }),
    }
}

async fn plugin_kv_set(
    State(state): State<HostState>,
    Extension(caller): Extension<CallerId>,
    Json(req): Json<PluginKvSetRequest>,
) -> Json<KvVoidResponse> {
    use base64::Engine as _;
    if let Err(e) = base64::engine::general_purpose::STANDARD.decode(&req.value_b64) {
        return Json(KvVoidResponse {
            ok: false,
            err: Some(format!("invalid base64: {e}")),
        });
    }
    let plugin = match effective_plugin(&caller, &req.plugin) {
        Ok(p) => p,
        Err(e) => return Json(KvVoidResponse { ok: false, err: Some(e) }),
    };
    let k = plugin_kv_key(&plugin, req.account.as_deref(), &req.key);
    let value = serde_json::Value::String(req.value_b64);
    Json(void_response(sqlite_set(&state, &k, &value)))
}

async fn plugin_kv_delete(
    State(state): State<HostState>,
    Extension(caller): Extension<CallerId>,
    Json(req): Json<PluginKvDeleteRequest>,
) -> Json<KvVoidResponse> {
    let plugin = match effective_plugin(&caller, &req.plugin) {
        Ok(p) => p,
        Err(e) => return Json(KvVoidResponse { ok: false, err: Some(e) }),
    };
    let k = plugin_kv_key(&plugin, req.account.as_deref(), &req.key);
    Json(void_response(sqlite_delete(&state, &k)))
}

/// Build the namespaced `poly_kv` key for a plugin-KV entry.
///
/// Global (no account): `plugin:{plugin}:global:{key}`.
/// Per-account: `plugin:{plugin}:account:{account}:{key}`.
#[must_use]
pub fn plugin_kv_key(plugin: &str, account: Option<&str>, key: &str) -> String {
    account.map_or_else(
        || format!("plugin:{plugin}:global:{key}"),
        |acct| format!("plugin:{plugin}:account:{acct}:{key}"),
    )
}

/// `POST /host` — the legacy tagged-union endpoint.
///
/// Phase B.5: `dispatch` no longer executes anything, so the only call
/// this still serves is `HttpRequest`. An `ExecCommand` posted here comes
/// back as an error, not a subprocess — there is exactly one exec entry
/// point now and it is [`host_exec`].
async fn host_legacy(Json(call): Json<HostCall>) -> Json<HostResponse> {
    Json(dispatch(call).await)
}

/// `POST /host/exec` — the single gated subprocess entry point.
///
/// The identity comes from the verified [`CallerId`] the auth middleware
/// stamped on the request, never from the body, and the program must be
/// declared *and* consented for that identity.
async fn host_exec(
    State(state): State<HostState>,
    Extension(caller): Extension<CallerId>,
    Json(call): Json<HostCall>,
) -> (StatusCode, Json<HostResponse>) {
    let HostCall::ExecCommand { program, args } = call else {
        return (
            StatusCode::BAD_REQUEST,
            Json(HostResponse::Err(
                "/host/exec accepts only the exec-command shape".to_string(),
            )),
        );
    };
    match poly_host_bridge::dispatch_exec(state.exec_policy.as_ref(), &caller, &program, &args)
        .await
    {
        Ok(resp) => (StatusCode::OK, Json(resp)),
        Err(denied) => {
            if let ExecDenied::NoConsent { ref program, .. } = denied {
                state
                    .consent_prompt
                    .prompt(&caller, Path::new(program.as_str()));
            }
            tracing::warn!(caller = %caller.label(), error = %denied, "exec denied");
            (
                StatusCode::FORBIDDEN,
                Json(HostResponse::Err(denied.to_string())),
            )
        }
    }
}

/// `POST /host/exec/declare` — register the absolute program paths a
/// caller is *allowed to ask for*. Shell-only.
///
/// This is the host-side landing point for a plugin manifest's declared
/// program list: whoever parses the manifest (the plugin registry) pushes
/// the parsed absolute paths here. Declaring is not consenting — the user
/// still has to approve each pair through [`host_exec_consent`].
async fn host_exec_declare(
    State(state): State<HostState>,
    Extension(caller): Extension<CallerId>,
    Json(req): Json<ExecDeclareRequest>,
) -> (StatusCode, Json<KvVoidResponse>) {
    if caller != CallerId::Shell {
        return shell_only(&caller);
    }
    let subject = req.subject();
    let programs: Vec<PathBuf> = req.programs.iter().map(PathBuf::from).collect();
    match state.exec_policy.declare_for(&subject, &programs) {
        Ok(()) => (
            StatusCode::OK,
            Json(KvVoidResponse { ok: true, err: None }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(KvVoidResponse {
                ok: false,
                err: Some(e),
            }),
        ),
    }
}

/// `POST /host/exec/consent` — record the user's approval for one
/// `(caller, program)` pair. Shell-only: consent is a decision the user
/// makes in the UI, so only the UI's own identity may record it.
async fn host_exec_consent(
    State(state): State<HostState>,
    Extension(caller): Extension<CallerId>,
    Json(req): Json<ExecConsentRequest>,
) -> (StatusCode, Json<KvVoidResponse>) {
    if caller != CallerId::Shell {
        return shell_only(&caller);
    }
    let subject = req.subject();
    let program = match std::fs::canonicalize(&req.program) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(KvVoidResponse {
                    ok: false,
                    err: Some(format!("cannot resolve `{}`: {e}", req.program)),
                }),
            );
        }
    };
    match state.exec_policy.grant_consent(&subject, &program) {
        Ok(()) => (
            StatusCode::OK,
            Json(KvVoidResponse { ok: true, err: None }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(KvVoidResponse {
                ok: false,
                err: Some(e),
            }),
        ),
    }
}

fn shell_only(caller: &CallerId) -> (StatusCode, Json<KvVoidResponse>) {
    (
        StatusCode::FORBIDDEN,
        Json(KvVoidResponse {
            ok: false,
            err: Some(format!("{} may not call this route", caller.label())),
        }),
    )
}

/// Body of `POST /host/exec/declare`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExecDeclareRequest {
    /// Plugin id the declaration is for; omit for the shell itself.
    #[serde(default)]
    pub plugin: Option<String>,
    /// Absolute program paths.
    #[serde(default)]
    pub programs: Vec<String>,
}

impl ExecDeclareRequest {
    fn subject(&self) -> CallerId {
        subject_of(self.plugin.as_deref())
    }
}

/// Body of `POST /host/exec/consent`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExecConsentRequest {
    /// Plugin id the consent is for; omit for the shell itself.
    #[serde(default)]
    pub plugin: Option<String>,
    /// Absolute path of the approved program.
    pub program: String,
}

impl ExecConsentRequest {
    fn subject(&self) -> CallerId {
        subject_of(self.plugin.as_deref())
    }
}

fn subject_of(plugin: Option<&str>) -> CallerId {
    match plugin {
        Some(id) if !id.trim().is_empty() => CallerId::Plugin {
            id: id.trim().to_string(),
        },
        _shell => CallerId::Shell,
    }
}

async fn host_http(Json(call): Json<HostCall>) -> Result<Json<HostResponse>, StatusCode> {
    match &call {
        HostCall::HttpRequest { .. } => Ok(Json(dispatch(call).await)),
        HostCall::ExecCommand { .. } => Err(StatusCode::BAD_REQUEST),
    }
}

/// `POST /host/open-external` — open a URL in the system's default browser.
///
/// # Security
///
/// Only `http://` and `https://` schemes are permitted. Any other scheme
/// (e.g. `javascript:`, `file:`, `data:`, custom protocol handlers) is
/// rejected with HTTP 400 to prevent protocol-handler abuse from a
/// compromised WASM page.
///
/// # Shell support
///
/// This route is registered by the Wry shell (via `apps/desktop`'s fullstack
/// server) and the standalone `poly-host` daemon. Web and Electron shells do
/// **not** need it:
/// - Web (`apps/web`): `<a target="_blank">` opens new tabs natively.
/// - Electron (`apps/desktop-electron`): `setWindowOpenHandler` already
///   forwards every `window.open` call to `shell.openExternal` (wired at
///   `apps/desktop-electron-web/electron/main.js:115-118`).
async fn open_external(
    Json(req): Json<OpenExternalRequest>,
) -> (StatusCode, Json<OpenExternalResponse>) {
    // Security: hard-reject non-HTTP(S) schemes.
    let url = req.url.trim();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return (
            StatusCode::BAD_REQUEST,
            Json(OpenExternalResponse {
                ok: false,
                err: Some(format!(
                    "rejected: only http:// and https:// URLs are allowed, got: {url}"
                )),
            }),
        );
    }

    // Basic parse sanity check (webbrowser crate does its own, but we want
    // a 400 rather than a silent failure on malformed input).
    if url.contains('\0') || url.contains('\n') || url.contains('\r') {
        return (
            StatusCode::BAD_REQUEST,
            Json(OpenExternalResponse {
                ok: false,
                err: Some("rejected: URL contains control characters".into()),
            }),
        );
    }

    match webbrowser::open(url) {
        Ok(()) => (
            StatusCode::OK,
            Json(OpenExternalResponse { ok: true, err: None }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OpenExternalResponse {
                ok: false,
                err: Some(format!("failed to open browser: {e}")),
            }),
        ),
    }
}

// ─── Plugin / account admin route handlers ──────────────────────────────────
//
// These routes let an external AI agent (or any HTTP client) drive the
// plugin / account-token mutations the settings UI already provides.
// They are the MCP-equivalent surface for the "add a plugin from a URL,
// then login on it" automation flow.
//
// Implementation notes:
//
// - Plugin entries live as JSON inside the `app_settings` row of
//   `poly_kv` under the `wasm_plugins` array. Account tokens live in
//   the `account_tokens` row as a JSON array. Both are read whole,
//   mutated in memory, and written back. Concurrent writers are
//   serialised by the same `Arc<Mutex<>>` that guards the SQLite
//   connection in `HostState`.
//
// - We deliberately avoid a `poly-core` dependency: `apps/poly-host` is
//   a thin daemon and pulling in `poly-core` would drag dioxus + every
//   plugin crate into its build. The wire types in `poly-host-bridge`
//   keep the contract honest.

const APP_SETTINGS_KEY: &str = "app_settings";
const ACCOUNT_TOKENS_KEY: &str = "account_tokens";

/// List of compiled-in built-in backends that ship with the host
/// daemon. Mirror of the `BUILTIN_BACKENDS` array in
/// `crates/core/src/client_manager.rs` — kept here so the daemon can
/// list / validate backends without depending on `poly-core`. The
/// `available` flag is always `true` here (the daemon doesn't know
/// which features the UI was compiled with — that's the UI's job to
/// override at runtime if needed).
const BUILTIN_BACKEND_SLUGS: &[&str] = &[
    "demo",
    "stoat",
    "matrix",
    "lemmy",
    "github",
    "forgejo",
    "hackernews",
    "poly",
];

async fn plugins_add(
    State(state): State<HostState>,
    Json(req): Json<PluginAddRequest>,
) -> Json<PluginAddResponse> {
    let url = req.url.trim().to_string();
    if url.is_empty() {
        return Json(PluginAddResponse {
            ok: false,
            err: Some("url is required".into()),
            ..PluginAddResponse::default_resp()
        });
    }
    if !is_acceptable_plugin_url(&url) {
        return Json(PluginAddResponse {
            ok: false,
            err: Some(format!("invalid plugin URL: {url}")),
            ..PluginAddResponse::default_resp()
        });
    }

    Json(match mutate_app_settings(&state, |settings| {
        let bundled = poly_host_bridge::is_bundled_url(&url);
        // Tombstone clearance — re-adding a previously-removed bundled
        // plugin lifts the user's intent so subsequent restarts keep it.
        if let Some(slug) = poly_host_bridge::bundled_slug_from_url(&url)
            && let Some(arr) = settings
                .get_mut("removed_bundled_plugins")
                .and_then(|v| v.as_array_mut())
        {
            arr.retain(|s| s.as_str() != Some(slug));
        }

        let plugins = wasm_plugins_array_mut(settings)?;

        // Idempotent re-add: existing entry → flip `enabled` true, no insert.
        if let Some(existing) = plugins
            .iter_mut()
            .find(|e| e.get("url").and_then(|v| v.as_str()) == Some(url.as_str()))
        {
            if let Some(map) = existing.as_object_mut() {
                map.insert("enabled".into(), serde_json::Value::Bool(true));
            }
            return Ok(false);
        }

        let mut entry = serde_json::Map::new();
        entry.insert("url".into(), serde_json::Value::String(url.clone()));
        entry.insert(
            "name".into(),
            req.name
                .clone()
                .map_or(serde_json::Value::Null, serde_json::Value::String),
        );
        entry.insert("enabled".into(), serde_json::Value::Bool(true));
        entry.insert("bundled".into(), serde_json::Value::Bool(bundled));
        plugins.push(serde_json::Value::Object(entry));
        Ok(true)
    }) {
        Ok(added) => PluginAddResponse {
            ok: true,
            added,
            slug: poly_host_bridge::bundled_slug_from_url(&url)
                .map_or_else(String::new, str::to_string),
            url: url.clone(),
            err: None,
        },
        Err(e) => PluginAddResponse {
            ok: false,
            err: Some(e),
            ..PluginAddResponse::default_resp()
        },
    })
}

async fn plugins_remove(
    State(state): State<HostState>,
    Json(req): Json<PluginRemoveRequest>,
) -> Json<PluginRemoveResponse> {
    let raw = req.url_or_slug.trim().to_string();
    if raw.is_empty() {
        return Json(PluginRemoveResponse {
            ok: false,
            removed: false,
            err: Some("url_or_slug is required".into()),
        });
    }
    Json(match mutate_app_settings(&state, |settings| {
        let try_targets: Vec<String> = if raw.contains("://") {
            vec![raw.clone()]
        } else {
            vec![raw.clone(), format!("{}{raw}", poly_host_bridge::BUNDLED_URL_SCHEME)]
        };
        let mut removed_was_bundled: Option<String> = None;
        let plugins = wasm_plugins_array_mut(settings)?;
        let before = plugins.len();
        plugins.retain(|e| {
            let url_str = e.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let matched = try_targets.iter().any(|t| t == url_str);
            if matched {
                let bundled = e
                    .get("bundled")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if bundled
                    && let Some(slug) = poly_host_bridge::bundled_slug_from_url(url_str)
                {
                    removed_was_bundled = Some(slug.to_string());
                }
            }
            !matched
        });
        let removed = before != plugins.len();
        if removed
            && let Some(slug) = removed_was_bundled
        {
            let arr = settings
                .as_object_mut()
                .and_then(|m| {
                    m.entry("removed_bundled_plugins")
                        .or_insert_with(|| serde_json::Value::Array(Vec::new()))
                        .as_array_mut()
                })
                .ok_or_else(|| "settings is not an object".to_string())?;
            if !arr.iter().any(|v| v.as_str() == Some(&slug)) {
                arr.push(serde_json::Value::String(slug));
            }
        }
        Ok(removed)
    }) {
        Ok(removed) => PluginRemoveResponse {
            ok: true,
            removed,
            err: None,
        },
        Err(e) => PluginRemoveResponse {
            ok: false,
            removed: false,
            err: Some(e),
        },
    })
}

async fn plugins_set_enabled(
    State(state): State<HostState>,
    Json(req): Json<PluginSetEnabledRequest>,
) -> Json<PluginSetEnabledResponse> {
    let url = req.url.trim().to_string();
    if url.is_empty() {
        return Json(PluginSetEnabledResponse {
            ok: false,
            enabled: false,
            err: Some("url is required".into()),
        });
    }
    Json(match mutate_app_settings(&state, |settings| {
        let plugins = wasm_plugins_array_mut(settings)?;
        let entry = plugins
            .iter_mut()
            .find(|e| e.get("url").and_then(|v| v.as_str()) == Some(url.as_str()))
            .ok_or_else(|| format!("plugin not found: {url}"))?;
        if let Some(map) = entry.as_object_mut() {
            map.insert("enabled".into(), serde_json::Value::Bool(req.enabled));
        }
        Ok(req.enabled)
    }) {
        Ok(new_state) => PluginSetEnabledResponse {
            ok: true,
            enabled: new_state,
            err: None,
        },
        Err(e) => PluginSetEnabledResponse {
            ok: false,
            enabled: false,
            err: Some(e),
        },
    })
}

async fn plugins_list(State(state): State<HostState>) -> Json<PluginListResponse> {
    let settings = match read_app_settings(&state) {
        Ok(v) => v,
        Err(e) => {
            return Json(PluginListResponse {
                ok: false,
                plugins: Vec::new(),
                err: Some(e),
            });
        }
    };

    let disabled: Vec<String> = settings
        .get("disabled_native_backends")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut out: Vec<PluginListEntry> = BUILTIN_BACKEND_SLUGS
        .iter()
        .map(|slug| PluginListEntry {
            kind: "builtin".into(),
            slug: (*slug).into(),
            url: String::new(),
            name: None,
            enabled: !disabled.iter().any(|d| d == *slug),
            available: true,
            bundled: false,
        })
        .collect();

    if let Some(plugins) = settings
        .get("wasm_plugins")
        .and_then(|v| v.as_array())
    {
        for entry in plugins {
            let url = entry
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let bundled = entry
                .get("bundled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let slug = poly_host_bridge::bundled_slug_from_url(&url)
                .map_or_else(|| url.clone(), str::to_string);
            out.push(PluginListEntry {
                kind: "sideloaded".into(),
                slug,
                url,
                name: entry
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                enabled: entry
                    .get("enabled")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                available: true,
                bundled,
            });
        }
    }

    Json(PluginListResponse {
        ok: true,
        plugins: out,
        err: None,
    })
}

async fn accounts_add(
    State(state): State<HostState>,
    Json(req): Json<AccountAddRequest>,
) -> Json<AccountAddResponse> {
    if req.account_id.trim().is_empty() {
        return Json(AccountAddResponse {
            ok: false,
            account_id: req.account_id,
            backend: req.backend,
            err: Some("account_id is required".into()),
        });
    }
    if req.backend.trim().is_empty() {
        return Json(AccountAddResponse {
            ok: false,
            account_id: req.account_id,
            backend: req.backend,
            err: Some("backend is required".into()),
        });
    }

    // Validate backend availability.
    match read_app_settings(&state) {
        Ok(settings) => {
            let allowed = available_backend_slugs(&settings);
            if !allowed.iter().any(|s| s == &req.backend) {
                return Json(AccountAddResponse {
                    ok: false,
                    err: Some(format!(
                        "backend `{}` is not available (not compiled in or disabled)",
                        req.backend
                    )),
                    account_id: req.account_id,
                    backend: req.backend,
                });
            }
        }
        Err(e) => {
            return Json(AccountAddResponse {
                ok: false,
                account_id: req.account_id,
                backend: req.backend,
                err: Some(e),
            });
        }
    }

    // Extract before moving req into the json! macro and closure.
    let account_id = req.account_id;
    let backend = req.backend;
    let entry = serde_json::json!({
        "backend": backend,
        "account_id": account_id,
        "token": req.token,
        "display_name": req.display_name,
        "instance_id": req.instance_id,
        "refresh_token": req.refresh_token,
        "token_expires_at": req.token_expires_at,
        "scope": req.scope,
    });
    Json(
        match mutate_account_tokens(&state, |tokens| {
            // Upsert by (backend, account_id).
            tokens.retain(|t| {
                !(t.get("backend").and_then(|v| v.as_str()) == Some(&backend)
                    && t.get("account_id").and_then(|v| v.as_str())
                        == Some(&account_id))
            });
            tokens.push(entry.clone());
            Ok(())
        }) {
            Ok(()) => AccountAddResponse {
                ok: true,
                account_id,
                backend,
                err: None,
            },
            Err(e) => AccountAddResponse {
                ok: false,
                account_id,
                backend,
                err: Some(e),
            },
        },
    )
}

async fn accounts_remove(
    State(state): State<HostState>,
    Json(req): Json<AccountRemoveRequest>,
) -> Json<AccountRemoveResponse> {
    Json(
        match mutate_account_tokens(&state, |tokens| {
            let before = tokens.len();
            tokens.retain(|t| {
                !(t.get("backend").and_then(|v| v.as_str()) == Some(&req.backend)
                    && t.get("account_id").and_then(|v| v.as_str())
                        == Some(&req.account_id))
            });
            Ok(before != tokens.len())
        }) {
            Ok(removed) => AccountRemoveResponse {
                ok: true,
                removed,
                err: None,
            },
            Err(e) => AccountRemoveResponse {
                ok: false,
                removed: false,
                err: Some(e),
            },
        },
    )
}

async fn accounts_list(State(state): State<HostState>) -> Json<AccountListResponse> {
    let raw = match sqlite_get(&state, ACCOUNT_TOKENS_KEY) {
        Ok(Some(v)) => v,
        Ok(None) => serde_json::Value::Array(Vec::new()),
        Err(e) => {
            return Json(AccountListResponse {
                ok: false,
                accounts: Vec::new(),
                err: Some(e),
            });
        }
    };
    let arr = raw.as_array().cloned().unwrap_or_default();
    let accounts = arr
        .into_iter()
        .filter_map(|entry| {
            let backend = entry.get("backend")?.as_str()?.to_string();
            let account_id = entry.get("account_id")?.as_str()?.to_string();
            let display_name = entry
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let instance_id = entry
                .get("instance_id")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let token_expires_at = entry
                .get("token_expires_at")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            Some(AccountListEntry {
                backend,
                account_id,
                display_name,
                instance_id,
                token_expires_at,
            })
        })
        .collect();
    Json(AccountListResponse {
        ok: true,
        accounts,
        err: None,
    })
}

// ─── Helpers used by the plugin / account admin handlers ─────────────────────

fn is_acceptable_plugin_url(url: &str) -> bool {
    ["https://", "http://", "file://", "bundled://"]
        .iter()
        .any(|prefix| url.starts_with(prefix))
}

/// Compute the user-effective set of backend slugs (builtin minus
/// disabled, plus enabled bundled entries). Mirrors
/// `poly_core::plugin_admin::available_backend_slugs`.
fn available_backend_slugs(settings: &serde_json::Value) -> Vec<String> {
    let disabled: Vec<String> = settings
        .get("disabled_native_backends")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let mut out: Vec<String> = BUILTIN_BACKEND_SLUGS
        .iter()
        .filter(|slug| !disabled.iter().any(|d| d == *slug))
        .map(|s| (*s).to_string())
        .collect();
    if let Some(plugins) = settings
        .get("wasm_plugins")
        .and_then(|v| v.as_array())
    {
        for entry in plugins {
            let bundled = entry
                .get("bundled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let enabled = entry
                .get("enabled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            if !bundled || !enabled {
                continue;
            }
            let url = entry.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(slug) = poly_host_bridge::bundled_slug_from_url(url)
                && !out.iter().any(|s| s == slug)
                && !disabled.iter().any(|d| d == slug)
            {
                out.push(slug.to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn read_app_settings(state: &HostState) -> Result<serde_json::Value, String> {
    Ok(sqlite_get(state, APP_SETTINGS_KEY)?
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new())))
}

/// Lock + read-modify-write the `app_settings` JSON value. The closure
/// receives a mutable `serde_json::Value` (object) and returns whatever
/// result-typed payload the caller wants to surface.
fn mutate_app_settings<F, T>(state: &HostState, f: F) -> Result<T, String>
where
    F: FnOnce(&mut serde_json::Value) -> Result<T, String>,
{
    let mut settings = read_app_settings(state)?;
    if !settings.is_object() {
        // Replace anything that isn't a JSON object with a fresh map so
        // downstream get_mut(...) calls don't panic.
        settings = serde_json::Value::Object(serde_json::Map::new());
    }
    let result = f(&mut settings)?;
    sqlite_set(state, APP_SETTINGS_KEY, &settings)?;
    Ok(result)
}

/// Get `wasm_plugins` from a settings JSON object as a `&mut Vec<Value>`,
/// inserting an empty array if it's missing or the wrong type.
///
/// Returns `Err` only if `settings` isn't a JSON object — `mutate_app_settings`
/// normalises that, so callers can usually `?` and not worry about the
/// failure path.
fn wasm_plugins_array_mut(
    settings: &mut serde_json::Value,
) -> Result<&mut Vec<serde_json::Value>, String> {
    let map = settings
        .as_object_mut()
        .ok_or_else(|| "app_settings root is not a JSON object".to_string())?;
    let entry = map
        .entry("wasm_plugins")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    if !entry.is_array() {
        *entry = serde_json::Value::Array(Vec::new());
    }
    entry
        .as_array_mut()
        .ok_or_else(|| "wasm_plugins entry was just set to array but is not".to_string())
}

/// Lock + read-modify-write the `account_tokens` JSON array.
fn mutate_account_tokens<F, T>(state: &HostState, f: F) -> Result<T, String>
where
    F: FnOnce(&mut Vec<serde_json::Value>) -> Result<T, String>,
{
    let mut tokens: Vec<serde_json::Value> = sqlite_get(state, ACCOUNT_TOKENS_KEY)?
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let result = f(&mut tokens)?;
    sqlite_set(
        state,
        ACCOUNT_TOKENS_KEY,
        &serde_json::Value::Array(tokens),
    )?;
    Ok(result)
}

/// Helper for `..Default::default_resp()` ergonomics inside the plugin-add response.
trait PluginAddDefault {
    fn default_resp() -> Self;
}

impl PluginAddDefault for PluginAddResponse {
    fn default_resp() -> Self {
        Self {
            ok: false,
            added: false,
            slug: String::new(),
            url: String::new(),
            err: None,
        }
    }
}

// ─── SQLite-backed ExecPolicy (Phase B.4) ────────────────────────────────────

/// Host-internal row holding `{ "<caller label>": ["/abs/path", …] }`.
const EXEC_DECLARED_KEY: &str = "host:exec:declared";
/// Host-internal row holding `{ "<caller label>": ["/abs/path", …] }` of
/// approved pairs.
const EXEC_CONSENT_KEY: &str = "host:exec:consent";

/// Persists exec declarations and consent in `poly_kv` under the
/// `host:` prefix, which [`check_kv_key`] makes unnameable from
/// `/host/kv/*`. That is what Phase B.4 means by "a key the KV surface
/// cannot rewrite": a caller who could rewrite these rows could grant
/// itself consent.
///
/// Holds its own handle on the connection rather than a `HostState` so
/// the policy is not circularly owned by the state that owns it.
struct SqliteExecPolicy {
    db: Arc<Mutex<ConnectionThreadSafe>>,
}

impl SqliteExecPolicy {
    const fn new(db: Arc<Mutex<ConnectionThreadSafe>>) -> Self {
        Self { db }
    }

    fn read_map(&self, key: &str) -> serde_json::Map<String, serde_json::Value> {
        raw_get(&self.db, key)
            .ok()
            .flatten()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    }

    fn paths_for(&self, key: &str, caller: &CallerId) -> Vec<PathBuf> {
        self.read_map(key)
            .get(&caller.label())
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(PathBuf::from))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn write_paths(&self, key: &str, caller: &CallerId, paths: &[PathBuf]) -> Result<(), String> {
        let mut map = self.read_map(key);
        let list: Vec<serde_json::Value> = paths
            .iter()
            .map(|p| serde_json::Value::String(p.display().to_string()))
            .collect();
        let _prev = map.insert(caller.label(), serde_json::Value::Array(list));
        raw_set(&self.db, key, &serde_json::Value::Object(map))
    }
}

impl ExecPolicy for SqliteExecPolicy {
    fn declared_programs(&self, caller: &CallerId) -> Vec<PathBuf> {
        self.paths_for(EXEC_DECLARED_KEY, caller)
    }

    fn declare_for(&self, caller: &CallerId, programs: &[PathBuf]) -> Result<(), String> {
        self.write_paths(EXEC_DECLARED_KEY, caller, programs)
    }

    fn has_consent(&self, caller: &CallerId, program: &Path) -> bool {
        self.paths_for(EXEC_CONSENT_KEY, caller)
            .iter()
            .any(|p| p == program)
    }

    fn grant_consent(&self, caller: &CallerId, program: &Path) -> Result<(), String> {
        let mut approved = self.paths_for(EXEC_CONSENT_KEY, caller);
        if !approved.iter().any(|p| p == program) {
            approved.push(program.to_path_buf());
        }
        self.write_paths(EXEC_CONSENT_KEY, caller, &approved)
    }
}

// ─── SQLite helpers ──────────────────────────────────────────────────────────

/// Rows whose payload is encrypted at rest (Phase C.3).
///
/// `account_tokens` is the OAuth access/refresh-token array. `app_settings`
/// is deliberately *not* sealed: it is read on the boot path by every
/// shell, holds no key material, and sealing it would put the whole UI
/// behind a key-file read for no confidentiality gain.
fn is_sealed_key(key: &str) -> bool {
    key == ACCOUNT_TOKENS_KEY
}

fn lock_db(
    db: &Arc<Mutex<ConnectionThreadSafe>>,
) -> Result<std::sync::MutexGuard<'_, ConnectionThreadSafe>, String> {
    db.lock()
        .map_err(|_poison| "sqlite mutex poisoned".to_string())
}

/// Read a row verbatim — no unsealing. Used by the host-internal stores
/// (which are never sealed) and by [`sqlite_get`].
#[allow(clippy::significant_drop_tightening)] // stmt borrows db; cannot release db before stmt
fn raw_get(
    db: &Arc<Mutex<ConnectionThreadSafe>>,
    key: &str,
) -> Result<Option<serde_json::Value>, String> {
    let db = lock_db(db)?;
    let mut stmt = db
        .prepare("SELECT payload FROM poly_kv WHERE key = ?1 LIMIT 1")
        .map_err(|e| format!("prepare get({key}): {e}"))?;
    stmt.bind((1, key))
        .map_err(|e| format!("bind get({key}): {e}"))?;
    match stmt
        .next()
        .map_err(|e| format!("step get({key}): {e}"))?
    {
        SqlState::Done => Ok(None),
        SqlState::Row => {
            let payload = stmt
                .read::<String, _>(0)
                .map_err(|e| format!("read get({key}): {e}"))?;
            let value = serde_json::from_str(&payload)
                .map_err(|e| format!("serde get({key}): {e}"))?;
            Ok(Some(value))
        }
    }
}

/// Write a row verbatim — no sealing. See [`raw_get`].
#[allow(clippy::significant_drop_tightening)] // stmt borrows db; cannot release db before stmt
fn raw_set(
    db: &Arc<Mutex<ConnectionThreadSafe>>,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    let serialized =
        serde_json::to_string(value).map_err(|e| format!("serde set({key}): {e}"))?;
    let db = lock_db(db)?;
    let mut stmt = db
        .prepare(
            "INSERT INTO poly_kv(key, payload) VALUES(?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET payload = excluded.payload",
        )
        .map_err(|e| format!("prepare set({key}): {e}"))?;
    stmt.bind((1, key))
        .map_err(|e| format!("bind key set({key}): {e}"))?;
    stmt.bind((2, serialized.as_str()))
        .map_err(|e| format!("bind payload set({key}): {e}"))?;
    while stmt
        .next()
        .map_err(|e| format!("step set({key}): {e}"))?
        != SqlState::Done
    {}
    Ok(())
}

/// Read a row, unsealing it if the key is one of the sealed ones.
fn sqlite_get(state: &HostState, key: &str) -> Result<Option<serde_json::Value>, String> {
    let raw = raw_get(&state.db, key)?;
    let Some(value) = raw else { return Ok(None) };
    if !is_sealed_key(key) {
        return Ok(Some(value));
    }
    // A sealed row is stored as a JSON string holding the envelope; a row
    // written before C.3 landed is still the bare value and passes through.
    let Some(envelope) = value.as_str().filter(|s| s.starts_with(SEAL_PREFIX)) else {
        return Ok(Some(value));
    };
    let plaintext = state
        .sealer
        .unseal(envelope)
        .map_err(|e| format!("unseal {key}: {e}"))?;
    serde_json::from_str(&plaintext).map_err(|e| format!("serde unsealed {key}: {e}"))
}

/// Write a row, sealing it if the key is one of the sealed ones.
fn sqlite_set(state: &HostState, key: &str, value: &serde_json::Value) -> Result<(), String> {
    if !is_sealed_key(key) {
        return raw_set(&state.db, key, value);
    }
    let plaintext =
        serde_json::to_string(value).map_err(|e| format!("serde set({key}): {e}"))?;
    let envelope = state
        .sealer
        .seal(&plaintext)
        .map_err(|e| format!("seal {key}: {e}"))?;
    raw_set(&state.db, key, &serde_json::Value::String(envelope))
}

#[allow(clippy::significant_drop_tightening)] // stmt borrows db; cannot release db before stmt
fn sqlite_delete(state: &HostState, key: &str) -> Result<(), String> {
    let db = lock_db(&state.db)?;
    let mut stmt = db
        .prepare("DELETE FROM poly_kv WHERE key = ?1")
        .map_err(|e| format!("prepare delete({key}): {e}"))?;
    stmt.bind((1, key))
        .map_err(|e| format!("bind delete({key}): {e}"))?;
    while stmt
        .next()
        .map_err(|e| format!("step delete({key}): {e}"))?
        != SqlState::Done
    {}
    Ok(())
}

fn sqlite_clear(state: &HostState) -> Result<(), String> {
    lock_db(&state.db)?
        .execute("DELETE FROM poly_kv")
        .map_err(|e| format!("clear: {e}"))?;
    Ok(())
}

fn void_response(result: Result<(), String>) -> KvVoidResponse {
    match result {
        Ok(()) => KvVoidResponse {
            ok: true,
            err: None,
        },
        Err(e) => KvVoidResponse {
            ok: false,
            err: Some(e),
        },
    }
}

// ─── Sandbox redirect shim (C.1) ─────────────────────────────────────────────
//
// GET /sandbox/{id}?<any-captured-fragment>
//
// This is the OAuth/captcha redirect target that the OAuth provider sends the
// browser back to after the user completes the challenge. It serves a tiny
// HTML page that:
//   1. Posts the captured URL (window.location.href) back to the opener via
//      `postMessage`, tagged with the sandbox `id` so the WASM listener can
//      match it.
//   2. Closes the popup window.
//
// Constraint: the OAuth provider MUST be configured to redirect to
// `<host-origin>/sandbox/<id>` (same-origin requirement so postMessage can
// use `location.origin` as the target, preventing cross-origin message leaks).
// See docs/plans/plan-host-sandbox-impl.md Phase C, task C.4.
//
// The handler is state-less — it only uses the `id` path segment to echo back
// so the WASM listener can match the right pending sandbox future.
async fn sandbox_shim(AxumPath(id): AxumPath<String>) -> impl IntoResponse {
    // Basic validation: id must be non-empty and contain only URL-safe chars.
    if id.is_empty() || !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return (StatusCode::BAD_REQUEST, HeaderMap::new(), "invalid sandbox id".to_string());
    }

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>Poly sandbox redirect</title></head>
<body>
<script>
(function() {{
  var id = {id_json};
  var url = window.location.href;
  if (window.opener) {{
    window.opener.postMessage({{ type: 'sandbox-captured', id: id, url: url }}, window.location.origin);
  }}
  window.close();
}})();
</script>
<p>Redirecting&hellip;</p>
</body>
</html>"#,
        id_json = serde_json::to_string(&id).unwrap_or_else(|_| "\"\"".to_string()),
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    // No caching — each sandbox id is ephemeral.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    (StatusCode::OK, headers, html)
}

#[allow(clippy::cognitive_complexity)] // signal dispatch: cfg branches inflate score artificially
async fn shutdown_signal() {
    let ctrl_c = async {
        drop(tokio::signal::ctrl_c().await);
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => tracing::info!("received ctrl-c, shutting down"),
        () = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use base64::Engine as _;
    use tower::util::ServiceExt as _;

    /// Fixed shell token so tests can present the right (and the wrong)
    /// credentials deterministically.
    const TEST_TOKEN: &str = "test-shell-session-token";

    fn test_state() -> HostState {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.keep().join("test.sqlite3");
        HostState::open(path)
            .expect("open")
            .with_auth(HostAuth::with_token(TEST_TOKEN))
    }

    /// Token a given plugin would be handed by the shell.
    fn plugin_token(id: &str) -> String {
        HostAuth::with_token(TEST_TOKEN).derive_plugin_token(id)
    }

    fn b64(s: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(s)
    }

    /// POST as the shell (the common case).
    async fn post_json(
        app: &Router,
        path: &str,
        body: serde_json::Value,
    ) -> (StatusCode, String) {
        post_as(app, path, body, Some(TEST_TOKEN), None).await
    }

    /// POST with explicit credentials — `None` token means "send no
    /// `Authorization` header at all".
    async fn post_as(
        app: &Router,
        path: &str,
        body: serde_json::Value,
        token: Option<&str>,
        plugin: Option<&str>,
    ) -> (StatusCode, String) {
        let mut builder = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json");
        if let Some(tok) = token {
            builder = builder.header("authorization", format!("Bearer {tok}"));
        }
        if let Some(id) = plugin {
            builder = builder.header(PLUGIN_HEADER, id);
        }
        let req = builder
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        send(app, req).await
    }

    async fn send(app: &Router, req: Request<Body>) -> (StatusCode, String) {
        let resp = app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[test]
    fn plugin_kv_key_global() {
        assert_eq!(
            plugin_kv_key("matrix", None, "token"),
            "plugin:matrix:global:token"
        );
    }

    #[test]
    fn plugin_kv_key_with_account() {
        assert_eq!(
            plugin_kv_key("matrix", Some("@alice:example.com"), "token"),
            "plugin:matrix:account:@alice:example.com:token"
        );
    }

    #[tokio::test]
    async fn plugin_kv_set_get_round_trip_no_account() {
        let app = router(test_state());
        let value = b64(b"hello world");

        let (status, body) = post_json(
            &app,
            "/host/plugin-kv/set",
            serde_json::json!({
                "plugin": "matrix",
                "key": "token",
                "value_b64": value
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);

        let (status, body) = post_json(
            &app,
            "/host/plugin-kv/get",
            serde_json::json!({
                "plugin": "matrix",
                "key": "token"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["value_b64"], value);
    }

    #[tokio::test]
    async fn plugin_kv_set_get_round_trip_with_account() {
        let app = router(test_state());
        let value = b64(b"secret-token");

        let (status, body) = post_json(
            &app,
            "/host/plugin-kv/set",
            serde_json::json!({
                "plugin": "matrix",
                "account": "@alice:example.com",
                "key": "token",
                "value_b64": value
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);

        let (status, body) = post_json(
            &app,
            "/host/plugin-kv/get",
            serde_json::json!({
                "plugin": "matrix",
                "account": "@alice:example.com",
                "key": "token"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["value_b64"], value);
    }

    #[tokio::test]
    async fn plugin_kv_delete_makes_get_return_none() {
        let app = router(test_state());
        let value = b64(b"to-be-deleted");

        post_json(
            &app,
            "/host/plugin-kv/set",
            serde_json::json!({
                "plugin": "stoat",
                "key": "session",
                "value_b64": value
            }),
        )
        .await;

        let (status, body) = post_json(
            &app,
            "/host/plugin-kv/delete",
            serde_json::json!({
                "plugin": "stoat",
                "key": "session"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);

        let (status, body) = post_json(
            &app,
            "/host/plugin-kv/get",
            serde_json::json!({
                "plugin": "stoat",
                "key": "session"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);
        assert!(resp["value_b64"].is_null());
    }

    #[tokio::test]
    async fn plugin_kv_cross_plugin_isolation() {
        let app = router(test_state());
        let v1 = b64(b"plugin-a-value");
        let v2 = b64(b"plugin-b-value");

        post_json(
            &app,
            "/host/plugin-kv/set",
            serde_json::json!({ "plugin": "plugin-a", "key": "shared-key", "value_b64": v1 }),
        )
        .await;
        post_json(
            &app,
            "/host/plugin-kv/set",
            serde_json::json!({ "plugin": "plugin-b", "key": "shared-key", "value_b64": v2 }),
        )
        .await;

        let (_s, body) = post_json(
            &app,
            "/host/plugin-kv/get",
            serde_json::json!({ "plugin": "plugin-a", "key": "shared-key" }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["value_b64"], v1);

        let (_s, body) = post_json(
            &app,
            "/host/plugin-kv/get",
            serde_json::json!({ "plugin": "plugin-b", "key": "shared-key" }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["value_b64"], v2);
    }

    #[tokio::test]
    async fn plugin_kv_cross_account_isolation() {
        let app = router(test_state());
        let v1 = b64(b"alice-token");
        let v2 = b64(b"bob-token");

        post_json(
            &app,
            "/host/plugin-kv/set",
            serde_json::json!({ "plugin": "matrix", "account": "alice", "key": "tok", "value_b64": v1 }),
        )
        .await;
        post_json(
            &app,
            "/host/plugin-kv/set",
            serde_json::json!({ "plugin": "matrix", "account": "bob", "key": "tok", "value_b64": v2 }),
        )
        .await;

        let (_s, body) = post_json(
            &app,
            "/host/plugin-kv/get",
            serde_json::json!({ "plugin": "matrix", "account": "alice", "key": "tok" }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["value_b64"], v1);

        let (_s, body) = post_json(
            &app,
            "/host/plugin-kv/get",
            serde_json::json!({ "plugin": "matrix", "account": "bob", "key": "tok" }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["value_b64"], v2);
    }

    #[tokio::test]
    async fn plugin_kv_set_rejects_invalid_base64() {
        let app = router(test_state());
        let (status, body) = post_json(
            &app,
            "/host/plugin-kv/set",
            serde_json::json!({
                "plugin": "matrix",
                "key": "tok",
                "value_b64": "!!not-base64!!"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], false);
        assert!(resp["err"].as_str().unwrap().contains("base64"));
    }

    #[tokio::test]
    async fn plugin_kv_get_nonexistent_returns_ok_with_null() {
        let app = router(test_state());
        let (_s, body) = post_json(
            &app,
            "/host/plugin-kv/get",
            serde_json::json!({ "plugin": "unknown", "key": "nope" }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);
        assert!(resp["value_b64"].is_null());
    }

    // ─── Plugin / account admin route tests ──────────────────────────────

    async fn get(app: &Router, path: &str) -> (StatusCode, String) {
        let req = Request::builder()
            .method("GET")
            .uri(path)
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::empty())
            .unwrap();
        send(app, req).await
    }

    fn read_settings_json(state: &HostState) -> serde_json::Value {
        sqlite_get(state, APP_SETTINGS_KEY)
            .unwrap()
            .unwrap_or(serde_json::Value::Null)
    }

    #[tokio::test]
    async fn plugins_add_inserts_new_entry() {
        let state = test_state();
        let app = router(state.clone());
        let (status, body) = post_json(
            &app,
            "/host/plugins/add",
            serde_json::json!({ "url": "https://example.com/p.wasm", "name": "Test" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["added"], true);
        assert_eq!(resp["url"], "https://example.com/p.wasm");

        let settings = read_settings_json(&state);
        let plugins = settings["wasm_plugins"].as_array().unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0]["url"], "https://example.com/p.wasm");
        assert_eq!(plugins[0]["enabled"], true);
        assert_eq!(plugins[0]["bundled"], false);
    }

    #[tokio::test]
    async fn plugins_add_idempotent_re_add_returns_already_present() {
        let app = router(test_state());
        let url = "https://example.com/p.wasm";
        post_json(&app, "/host/plugins/add", serde_json::json!({ "url": url })).await;
        let (_s, body) =
            post_json(&app, "/host/plugins/add", serde_json::json!({ "url": url })).await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["added"], false);
    }

    #[tokio::test]
    async fn plugins_add_bundled_url_marks_bundled_true() {
        let state = test_state();
        let app = router(state.clone());
        post_json(
            &app,
            "/host/plugins/add",
            serde_json::json!({ "url": "bundled://discord" }),
        )
        .await;
        let plugins = read_settings_json(&state)["wasm_plugins"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0]["bundled"], true);
    }

    #[tokio::test]
    async fn plugins_add_rejects_empty_url() {
        let app = router(test_state());
        let (_s, body) =
            post_json(&app, "/host/plugins/add", serde_json::json!({ "url": "" })).await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], false);
        assert!(resp["err"].as_str().unwrap().contains("required"));
    }

    #[tokio::test]
    async fn plugins_add_rejects_invalid_scheme() {
        let app = router(test_state());
        let (_s, body) = post_json(
            &app,
            "/host/plugins/add",
            serde_json::json!({ "url": "ftp://x" }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], false);
        assert!(resp["err"].as_str().unwrap().contains("invalid"));
    }

    #[tokio::test]
    async fn plugins_remove_drops_entry() {
        let state = test_state();
        let app = router(state.clone());
        let url = "https://example.com/p.wasm";
        post_json(&app, "/host/plugins/add", serde_json::json!({ "url": url })).await;
        let (_s, body) = post_json(
            &app,
            "/host/plugins/remove",
            serde_json::json!({ "url_or_slug": url }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["removed"], true);
        let plugins = read_settings_json(&state)["wasm_plugins"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(plugins.is_empty());
    }

    #[tokio::test]
    async fn plugins_remove_by_bare_slug_for_bundled_plugin() {
        let state = test_state();
        let app = router(state.clone());
        post_json(
            &app,
            "/host/plugins/add",
            serde_json::json!({ "url": "bundled://discord" }),
        )
        .await;
        let (_s, body) = post_json(
            &app,
            "/host/plugins/remove",
            serde_json::json!({ "url_or_slug": "discord" }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["removed"], true);
        let settings = read_settings_json(&state);
        assert!(
            settings["wasm_plugins"]
                .as_array()
                .is_none_or(std::vec::Vec::is_empty)
        );
        let removed = settings["removed_bundled_plugins"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>();
        assert_eq!(removed, vec!["discord"]);
    }

    #[tokio::test]
    async fn plugins_remove_unknown_returns_removed_false() {
        let app = router(test_state());
        let (_s, body) = post_json(
            &app,
            "/host/plugins/remove",
            serde_json::json!({ "url_or_slug": "https://nowhere.test/none.wasm" }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["removed"], false);
    }

    #[tokio::test]
    async fn plugins_set_enabled_toggles_value() {
        let state = test_state();
        let app = router(state.clone());
        let url = "https://example.com/p.wasm";
        post_json(&app, "/host/plugins/add", serde_json::json!({ "url": url })).await;
        let (_s, body) = post_json(
            &app,
            "/host/plugins/set-enabled",
            serde_json::json!({ "url": url, "enabled": false }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["enabled"], false);

        let plugins = read_settings_json(&state)["wasm_plugins"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(plugins[0]["enabled"], false);
    }

    #[tokio::test]
    async fn plugins_set_enabled_unknown_url_returns_error() {
        let app = router(test_state());
        let (_s, body) = post_json(
            &app,
            "/host/plugins/set-enabled",
            serde_json::json!({ "url": "https://x.test/none.wasm", "enabled": true }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], false);
        assert!(resp["err"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn plugins_list_includes_builtin_and_sideloaded() {
        let state = test_state();
        let app = router(state.clone());
        post_json(
            &app,
            "/host/plugins/add",
            serde_json::json!({ "url": "bundled://discord" }),
        )
        .await;
        post_json(
            &app,
            "/host/plugins/add",
            serde_json::json!({ "url": "https://example.com/p.wasm" }),
        )
        .await;

        let (status, body) = get(&app, "/host/plugins/list").await;
        assert_eq!(status, StatusCode::OK);
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);
        let plugins = resp["plugins"].as_array().unwrap();
        let builtin: Vec<&str> = plugins
            .iter()
            .filter(|p| p["kind"] == "builtin")
            .filter_map(|p| p["slug"].as_str())
            .collect();
        // The list should contain at least our known canonical builtins.
        assert!(builtin.contains(&"demo"));
        assert!(builtin.contains(&"poly"));
        let sideloaded: Vec<&str> = plugins
            .iter()
            .filter(|p| p["kind"] == "sideloaded")
            .filter_map(|p| p["url"].as_str())
            .collect();
        assert!(sideloaded.contains(&"bundled://discord"));
        assert!(sideloaded.contains(&"https://example.com/p.wasm"));
    }

    #[tokio::test]
    async fn plugins_list_marks_disabled_native_backends() {
        let state = test_state();
        // Pre-populate disabled_native_backends so `plugins_list` reflects it.
        sqlite_set(
            &state,
            APP_SETTINGS_KEY,
            &serde_json::json!({
                "disabled_native_backends": ["stoat"]
            }),
        )
        .unwrap();
        let app = router(state);
        let (_s, body) = get(&app, "/host/plugins/list").await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        let stoat = resp["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["slug"] == "stoat" && p["kind"] == "builtin")
            .unwrap()
            .clone();
        assert_eq!(stoat["enabled"], false);
    }

    #[tokio::test]
    async fn accounts_add_persists_token() {
        let state = test_state();
        let app = router(state.clone());
        // Demo backend is in BUILTIN_BACKEND_SLUGS so this must pass.
        let (status, body) = post_json(
            &app,
            "/host/accounts/add",
            serde_json::json!({
                "backend": "demo",
                "account_id": "alice",
                "token": "tok-123",
                "display_name": "Alice"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);
        let stored = sqlite_get(&state, ACCOUNT_TOKENS_KEY).unwrap().unwrap();
        let arr = stored.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["account_id"], "alice");
        assert_eq!(arr[0]["token"], "tok-123");
    }

    #[tokio::test]
    async fn accounts_add_rejects_unknown_backend() {
        let app = router(test_state());
        let (_s, body) = post_json(
            &app,
            "/host/accounts/add",
            serde_json::json!({
                "backend": "no-such-backend",
                "account_id": "alice",
                "token": "tok",
                "display_name": "A"
            }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], false);
        assert!(resp["err"].as_str().unwrap().contains("not available"));
    }

    #[tokio::test]
    async fn accounts_add_rejects_disabled_backend() {
        let state = test_state();
        // Disable demo via the settings JSON before attempting signup.
        sqlite_set(
            &state,
            APP_SETTINGS_KEY,
            &serde_json::json!({
                "disabled_native_backends": ["demo"]
            }),
        )
        .unwrap();
        let app = router(state);
        let (_s, body) = post_json(
            &app,
            "/host/accounts/add",
            serde_json::json!({
                "backend": "demo",
                "account_id": "alice",
                "token": "tok",
                "display_name": "A"
            }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], false);
        assert!(resp["err"].as_str().unwrap().contains("not available"));
    }

    #[tokio::test]
    async fn accounts_add_then_login_with_bundled_plugin() {
        let state = test_state();
        let app = router(state.clone());

        // 1. Sideload Discord (bundled).
        let (_s, body) = post_json(
            &app,
            "/host/plugins/add",
            serde_json::json!({ "url": "bundled://discord" }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["slug"], "discord");

        // 2. Login on it (bundled is enabled by default after add).
        let (_s, body) = post_json(
            &app,
            "/host/accounts/add",
            serde_json::json!({
                "backend": "discord",
                "account_id": "user#1234",
                "token": "discord-token",
                "display_name": "My Discord"
            }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true, "discord login should succeed: {body}");

        let stored = sqlite_get(&state, ACCOUNT_TOKENS_KEY).unwrap().unwrap();
        assert_eq!(stored.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn accounts_add_blocked_when_bundled_disabled() {
        let state = test_state();
        let app = router(state.clone());

        post_json(
            &app,
            "/host/plugins/add",
            serde_json::json!({ "url": "bundled://discord" }),
        )
        .await;
        // Toggle Discord off.
        post_json(
            &app,
            "/host/plugins/set-enabled",
            serde_json::json!({ "url": "bundled://discord", "enabled": false }),
        )
        .await;

        let (_s, body) = post_json(
            &app,
            "/host/accounts/add",
            serde_json::json!({
                "backend": "discord",
                "account_id": "x",
                "token": "t",
                "display_name": "D"
            }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], false);
        assert!(resp["err"].as_str().unwrap().contains("not available"));
    }

    #[tokio::test]
    async fn accounts_remove_drops_entry() {
        let state = test_state();
        let app = router(state.clone());
        post_json(
            &app,
            "/host/accounts/add",
            serde_json::json!({
                "backend": "demo",
                "account_id": "alice",
                "token": "t",
                "display_name": "A"
            }),
        )
        .await;
        let (_s, body) = post_json(
            &app,
            "/host/accounts/remove",
            serde_json::json!({ "backend": "demo", "account_id": "alice" }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["removed"], true);
        let stored = sqlite_get(&state, ACCOUNT_TOKENS_KEY).unwrap().unwrap();
        assert!(stored.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn accounts_list_omits_token_field() {
        let state = test_state();
        let app = router(state.clone());
        post_json(
            &app,
            "/host/accounts/add",
            serde_json::json!({
                "backend": "demo",
                "account_id": "alice",
                "token": "secret-token-do-not-leak",
                "display_name": "Alice"
            }),
        )
        .await;
        let (status, body) = get(&app, "/host/accounts/list").await;
        assert_eq!(status, StatusCode::OK);
        // The serialised response must not contain `token` or
        // `refresh_token` fields — `AccountListEntry` deliberately omits
        // them. Asserting on the raw body is the strongest guarantee.
        assert!(
            !body.contains("secret-token-do-not-leak"),
            "secret token leaked in /host/accounts/list response: {body}"
        );
        assert!(
            !body.contains("\"token\""),
            "`token` field must not be serialised by /host/accounts/list"
        );
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        let accounts = resp["accounts"].as_array().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0]["account_id"], "alice");
        assert_eq!(accounts[0]["display_name"], "Alice");
    }

    #[tokio::test]
    async fn full_flow_add_plugin_then_login_then_list() {
        // The headline integration scenario: an MCP-driven AI does the
        // canonical "sideload + login" flow and the resulting state is
        // visible via the list endpoints. End-to-end through axum +
        // SQLite.
        let state = test_state();
        let app = router(state);

        // 1. Add a sideloaded plugin from a URL.
        let (_s, body) = post_json(
            &app,
            "/host/plugins/add",
            serde_json::json!({
                "url": "https://plugins.example.com/custom.wasm",
                "name": "Custom"
            }),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["added"], true);

        // 2. Confirm the plugin appears in the listing.
        let (_s, body) = get(&app, "/host/plugins/list").await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        let urls: Vec<&str> = resp["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p["url"].as_str())
            .collect();
        assert!(urls.contains(&"https://plugins.example.com/custom.wasm"));

        // 3. Login on a built-in backend (demo) so that the validation
        //    layer accepts it. The point is to assert that the listing
        //    shows the new account.
        post_json(
            &app,
            "/host/accounts/add",
            serde_json::json!({
                "backend": "demo",
                "account_id": "agent-test",
                "token": "tok",
                "display_name": "Agent Test"
            }),
        )
        .await;

        // 4. Verify the account is listed.
        let (_s, body) = get(&app, "/host/accounts/list").await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        let accounts = resp["accounts"].as_array().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0]["account_id"], "agent-test");
        assert_eq!(accounts[0]["backend"], "demo");
    }

    // ─── open-external route tests ────────────────────────────────────────────

    /// A valid HTTPS URL returns 200 with `ok: true`.
    /// Note: the test does NOT assert that a browser actually opened — that
    /// is an OS side-effect and not testable in unit tests without a display.
    /// The route returns 200 even in CI (no GUI) because `webbrowser::open`
    /// returns Ok(()) as long as it could *attempt* to hand off to the OS;
    /// the browser may not actually open in a headless environment, but the
    /// HTTP contract is satisfied.
    #[tokio::test]
    async fn open_external_valid_https_url_returns_200() {
        let app = router(test_state());
        let (status, body) = post_json(
            &app,
            "/host/open-external",
            serde_json::json!({ "url": "https://example.com" }),
        )
        .await;
        // We accept either 200 or 500 here: headless CI may not have a
        // browser, causing webbrowser::open to return Err. What we DO assert
        // is that the route exists (not 404) and parses the request correctly
        // (not 400 from a bad-request guard).
        assert_ne!(status, StatusCode::NOT_FOUND, "route must exist");
        assert_ne!(
            status,
            StatusCode::BAD_REQUEST,
            "valid https URL must not be 400: {body}"
        );
    }

    /// A `javascript:` URL must be rejected with HTTP 400 (security gate).
    #[tokio::test]
    async fn open_external_javascript_scheme_rejected() {
        let app = router(test_state());
        let (status, body) = post_json(
            &app,
            "/host/open-external",
            serde_json::json!({ "url": "javascript:alert(1)" }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "javascript: scheme must be rejected: {body}"
        );
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], false);
    }

    /// A malformed URL (not http/https) must be rejected with HTTP 400.
    #[tokio::test]
    async fn open_external_file_scheme_rejected() {
        let app = router(test_state());
        let (status, body) = post_json(
            &app,
            "/host/open-external",
            serde_json::json!({ "url": "file:///etc/passwd" }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "file: scheme must be rejected: {body}"
        );
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], false);
    }

    /// An `http://` URL must also pass the scheme gate (not just https).
    #[tokio::test]
    async fn open_external_http_scheme_allowed_by_gate() {
        let app = router(test_state());
        let (status, body) = post_json(
            &app,
            "/host/open-external",
            serde_json::json!({ "url": "http://example.com" }),
        )
        .await;
        // Same as the https test: 404 and 400 are forbidden; 200 or 500 accepted.
        assert_ne!(status, StatusCode::NOT_FOUND, "route must exist");
        assert_ne!(
            status,
            StatusCode::BAD_REQUEST,
            "valid http URL must not be 400: {body}"
        );
    }

    // ─── Phase A — caller identity (`plan-host-substrate-capability-gating.md`) ──

    #[tokio::test]
    async fn unauthenticated_host_request_is_rejected() {
        let app = router(test_state());
        let (status, body) = post_as(
            &app,
            "/host/kv/get",
            serde_json::json!({ "key": "account_tokens" }),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
        assert!(!body.contains("account_tokens"), "leaked state: {body}");
    }

    #[tokio::test]
    async fn wrong_token_is_rejected() {
        let app = router(test_state());
        for bad in ["not-the-token", ""] {
            let (status, body) = post_as(
                &app,
                "/host/kv/get",
                serde_json::json!({ "key": "anything" }),
                Some(bad),
                None,
            )
            .await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "`{bad}`: {body}");
        }
    }

    #[tokio::test]
    async fn correct_token_is_accepted() {
        let app = router(test_state());
        let (status, body) = post_json(
            &app,
            "/host/kv/set",
            serde_json::json!({ "key": "theme", "value": "dark" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true, "{body}");
    }

    /// Every guarded route rejects a missing token — not just the KV one
    /// the other tests exercise.
    #[tokio::test]
    async fn the_whole_host_surface_is_guarded() {
        let app = router(test_state());
        for path in [
            "/host/kv/get",
            "/host/kv/set",
            "/host/kv/delete",
            "/host/kv/clear",
            "/host/plugin-kv/get",
            "/host/exec",
            "/host/http",
            "/host/plugins/add",
            "/host/accounts/add",
            "/host/open-external",
            "/host",
        ] {
            let (status, body) =
                post_as(&app, path, serde_json::json!({}), None, None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{path} was open: {body}");
        }
    }

    /// The mint route is the bootstrap, so it must not itself be reachable
    /// from a page on another origin.
    #[tokio::test]
    async fn session_route_refuses_a_foreign_origin() {
        let app = router(test_state());
        let req = Request::builder()
            .method("GET")
            .uri(ROUTE_SESSION)
            .header("origin", "https://evil.test")
            .header("sec-fetch-site", "cross-site")
            .body(Body::empty())
            .unwrap();
        let (status, body) = send(&app, req).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(!body.contains(TEST_TOKEN), "token leaked: {body}");
    }

    #[tokio::test]
    async fn session_route_refuses_a_cross_site_fetch_metadata_header() {
        let app = router(test_state());
        let req = Request::builder()
            .method("GET")
            .uri(ROUTE_SESSION)
            .header("sec-fetch-site", "cross-site")
            .body(Body::empty())
            .unwrap();
        let (status, body) = send(&app, req).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(!body.contains(TEST_TOKEN), "token leaked: {body}");
    }

    #[tokio::test]
    async fn session_route_mints_for_the_shells_own_origin() {
        let app = router(test_state());
        let req = Request::builder()
            .method("GET")
            .uri(ROUTE_SESSION)
            .header("origin", "http://127.0.0.1:3000")
            .header("sec-fetch-site", "same-origin")
            .body(Body::empty())
            .unwrap();
        let (status, body) = send(&app, req).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["token"], TEST_TOKEN);
    }

    /// Phase A.4: `CorsLayer::allow_origin(Any)` is gone, so a foreign
    /// origin never gets an `Access-Control-Allow-Origin` echo and the
    /// browser refuses to hand the reply to the attacker's script.
    #[tokio::test]
    async fn cors_does_not_echo_a_foreign_origin() {
        let app = router(test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/host/kv/get")
            .header("content-type", "application/json")
            .header("origin", "https://evil.test")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({"key": "x"})).unwrap(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert!(
            resp.headers().get("access-control-allow-origin").is_none(),
            "foreign origin was echoed: {:?}",
            resp.headers()
        );
    }

    #[tokio::test]
    async fn cors_echoes_the_shells_own_origin() {
        let app = router(test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/host/kv/get")
            .header("content-type", "application/json")
            .header("origin", "http://localhost:3001")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({"key": "x"})).unwrap(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("http://localhost:3001")
        );
    }

    /// The named escape hatch keeps the tree runnable while the client
    /// half learns to send the header. It must be the *only* thing that
    /// opens the surface.
    #[tokio::test]
    async fn opt_out_env_var_disables_enforcement() {
        let state = test_state().with_auth(HostAuth::unenforced(TEST_TOKEN));
        let app = router(state);
        let (status, body) = post_as(
            &app,
            "/host/kv/set",
            serde_json::json!({ "key": "theme", "value": "dark" }),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    // ─── Phase B — exec allowlist + consent ──────────────────────────────

    /// A program that exists on every CI box and is safe to run.
    fn probe_program() -> PathBuf {
        for candidate in ["/bin/echo", "/usr/bin/echo", "/bin/true"] {
            let p = PathBuf::from(candidate);
            if p.exists() {
                return p;
            }
        }
        panic!("no probe program available");
    }

    async fn exec(app: &Router, program: &str, token: Option<&str>, plugin: Option<&str>) -> (StatusCode, String) {
        post_as(
            app,
            "/host/exec",
            serde_json::json!({ "call": "exec-command", "program": program, "args": [] }),
            token,
            plugin,
        )
        .await
    }

    #[tokio::test]
    async fn exec_of_a_non_allowlisted_program_is_denied() {
        let app = router(test_state());
        let (status, body) = exec(&app, "/bin/sh", Some(TEST_TOKEN), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(body.contains("exec denied"), "{body}");
    }

    #[tokio::test]
    async fn exec_of_a_relative_path_is_denied() {
        let app = router(test_state());
        for bad in ["./echo", "sub/echo", ""] {
            let (status, body) = exec(&app, bad, Some(TEST_TOKEN), None).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "`{bad}`: {body}");
        }
    }

    #[tokio::test]
    async fn exec_of_a_traversal_path_is_denied() {
        let app = router(test_state());
        let (status, body) = exec(&app, "/usr/../bin/sh", Some(TEST_TOKEN), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(body.contains(".."), "{body}");
    }

    /// Declaring is not consenting; consenting completes the gate.
    #[tokio::test]
    async fn declared_plus_consented_program_runs() {
        let app = router(test_state());
        let prog = probe_program();
        let prog_str = prog.display().to_string();

        let (status, body) = exec(&app, &prog_str, Some(TEST_TOKEN), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "undeclared must fail: {body}");

        let (status, body) = post_json(
            &app,
            "/host/exec/declare",
            serde_json::json!({ "programs": [prog_str] }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");

        let (status, body) = exec(&app, &prog_str, Some(TEST_TOKEN), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "consent still missing: {body}");
        assert!(body.contains("consent"), "{body}");

        let (status, body) = post_json(
            &app,
            "/host/exec/consent",
            serde_json::json!({ "program": prog_str }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");

        let (status, body) = exec(&app, &prog_str, Some(TEST_TOKEN), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"]["exit_code"], 0_i32, "{body}");
    }

    /// One caller's grant does not carry over to another caller.
    #[tokio::test]
    async fn a_plugin_does_not_inherit_the_shells_consent() {
        let app = router(test_state());
        let prog = probe_program().display().to_string();
        let _d = post_json(
            &app,
            "/host/exec/declare",
            serde_json::json!({ "programs": [prog] }),
        )
        .await;
        let _c = post_json(
            &app,
            "/host/exec/consent",
            serde_json::json!({ "program": prog }),
        )
        .await;

        let tok = plugin_token("plugin-a");
        let (status, body) = exec(&app, &prog, Some(&tok), Some("plugin-a")).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    }

    /// The SOLID-item-7 seams are real: swapping in the in-memory policy
    /// and a recording prompt drives the whole `/host/exec` route with no
    /// SQLite row, no UI and no shell.
    #[tokio::test]
    async fn exec_policy_and_prompt_seams_are_substitutable() {
        use poly_host_bridge::exec_policy::{InMemoryExecPolicy, RecordingConsentPrompt};

        let policy = Arc::new(InMemoryExecPolicy::new());
        let prompt = Arc::new(RecordingConsentPrompt::default());
        let prog = probe_program();
        policy.declare(&CallerId::Shell, vec![prog.clone()]);

        let policy_dyn: Arc<dyn ExecPolicy> = Arc::<InMemoryExecPolicy>::clone(&policy);
        let prompt_dyn: Arc<dyn ConsentPrompt> =
            Arc::<RecordingConsentPrompt>::clone(&prompt);
        let state = test_state()
            .with_exec_policy(policy_dyn)
            .with_consent_prompt(prompt_dyn);
        let app = router(state);
        let prog_str = prog.display().to_string();

        let (status, body) = exec(&app, &prog_str, Some(TEST_TOKEN), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(prompt.seen().len(), 1, "prompt was not surfaced");

        let canonical = std::fs::canonicalize(&prog).unwrap();
        policy.grant_consent(&CallerId::Shell, &canonical).unwrap();
        let (status, body) = exec(&app, &prog_str, Some(TEST_TOKEN), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(prompt.seen().len(), 1, "no second prompt after consent");
    }

    /// Phase B.5: the legacy tagged-union endpoint is no longer a second
    /// way into the subprocess spawner.
    #[tokio::test]
    async fn legacy_post_host_never_executes() {
        let app = router(test_state());
        let prog = probe_program().display().to_string();
        let _d = post_json(
            &app,
            "/host/exec/declare",
            serde_json::json!({ "programs": [prog] }),
        )
        .await;
        let _c = post_json(
            &app,
            "/host/exec/consent",
            serde_json::json!({ "program": prog }),
        )
        .await;

        let (status, body) = post_json(
            &app,
            "/host",
            serde_json::json!({ "call": "exec-command", "program": prog, "args": [] }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(resp["err"].is_string(), "legacy endpoint executed: {body}");
        assert!(resp["ok"].is_null(), "legacy endpoint executed: {body}");
    }

    #[tokio::test]
    async fn declare_and_consent_are_shell_only() {
        let app = router(test_state());
        let tok = plugin_token("plugin-a");
        let prog = probe_program().display().to_string();
        for (path, body) in [
            (
                "/host/exec/declare",
                serde_json::json!({ "programs": [prog.clone()] }),
            ),
            (
                "/host/exec/consent",
                serde_json::json!({ "program": prog.clone() }),
            ),
        ] {
            let (status, text) =
                post_as(&app, path, body, Some(&tok), Some("plugin-a")).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {text}");
        }
    }

    // ─── Phase C — KV namespacing + credential separation ────────────────

    /// C.4: not merely "distinct keys" — plugin A must be *refused* when
    /// it addresses plugin B's namespace.
    #[tokio::test]
    async fn plugin_a_cannot_read_plugin_bs_key() {
        let app = router(test_state());
        let tok_b = plugin_token("plugin-b");
        let secret = b64(b"plugin-b-secret");
        let (status, body) = post_as(
            &app,
            "/host/plugin-kv/set",
            serde_json::json!({ "plugin": "plugin-b", "key": "k", "value_b64": secret }),
            Some(&tok_b),
            Some("plugin-b"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");

        let tok_a = plugin_token("plugin-a");
        let (status, body) = post_as(
            &app,
            "/host/plugin-kv/get",
            serde_json::json!({ "plugin": "plugin-b", "key": "k" }),
            Some(&tok_a),
            Some("plugin-a"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], false, "{body}");
        assert!(resp["value_b64"].is_null(), "{body}");
        assert!(!body.contains(&secret), "value leaked: {body}");

        // …and asking within its own namespace sees its own (absent) key,
        // never plugin B's value.
        let (_s, body) = post_as(
            &app,
            "/host/plugin-kv/get",
            serde_json::json!({ "plugin": "plugin-a", "key": "k" }),
            Some(&tok_a),
            Some("plugin-a"),
        )
        .await;
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["ok"], true, "{body}");
        assert!(resp["value_b64"].is_null(), "{body}");
    }

    /// C.2: the credential row is outside every plugin's namespace.
    #[tokio::test]
    async fn a_plugin_cannot_name_the_account_tokens_row() {
        let state = test_state();
        sqlite_set(
            &state,
            ACCOUNT_TOKENS_KEY,
            &serde_json::json!([{ "backend": "matrix", "token": "super-secret" }]),
        )
        .unwrap();
        let app = router(state);
        let tok = plugin_token("plugin-a");
        for path in ["/host/kv/get", "/host/kv/delete"] {
            let (status, body) = post_as(
                &app,
                path,
                serde_json::json!({ "key": ACCOUNT_TOKENS_KEY }),
                Some(&tok),
                Some("plugin-a"),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{path}");
            let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(resp["ok"], false, "{path}: {body}");
            assert!(!body.contains("super-secret"), "{path} leaked: {body}");
        }
    }

    /// C.1/B.4: nobody — not even the shell — can name the rows that hold
    /// the exec declarations and consent grants.
    #[tokio::test]
    async fn host_internal_rows_are_unnameable_even_by_the_shell() {
        let app = router(test_state());
        for key in [EXEC_DECLARED_KEY, EXEC_CONSENT_KEY, "host:anything"] {
            let (status, body) = post_json(
                &app,
                "/host/kv/set",
                serde_json::json!({ "key": key, "value": ["/bin/sh"] }),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(resp["ok"], false, "{key} was writable: {body}");
        }
    }

    #[tokio::test]
    async fn a_plugin_cannot_clear_the_shared_table() {
        let app = router(test_state());
        let tok = plugin_token("plugin-a");
        let (status, body) = post_as(
            &app,
            "/host/kv/clear",
            serde_json::json!({}),
            Some(&tok),
            Some("plugin-a"),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    }

    /// C.3: a raw read of the SQLite row yields ciphertext, while the
    /// typed accessors keep working.
    #[tokio::test]
    async fn account_tokens_are_sealed_at_rest() {
        let state = test_state();
        let plaintext =
            serde_json::json!([{ "backend": "matrix", "token": "oauth-secret-value" }]);
        sqlite_set(&state, ACCOUNT_TOKENS_KEY, &plaintext).unwrap();

        let stored = raw_get(&state.db, ACCOUNT_TOKENS_KEY).unwrap().unwrap();
        let envelope = stored.as_str().expect("sealed row is a JSON string");
        assert!(envelope.starts_with(SEAL_PREFIX), "{envelope}");
        assert!(!envelope.contains("oauth-secret-value"), "{envelope}");

        assert_eq!(
            sqlite_get(&state, ACCOUNT_TOKENS_KEY).unwrap().unwrap(),
            plaintext
        );
    }

    /// Databases written before C.3 hold cleartext; they must keep
    /// loading, and get sealed on the next write.
    #[tokio::test]
    async fn legacy_cleartext_account_tokens_still_load() {
        let state = test_state();
        let legacy = serde_json::json!([{ "backend": "matrix", "token": "legacy" }]);
        raw_set(&state.db, ACCOUNT_TOKENS_KEY, &legacy).unwrap();
        assert_eq!(
            sqlite_get(&state, ACCOUNT_TOKENS_KEY).unwrap().unwrap(),
            legacy
        );

        sqlite_set(&state, ACCOUNT_TOKENS_KEY, &legacy).unwrap();
        let stored = raw_get(&state.db, ACCOUNT_TOKENS_KEY).unwrap().unwrap();
        assert!(
            stored.as_str().is_some_and(|s| s.starts_with(SEAL_PREFIX)),
            "rewrite did not seal: {stored}"
        );
    }

    #[tokio::test]
    async fn a_rebound_dns_name_cannot_reach_the_host_surface() {
        let app = router(test_state());
        // The mint route and a guarded route, both from a page whose DNS
        // was rebound to loopback: same-origin by every browser signal,
        // but the Host header still says `evil.test`.
        let req = Request::builder()
            .method("GET")
            .uri(ROUTE_SESSION)
            .header("host", "evil.test:3000")
            .header("sec-fetch-site", "same-origin")
            .body(Body::empty())
            .unwrap();
        let (status, body) = send(&app, req).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(!body.contains(TEST_TOKEN), "token leaked: {body}");

        let req = Request::builder()
            .method("POST")
            .uri("/host/kv/get")
            .header("content-type", "application/json")
            .header("host", "evil.test:3000")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({"key": "x"})).unwrap(),
            ))
            .unwrap();
        let (status, body) = send(&app, req).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    }

    #[tokio::test]
    async fn loopback_hosts_are_accepted() {
        let app = router(test_state());
        for host in ["127.0.0.1:3000", "localhost:3002", "[::1]:9333", "127.0.0.1"] {
            let req = Request::builder()
                .method("GET")
                .uri(ROUTE_SESSION)
                .header("host", host)
                .body(Body::empty())
                .unwrap();
            let (status, body) = send(&app, req).await;
            assert_eq!(status, StatusCode::OK, "{host}: {body}");
        }
    }

    /// The dev MCPs poll these before any WASM has run, so they must stay
    /// reachable without a token — and must stay side-effect free.
    #[tokio::test]
    async fn status_and_caps_stay_unauthenticated() {
        let app = router(test_state());
        for path in ["/host/status", "/host/caps"] {
            let req = Request::builder()
                .method("GET")
                .uri(path)
                .body(Body::empty())
                .unwrap();
            let (status, body) = send(&app, req).await;
            assert_eq!(status, StatusCode::OK, "{path}: {body}");
            assert!(!body.contains(TEST_TOKEN), "{path} leaked the token: {body}");
        }
    }

    /// The end-to-end shape C.5's redaction comment now depends on: the
    /// account inventory never carries key material.
    #[tokio::test]
    async fn accounts_list_is_the_widest_account_view() {
        let state = test_state();
        let app = router(state);
        let (status, body) = post_json(
            &app,
            "/host/accounts/add",
            serde_json::json!({
                "backend": "matrix",
                "account_id": "@a:example.test",
                "display_name": "A",
                "token": "oauth-secret-value"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (_s, body) = get(&app, "/host/accounts/list").await;
        assert!(!body.contains("oauth-secret-value"), "{body}");
    }
}
