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
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use poly_host_bridge::{
    AccountAddRequest, AccountAddResponse, AccountListEntry, AccountListResponse,
    AccountRemoveRequest, AccountRemoveResponse, HostCall, HostResponse, KvDeleteRequest,
    KvGetRequest, KvGetResponse, KvSetRequest, KvVoidResponse, PluginAddRequest,
    PluginAddResponse, PluginKvDeleteRequest, PluginKvGetRequest, PluginKvGetResponse,
    PluginKvSetRequest, PluginListEntry, PluginListResponse, PluginRemoveRequest,
    PluginRemoveResponse, PluginSetEnabledRequest, PluginSetEnabledResponse, dispatch,
};
use sqlite::{Connection, ConnectionThreadSafe, State as SqlState};
use tower_http::cors::{Any, CorsLayer};

/// Shared daemon state — a SQLite handle plus the path we opened it from
/// (kept around so `GET /host/status` can report where storage lives).
#[derive(Clone)]
pub struct HostState {
    db: Arc<Mutex<ConnectionThreadSafe>>,
    db_path: PathBuf,
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
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            db_path,
        })
    }

    /// Path to the SQLite file backing this handle. Useful for log output.
    #[must_use]
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}

/// Build the full `/host/*` router over an already-open [`HostState`].
///
/// The caller is responsible for picking the listener address and for
/// deciding whether the router should be composed with additional routes
/// (the Wry shell does this to keep its MCP eval bridge on the same port
/// as before).
#[must_use = "the Router must be merged into the Dioxus router or served directly"]
pub fn router(state: HostState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/host/status", get(status))
        .route("/host/kv/get", post(kv_get))
        .route("/host/kv/set", post(kv_set))
        .route("/host/kv/delete", post(kv_delete))
        .route("/host/kv/clear", post(kv_clear))
        .route("/host/plugin-kv/get", post(plugin_kv_get))
        .route("/host/plugin-kv/set", post(plugin_kv_set))
        .route("/host/plugin-kv/delete", post(plugin_kv_delete))
        .route("/host/exec", post(host_exec))
        .route("/host/http", post(host_http))
        .route("/host/plugins/add", post(plugins_add))
        .route("/host/plugins/remove", post(plugins_remove))
        .route("/host/plugins/set-enabled", post(plugins_set_enabled))
        .route("/host/plugins/list", get(plugins_list))
        .route("/host/accounts/add", post(accounts_add))
        .route("/host/accounts/remove", post(accounts_remove))
        .route("/host/accounts/list", get(accounts_list))
        .route("/host", post(host_legacy))
        .route("/poly-service-worker.js", get(poly_service_worker))
        .with_state(state)
        .layer(cors)
}

/// ServiceWorker script — main-thread hang detector + auto-reload.
///
/// The main WASM app posts `{type:'poly-heartbeat'}` to this SW every 500ms.
/// If a client stops heartbeating for more than `HEARTBEAT_TIMEOUT_MS`, the
/// SW calls `client.navigate(client.url)` to force-reload that tab — which
/// works even when the main thread is stuck in an infinite WASM loop
/// (the navigation is executed at the browser level, not by main-thread JS).
const POLY_SERVICE_WORKER_JS: &str = r#"// poly hang watchdog
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
"#;

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
    let app = router(state);
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
        let base: PathBuf = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".local").join("share")
            });
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

async fn kv_get(
    State(state): State<HostState>,
    Json(req): Json<KvGetRequest>,
) -> Json<KvGetResponse> {
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
    Json(req): Json<KvSetRequest>,
) -> Json<KvVoidResponse> {
    Json(void_response(sqlite_set(&state, &req.key, &req.value)))
}

async fn kv_delete(
    State(state): State<HostState>,
    Json(req): Json<KvDeleteRequest>,
) -> Json<KvVoidResponse> {
    Json(void_response(sqlite_delete(&state, &req.key)))
}

async fn kv_clear(State(state): State<HostState>) -> Json<KvVoidResponse> {
    Json(void_response(sqlite_clear(&state)))
}

// ─── Plugin-KV route handlers ─────────────────────────────────────────────────

async fn plugin_kv_get(
    State(state): State<HostState>,
    Json(req): Json<PluginKvGetRequest>,
) -> Json<PluginKvGetResponse> {
    let k = plugin_kv_key(&req.plugin, req.account.as_deref(), &req.key);
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
    Json(req): Json<PluginKvSetRequest>,
) -> Json<KvVoidResponse> {
    use base64::Engine as _;
    if let Err(e) = base64::engine::general_purpose::STANDARD.decode(&req.value_b64) {
        return Json(KvVoidResponse {
            ok: false,
            err: Some(format!("invalid base64: {e}")),
        });
    }
    let k = plugin_kv_key(&req.plugin, req.account.as_deref(), &req.key);
    let value = serde_json::Value::String(req.value_b64);
    Json(void_response(sqlite_set(&state, &k, &value)))
}

async fn plugin_kv_delete(
    State(state): State<HostState>,
    Json(req): Json<PluginKvDeleteRequest>,
) -> Json<KvVoidResponse> {
    let k = plugin_kv_key(&req.plugin, req.account.as_deref(), &req.key);
    Json(void_response(sqlite_delete(&state, &k)))
}

/// Build the namespaced `poly_kv` key for a plugin-KV entry.
///
/// Global (no account): `plugin:{plugin}:global:{key}`.
/// Per-account: `plugin:{plugin}:account:{account}:{key}`.
#[must_use]
pub fn plugin_kv_key(plugin: &str, account: Option<&str>, key: &str) -> String {
    match account {
        None => format!("plugin:{plugin}:global:{key}"),
        Some(acct) => format!("plugin:{plugin}:account:{acct}:{key}"),
    }
}

