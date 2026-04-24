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
    HostCall, HostResponse, KvDeleteRequest, KvGetRequest, KvGetResponse, KvSetRequest,
    KvVoidResponse, PluginKvDeleteRequest, PluginKvGetRequest, PluginKvGetResponse,
    PluginKvSetRequest, dispatch,
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
