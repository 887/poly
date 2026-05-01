//! Phase D integration tests for the `client_settings_*` MCP tool family.
//!
//! Each test spins up a minimal in-process axum server that implements the
//! four host-bridge KV routes (`/host/kv/{get,set,delete,clear}`).  The
//! `ClientConfigStore` is pointed at that server so we can do full round-trips
//! without a live poly-host daemon.
//!
//! Audit-row assertions query the `MemoryDb` (SQLite in-memory) directly after
//! each `set_*` tool call.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    extract::State,
    routing::post,
    Json, Router,
};
use poly_chat_mcp::{memory::MemoryDb, state::BackendPool, tools};
use poly_host_bridge::{
    client_config::ClientConfigStore, Client,
    KvDeleteRequest, KvGetRequest, KvGetResponse, KvSetRequest, KvVoidResponse,
};
use serde_json::{json, Value};
use tokio::net::TcpListener;

// ── Minimal host-bridge KV mock ──────────────────────────────────────────────

type KvStore = Arc<Mutex<HashMap<String, Value>>>;

async fn kv_get(State(store): State<KvStore>, Json(req): Json<KvGetRequest>) -> Json<KvGetResponse> {
    let map = store.lock().unwrap();
    let value = map.get(&req.key).cloned();
    Json(KvGetResponse { ok: true, value, err: None })
}

async fn kv_set(State(store): State<KvStore>, Json(req): Json<KvSetRequest>) -> Json<KvVoidResponse> {
    let mut map = store.lock().unwrap();
    map.insert(req.key, req.value);
    Json(KvVoidResponse { ok: true, err: None })
}

async fn kv_delete(State(store): State<KvStore>, Json(req): Json<KvDeleteRequest>) -> Json<KvVoidResponse> {
    let mut map = store.lock().unwrap();
    map.remove(&req.key);
    Json(KvVoidResponse { ok: true, err: None })
}

async fn kv_clear(State(store): State<KvStore>) -> Json<KvVoidResponse> {
    let mut map = store.lock().unwrap();
    map.clear();
    Json(KvVoidResponse { ok: true, err: None })
}

struct MockBridge {
    base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl MockBridge {
    async fn start() -> Self {
        let store: KvStore = Arc::new(Mutex::new(HashMap::new()));
        let app = Router::new()
            .route("/host/kv/get", post(kv_get))
            .route("/host/kv/set", post(kv_set))
            .route("/host/kv/delete", post(kv_delete))
            .route("/host/kv/clear", post(kv_clear))
            .with_state(store);

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { rx.await.ok(); })
                .await
                .ok();
        });
        // Brief settle time.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            _shutdown: tx,
        }
    }
}

// ── Test helpers ─────────────────────────────────────────────────────────────

fn test_mem() -> MemoryDb {
    MemoryDb::open(":memory:").expect("in-memory MemoryDb")
}

/// Build a `BackendPool` whose `config_store` points at the given mock bridge.
fn pool_with_bridge(base_url: &str) -> BackendPool {
    let client = Client::with_url(format!("{base_url}/host"));
    let store = ClientConfigStore::from_client(client);
    BackendPool::new_with_config_store(store)
}

async fn call(pool: &mut BackendPool, mem: &MemoryDb, tool: &str, args: Value) -> Value {
    tools::dispatch(tool, &args, pool, mem).await
}

fn assert_ok(result: &Value, context: &str) {
    assert!(
        !result.get("isError").and_then(|e| e.as_bool()).unwrap_or(false),
        "{context}: tool returned error: {}",
        result
    );
}