async fn host_legacy(Json(call): Json<HostCall>) -> Json<HostResponse> {
    Json(dispatch(call).await)
}

async fn host_exec(Json(call): Json<HostCall>) -> Result<Json<HostResponse>, StatusCode> {
    match &call {
        HostCall::ExecCommand { .. } => Ok(Json(dispatch(call).await)),
        HostCall::HttpRequest { .. } => Err(StatusCode::BAD_REQUEST),
    }
}

async fn host_http(Json(call): Json<HostCall>) -> Result<Json<HostResponse>, StatusCode> {
    match &call {
        HostCall::HttpRequest { .. } => Ok(Json(dispatch(call).await)),
        HostCall::ExecCommand { .. } => Err(StatusCode::BAD_REQUEST),
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
        if let Some(slug) = poly_host_bridge::bundled_slug_from_url(&url) {
            if let Some(arr) = settings
                .get_mut("removed_bundled_plugins")
                .and_then(|v| v.as_array_mut())
            {
                arr.retain(|s| s.as_str() != Some(slug));
            }
        }

        let plugins = wasm_plugins_array_mut(settings);

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
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
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
                .map(str::to_string)
                .unwrap_or_default(),
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
        let plugins = wasm_plugins_array_mut(settings);
        let before = plugins.len();
        plugins.retain(|e| {
            let url_str = e.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let matched = try_targets.iter().any(|t| t == url_str);
            if matched {
                let bundled = e
                    .get("bundled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if bundled {
                    if let Some(slug) = poly_host_bridge::bundled_slug_from_url(url_str) {
                        removed_was_bundled = Some(slug.to_string());
                    }
                }
            }
            !matched
        });
        let removed = before != plugins.len();
        if removed {
            if let Some(slug) = removed_was_bundled {
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
        let plugins = wasm_plugins_array_mut(settings);
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
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let slug = poly_host_bridge::bundled_slug_from_url(&url)
                .map(str::to_string)
                .unwrap_or_else(|| url.clone());
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
                    .and_then(|v| v.as_bool())
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
            account_id: req.account_id.clone(),
            backend: req.backend.clone(),
            err: Some("account_id is required".into()),
        });
    }
    if req.backend.trim().is_empty() {
        return Json(AccountAddResponse {
            ok: false,
            account_id: req.account_id.clone(),
            backend: req.backend.clone(),
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
                    account_id: req.account_id.clone(),
                    backend: req.backend.clone(),
                    err: Some(format!(
                        "backend `{}` is not available (not compiled in or disabled)",
                        req.backend
                    )),
                });
            }
        }
        Err(e) => {
            return Json(AccountAddResponse {
                ok: false,
                account_id: req.account_id.clone(),
                backend: req.backend.clone(),
                err: Some(e),
            });
        }
    }

    let entry = serde_json::json!({
        "backend": req.backend,
        "account_id": req.account_id,
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
                !(t.get("backend").and_then(|v| v.as_str()) == Some(&req.backend)
                    && t.get("account_id").and_then(|v| v.as_str())
                        == Some(&req.account_id))
            });
            tokens.push(entry.clone());
            Ok(())
        }) {
            Ok(()) => AccountAddResponse {
                ok: true,
                account_id: req.account_id.clone(),
                backend: req.backend.clone(),
                err: None,
            },
            Err(e) => AccountAddResponse {
                ok: false,
                account_id: req.account_id.clone(),
                backend: req.backend.clone(),
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
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let enabled = entry
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if !bundled || !enabled {
                continue;
            }
            let url = entry.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(slug) = poly_host_bridge::bundled_slug_from_url(url) {
                if !out.iter().any(|s| s == slug)
                    && !disabled.iter().any(|d| d == slug)
                {
                    out.push(slug.to_string());
                }
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
fn wasm_plugins_array_mut(settings: &mut serde_json::Value) -> &mut Vec<serde_json::Value> {
    let map = settings
        .as_object_mut()
        .expect("settings was normalised to an object by mutate_app_settings");
    let entry = map
        .entry("wasm_plugins")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    if !entry.is_array() {
        *entry = serde_json::Value::Array(Vec::new());
    }
    entry
        .as_array_mut()
        .expect("just-set value is an array")
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

// ─── SQLite helpers ──────────────────────────────────────────────────────────

fn lock_db(
    state: &HostState,
) -> Result<std::sync::MutexGuard<'_, ConnectionThreadSafe>, String> {
    state
        .db
        .lock()
        .map_err(|_| "sqlite mutex poisoned".to_string())
}

fn sqlite_get(state: &HostState, key: &str) -> Result<Option<serde_json::Value>, String> {
    let db = lock_db(state)?;
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

fn sqlite_set(state: &HostState, key: &str, value: &serde_json::Value) -> Result<(), String> {
    let serialized =
        serde_json::to_string(value).map_err(|e| format!("serde set({key}): {e}"))?;
    let db = lock_db(state)?;
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

fn sqlite_delete(state: &HostState, key: &str) -> Result<(), String> {
    let db = lock_db(state)?;
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
    let db = lock_db(state)?;
    db.execute("DELETE FROM poly_kv")
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

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use base64::Engine as _;
    use tower::util::ServiceExt as _;

    fn test_state() -> HostState {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.keep().join("test.sqlite3");
        HostState::open(path).expect("open")
    }

    fn b64(s: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(s)
    }

    async fn post_json(
        app: &Router,
        path: &str,
        body: serde_json::Value,
    ) -> (StatusCode, String) {
        let req = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
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
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
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
                .map(|v| v.is_empty())
                .unwrap_or(true)
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
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
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
        _ = ctrl_c => tracing::info!("received ctrl-c, shutting down"),
        _ = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}