fn text_of(result: &Value) -> String {
    result
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// D.7.1 — set_version_override → get_version: round-trip persists the value.
#[tokio::test]
async fn test_set_and_get_version_override_round_trip() {
    let bridge = MockBridge::start().await;
    let mem = test_mem();
    let mut pool = pool_with_bridge(&bridge.base_url);

    // Set override.
    let r = call(&mut pool, &mem, "client_settings_set_version_override",
        json!({ "backend_id": "discord", "override": "poly-discord/0.0.0" }),
    ).await;
    assert_ok(&r, "set_version_override");
    let text = text_of(&r);
    assert!(text.contains("poly-discord/0.0.0"), "set response should mention version: {text}");

    // Get version — should return the override.
    let r = call(&mut pool, &mem, "client_settings_get_version",
        json!({ "backend_id": "discord" }),
    ).await;
    assert_ok(&r, "get_version after set");
    let text = text_of(&r);
    assert!(text.contains("poly-discord/0.0.0"), "get should return override: {text}");
    assert!(text.contains("override"), "source should be 'override': {text}");
}

/// D.7.2 — client_settings_list snapshot includes the backend.
#[tokio::test]
async fn test_client_settings_list_snapshot() {
    let bridge = MockBridge::start().await;
    let mem = test_mem();
    let mut pool = pool_with_bridge(&bridge.base_url);

    // Set a version override for "stoat".
    let r = call(&mut pool, &mem, "client_settings_set_version_override",
        json!({ "backend_id": "stoat", "override": "stoat-test/1.0" }),
    ).await;
    assert_ok(&r, "set_version_override for stoat");

    // List just the stoat backend.
    let r = call(&mut pool, &mem, "client_settings_list",
        json!({ "backend_id": "stoat" }),
    ).await;
    assert_ok(&r, "list single backend");
    let text = text_of(&r);
    assert!(text.contains("stoat"), "response should include backend_id: {text}");
    assert!(text.contains("stoat-test/1.0"), "response should include version override: {text}");
}

/// D.7.3 — mechanism toggle: set → list_mechanisms shows the new state.
#[tokio::test]
async fn test_mechanism_toggle_round_trip() {
    let bridge = MockBridge::start().await;
    let mem = test_mem();
    let mut pool = pool_with_bridge(&bridge.base_url);

    // Enable a mechanism.
    let r = call(&mut pool, &mem, "client_settings_set_mechanism",
        json!({ "backend_id": "discord", "mechanism_id": "captcha-sandbox", "enabled": true }),
    ).await;
    assert_ok(&r, "set_mechanism enable");

    // List mechanisms — should show captcha-sandbox: true.
    let r = call(&mut pool, &mem, "client_settings_list_mechanisms",
        json!({ "backend_id": "discord" }),
    ).await;
    assert_ok(&r, "list_mechanisms after enable");
    let text = text_of(&r);
    assert!(text.contains("captcha-sandbox"), "mechanism should appear: {text}");
    assert!(text.contains("true"), "mechanism should be enabled: {text}");

    // Disable the mechanism.
    let r = call(&mut pool, &mem, "client_settings_set_mechanism",
        json!({ "backend_id": "discord", "mechanism_id": "captcha-sandbox", "enabled": false }),
    ).await;
    assert_ok(&r, "set_mechanism disable");

    // List again — should show captcha-sandbox: false.
    let r = call(&mut pool, &mem, "client_settings_list_mechanisms",
        json!({ "backend_id": "discord" }),
    ).await;
    assert_ok(&r, "list_mechanisms after disable");
    let text = text_of(&r);
    assert!(text.contains("false"), "mechanism should be disabled: {text}");
}

/// D.7.4 — audit-row count delta: each set_* call inserts exactly +1 row.
#[tokio::test]
async fn test_audit_row_count_delta_after_set() {
    let bridge = MockBridge::start().await;
    let mem = test_mem();
    let mut pool = pool_with_bridge(&bridge.base_url);
    let bid = "matrix";

    let before = mem.count_client_settings_audit(bid).expect("count before");
    assert_eq!(before, 0, "no audit rows before any write");

    // set_version_override → +1 row.
    let r = call(&mut pool, &mem, "client_settings_set_version_override",
        json!({ "backend_id": bid, "override": "poly-matrix/1.2.3" }),
    ).await;
    assert_ok(&r, "set_version_override");

    let after_v = mem.count_client_settings_audit(bid).expect("count after version set");
    assert_eq!(after_v, before + 1, "set_version_override should emit exactly one audit row");

    // set_mechanism → +1 row.
    let r = call(&mut pool, &mem, "client_settings_set_mechanism",
        json!({ "backend_id": bid, "mechanism_id": "tls-pinning", "enabled": true }),
    ).await;
    assert_ok(&r, "set_mechanism");

    let after_m = mem.count_client_settings_audit(bid).expect("count after mechanism set");
    assert_eq!(after_m, after_v + 1, "set_mechanism should emit exactly one audit row");
}

/// D.7.5 — clear version override: passing null clears the value and get returns default.
#[tokio::test]
async fn test_clear_version_override() {
    let bridge = MockBridge::start().await;
    let mem = test_mem();
    let mut pool = pool_with_bridge(&bridge.base_url);

    // Set then clear.
    let r = call(&mut pool, &mem, "client_settings_set_version_override",
        json!({ "backend_id": "teams", "override": "poly-teams/9.9" }),
    ).await;
    assert_ok(&r, "set override");

    let r = call(&mut pool, &mem, "client_settings_set_version_override",
        json!({ "backend_id": "teams", "override": null }),
    ).await;
    assert_ok(&r, "clear override");
    let text = text_of(&r);
    assert!(text.contains("cleared"), "clear should say 'cleared': {text}");

    // get_version should now return source=default.
    let r = call(&mut pool, &mem, "client_settings_get_version",
        json!({ "backend_id": "teams" }),
    ).await;
    assert_ok(&r, "get after clear");
    let text = text_of(&r);
    assert!(text.contains("default"), "source should be 'default' after clear: {text}");

    // Audit: 2 rows (set + clear).
    let count = mem.count_client_settings_audit("teams").expect("audit count");
    assert_eq!(count, 2, "two audit rows expected (set + clear): got {count}");
}
