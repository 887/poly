//! End-to-end MCP tool integration tests.
//!
//! Spins up Discord + Teams + Lemmy + HackerNews + Stoat + Matrix test servers
//! in-process, then exercises the `poly-chat-mcp` tool dispatch layer end-to-end.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;

use poly_chat_mcp::memory::MemoryDb;
use poly_chat_mcp::state::BackendPool;
use poly_chat_mcp::tools;
use poly_test_discord::{DiscordState, router as discord_router};
use poly_test_hackernews::HnState;
use poly_test_lemmy::LemmyState;
use poly_test_matrix::{MatrixState, router as matrix_router};
use poly_test_stoat::{StoatState, router as stoat_router};
use poly_test_teams::{TeamsState, router as teams_router};
use serde_json::{Value, json};
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Test server infrastructure
// ---------------------------------------------------------------------------

struct TestSrv {
    base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestSrv {
    async fn discord() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(DiscordState::new());
        state.seed();
        let app = discord_router(state);
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { rx.await.ok(); })
                .await
                .ok();
        });
        Self { base_url: format!("http://127.0.0.1:{port}"), _shutdown: tx }
    }

    async fn teams() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(TeamsState::new());
        state.seed();
        let app = teams_router(state);
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { rx.await.ok(); })
                .await
                .ok();
        });
        Self { base_url: format!("http://127.0.0.1:{port}"), _shutdown: tx }
    }

    async fn lemmy() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(LemmyState::new());
        state.seed();
        let app = poly_test_lemmy::router_with_state(state);
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { rx.await.ok(); })
                .await
                .ok();
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        Self { base_url: format!("http://127.0.0.1:{port}"), _shutdown: tx }
    }

    async fn hackernews() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(HnState::new());
        state.seed();
        let app = poly_test_hackernews::router(state);
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { rx.await.ok(); })
                .await
                .ok();
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        Self { base_url: format!("http://127.0.0.1:{port}"), _shutdown: tx }
    }

    async fn stoat() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(StoatState::new());
        state.seed();
        let app = stoat_router(state);
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { rx.await.ok(); })
                .await
                .ok();
        });
        Self { base_url: format!("http://127.0.0.1:{port}"), _shutdown: tx }
    }

    async fn matrix() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(MatrixState::new());
        state.seed();
        let app = matrix_router(state);
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { rx.await.ok(); })
                .await
                .ok();
        });
        Self { base_url: format!("http://127.0.0.1:{port}"), _shutdown: tx }
    }
}

// ---------------------------------------------------------------------------
// Helper — call dispatch and assert not error
// ---------------------------------------------------------------------------

fn test_mem() -> MemoryDb {
    MemoryDb::open(":memory:").expect("in-memory MemoryDb")
}

async fn call(pool: &mut BackendPool, tool: &str, args: Value) -> Value {
    let mem = test_mem();
    tools::dispatch(tool, &args, pool, &mem).await
}

fn assert_ok(result: &Value) {
    assert!(
        !result.get("isError").and_then(|e| e.as_bool()).unwrap_or(false),
        "tool returned error: {}",
        result
    );
}

fn assert_err(result: &Value) {
    assert!(
        result.get("isError").and_then(|e| e.as_bool()).unwrap_or(false),
        "expected error but got success: {}",
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

fn parse_text<T: serde::de::DeserializeOwned>(result: &Value) -> T {
    let text = text_of(result);
    // Strip leading prose before JSON (e.g. "Logged in successfully.\n{...}")
    let json_start = text.find('[').or_else(|| text.find('{')).unwrap_or(0);
    serde_json::from_str(&text[json_start..]).expect("parse text as JSON")
}

// ---------------------------------------------------------------------------
// Discord MCP tool tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn discord_test_signin_and_list_accounts() {
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();

    let result = call(&mut pool, "test_signin", json!({
        "backend": "discord",
        "url": srv.base_url,
        "username": "koala"
    })).await;
    assert_ok(&result);
    assert!(text_of(&result).contains("koala"), "session should mention koala");

    let accounts = call(&mut pool, "list_accounts", json!({})).await;
    assert_ok(&accounts);
    let list: Vec<Value> = parse_text(&accounts);
    assert!(!list.is_empty(), "should have one account");
    // BackendType serializes via Debug as BackendId("discord")
    assert!(
        list[0]["backend"].as_str().unwrap_or("").contains("discord"),
        "expected discord backend, got: {}",
        list[0]["backend"]
    );
}

#[tokio::test]
async fn discord_list_servers() {
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": srv.base_url, "username": "koala"
    })).await;

    let result = call(&mut pool, "list_servers", json!({ "backend": "discord" })).await;
    assert_ok(&result);
    let servers: Vec<Value> = parse_text(&result);
    assert!(!servers.is_empty());
    let names: Vec<&str> = servers.iter().filter_map(|s| s["name"].as_str()).collect();
    assert!(names.contains(&"Australiana"));
    assert!(names.contains(&"Wildlife Chat"));
}

#[tokio::test]
async fn discord_list_channels() {
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": srv.base_url, "username": "koala"
    })).await;

    let result = call(&mut pool, "list_channels", json!({
        "backend": "discord",
        "server_id": "100"
    })).await;
    assert_ok(&result);
    let channels: Vec<Value> = parse_text(&result);
    assert!(!channels.is_empty());
    let names: Vec<&str> = channels.iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(names.contains(&"general"));
    assert!(names.contains(&"random"));
}

#[tokio::test]
async fn discord_get_messages() {
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": srv.base_url, "username": "koala"
    })).await;

    let result = call(&mut pool, "get_messages", json!({
        "backend": "discord",
        "channel_id": "200",
        "limit": 10
    })).await;
    assert_ok(&result);
    let msgs: Vec<Value> = parse_text(&result);
    assert!(!msgs.is_empty());
}

#[tokio::test]
async fn discord_send_message() {
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": srv.base_url, "username": "koala"
    })).await;

    let result = call(&mut pool, "send_message", json!({
        "backend": "discord",
        "channel_id": "200",
        "text": "Hello from MCP test!"
    })).await;
    assert_ok(&result);
    assert!(text_of(&result).contains("Hello from MCP test!"), "sent message text should appear in result");
}

#[tokio::test]
async fn discord_list_dms() {
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": srv.base_url, "username": "koala"
    })).await;

    let result = call(&mut pool, "list_dms", json!({ "backend": "discord" })).await;
    assert_ok(&result);
    // Just verify it doesn't error — DMs may be empty
}

#[tokio::test]
async fn discord_logout() {
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": srv.base_url, "username": "koala"
    })).await;

    // Get account_id from the accounts list
    let accounts: Vec<Value> = {
        let r = call(&mut pool, "list_accounts", json!({})).await;
        parse_text(&r)
    };
    let account_id = accounts[0]["user_id"].as_str().unwrap().to_string();

    let result = call(&mut pool, "logout", json!({
        "backend": "discord",
        "account_id": account_id
    })).await;
    assert_ok(&result);

    // Verify it's gone
    let after: Vec<Value> = parse_text(&call(&mut pool, "list_accounts", json!({})).await);
    assert!(after.is_empty(), "pool should be empty after logout");
}

#[tokio::test]
async fn discord_unknown_tool_returns_error() {
    let mut pool = BackendPool::new();
    let result = call(&mut pool, "not_a_real_tool", json!({})).await;
    assert_err(&result);
}

#[tokio::test]
async fn discord_list_servers_without_login_returns_error() {
    let mut pool = BackendPool::new();
    let result = call(&mut pool, "list_servers", json!({ "backend": "discord" })).await;
    assert_err(&result);
}

// ---------------------------------------------------------------------------
// Teams MCP tool tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn teams_test_signin_and_list_accounts() {
    let srv = TestSrv::teams().await;
    let mut pool = BackendPool::new();

    let result = call(&mut pool, "test_signin", json!({
        "backend": "teams",
        "url": srv.base_url,
        "username": "Sheep"
    })).await;
    assert_ok(&result);
    assert!(text_of(&result).contains("Sheep"), "session should mention Sheep");
}

#[tokio::test]
async fn teams_list_servers() {
    let srv = TestSrv::teams().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "teams", "url": srv.base_url, "username": "Sheep"
    })).await;

    let result = call(&mut pool, "list_servers", json!({ "backend": "teams" })).await;
    assert_ok(&result);
    let servers: Vec<Value> = parse_text(&result);
    assert!(!servers.is_empty());
    let names: Vec<&str> = servers.iter().filter_map(|s| s["name"].as_str()).collect();
    assert!(names.contains(&"Contoso Corp"));
    assert!(names.contains(&"Project Alpha"));
}

#[tokio::test]
async fn teams_list_channels() {
    let srv = TestSrv::teams().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "teams", "url": srv.base_url, "username": "Sheep"
    })).await;

    let result = call(&mut pool, "list_channels", json!({
        "backend": "teams",
        "server_id": "T001"
    })).await;
    assert_ok(&result);
    let channels: Vec<Value> = parse_text(&result);
    assert!(!channels.is_empty());
    let names: Vec<&str> = channels.iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(names.contains(&"General"));
    assert!(names.contains(&"Engineering"));
}

#[tokio::test]
async fn teams_get_messages() {
    let srv = TestSrv::teams().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "teams", "url": srv.base_url, "username": "Sheep"
    })).await;

    let result = call(&mut pool, "get_messages", json!({
        "backend": "teams",
        "channel_id": "T001/CH001",
        "limit": 10
    })).await;
    assert_ok(&result);
    let msgs: Vec<Value> = parse_text(&result);
    assert!(!msgs.is_empty());
}

#[tokio::test]
async fn teams_send_message() {
    let srv = TestSrv::teams().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "teams", "url": srv.base_url, "username": "Sheep"
    })).await;

    let result = call(&mut pool, "send_message", json!({
        "backend": "teams",
        "channel_id": "T001/CH001",
        "text": "Hello from Teams MCP test!"
    })).await;
    assert_ok(&result);
}

#[tokio::test]
async fn teams_list_dms() {
    let srv = TestSrv::teams().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "teams", "url": srv.base_url, "username": "Sheep"
    })).await;

    let result = call(&mut pool, "list_dms", json!({ "backend": "teams" })).await;
    assert_ok(&result);
    let dms: Vec<Value> = parse_text(&result);
    assert!(!dms.is_empty(), "Sheep should have at least one chat");
}

#[tokio::test]
async fn multi_backend_pool() {
    let discord_srv = TestSrv::discord().await;
    let teams_srv = TestSrv::teams().await;
    let mut pool = BackendPool::new();

    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": discord_srv.base_url, "username": "koala"
    })).await;
    call(&mut pool, "test_signin", json!({
        "backend": "teams", "url": teams_srv.base_url, "username": "Sheep"
    })).await;

    let accounts: Vec<Value> = parse_text(&call(&mut pool, "list_accounts", json!({})).await);
    assert_eq!(accounts.len(), 2, "should have 2 accounts");

    let backends: Vec<&str> = accounts.iter().filter_map(|a| a["backend"].as_str()).collect();
    assert!(backends.iter().any(|b| b.contains("discord")), "discord in pool");
    assert!(backends.iter().any(|b| b.contains("teams")), "teams in pool");

    // Both backends serve their own servers
    let d_servers: Vec<Value> = parse_text(&call(&mut pool, "list_servers", json!({ "backend": "discord" })).await);
    let t_servers: Vec<Value> = parse_text(&call(&mut pool, "list_servers", json!({ "backend": "teams" })).await);
    assert!(!d_servers.is_empty());
    assert!(!t_servers.is_empty());
}

// ---------------------------------------------------------------------------
// Lemmy MCP tool tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lemmy_test_signin_and_list_accounts() {
    let srv = TestSrv::lemmy().await;
    let mut pool = BackendPool::new();

    let result = call(&mut pool, "test_signin", json!({
        "backend": "lemmy",
        "url": srv.base_url,
        "username": "testuser"
    })).await;
    assert_ok(&result);
    assert!(text_of(&result).contains("testuser") || text_of(&result).contains("Test User"),
        "session should mention user, got: {}", text_of(&result));

    let accounts = call(&mut pool, "list_accounts", json!({})).await;
    assert_ok(&accounts);
    let list: Vec<Value> = parse_text(&accounts);
    assert!(!list.is_empty(), "should have one account");
    assert!(
        list[0]["backend"].as_str().unwrap_or("").contains("lemmy"),
        "expected lemmy backend, got: {}", list[0]["backend"]
    );
}

#[tokio::test]
async fn lemmy_list_servers() {
    let srv = TestSrv::lemmy().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "lemmy", "url": srv.base_url, "username": "testuser"
    })).await;

    let result = call(&mut pool, "list_servers", json!({ "backend": "lemmy" })).await;
    assert_ok(&result);
    let servers: Vec<Value> = parse_text(&result);
    assert!(!servers.is_empty(), "should have seeded communities");
    let names: Vec<&str> = servers.iter().filter_map(|s| s["name"].as_str()).collect();
    assert!(names.contains(&"Rust Programming"), "rust community expected, got: {names:?}");
}

#[tokio::test]
async fn lemmy_get_messages() {
    let srv = TestSrv::lemmy().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "lemmy", "url": srv.base_url, "username": "testuser"
    })).await;

    let result = call(&mut pool, "get_messages", json!({
        "backend": "lemmy",
        "channel_id": "lemmy-feed-1",
        "limit": 5
    })).await;
    assert_ok(&result);
    let msgs: Vec<Value> = parse_text(&result);
    assert!(!msgs.is_empty(), "rust community should have posts");
}

// ---------------------------------------------------------------------------
// HackerNews MCP tool tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hackernews_test_signin_and_list_accounts() {
    let srv = TestSrv::hackernews().await;
    let mut pool = BackendPool::new();

    let result = call(&mut pool, "test_signin", json!({
        "backend": "hackernews",
        "url": srv.base_url,
        "username": "anonymous"
    })).await;
    assert_ok(&result);

    let accounts = call(&mut pool, "list_accounts", json!({})).await;
    assert_ok(&accounts);
    let list: Vec<Value> = parse_text(&accounts);
    assert!(!list.is_empty(), "should have one guest account");
    assert!(
        list[0]["backend"].as_str().unwrap_or("").contains("hackernews"),
        "expected hackernews backend, got: {}", list[0]["backend"]
    );
}

#[tokio::test]
async fn hackernews_list_servers() {
    let srv = TestSrv::hackernews().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "hackernews", "url": srv.base_url, "username": "anonymous"
    })).await;

    let result = call(&mut pool, "list_servers", json!({ "backend": "hackernews" })).await;
    assert_ok(&result);
    let servers: Vec<Value> = parse_text(&result);
    assert_eq!(servers.len(), 1, "HN has exactly one virtual server");
    assert_eq!(servers[0]["name"].as_str().unwrap_or(""), "Hacker News");
}

#[tokio::test]
async fn hackernews_get_messages_top() {
    let srv = TestSrv::hackernews().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "hackernews", "url": srv.base_url, "username": "anonymous"
    })).await;

    let result = call(&mut pool, "get_messages", json!({
        "backend": "hackernews",
        "channel_id": "hn-top",
        "limit": 5
    })).await;
    assert_ok(&result);
    let msgs: Vec<Value> = parse_text(&result);
    assert!(!msgs.is_empty(), "top feed should have stories");
}

// ---------------------------------------------------------------------------
// Stoat MCP tool tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stoat_test_signin_and_list_accounts() {
    let srv = TestSrv::stoat().await;
    let mut pool = BackendPool::new();

    let result = call(&mut pool, "test_signin", json!({
        "backend": "stoat",
        "url": srv.base_url,
        "username": "stoat"
    })).await;
    assert_ok(&result);
    assert!(
        text_of(&result).contains("stoat") || text_of(&result).contains("Stoat"),
        "session should mention stoat user, got: {}",
        text_of(&result)
    );

    let accounts = call(&mut pool, "list_accounts", json!({})).await;
    assert_ok(&accounts);
    let list: Vec<Value> = parse_text(&accounts);
    assert!(!list.is_empty(), "should have one account");
    assert!(
        list[0]["backend"].as_str().unwrap_or("").contains("stoat"),
        "expected stoat backend, got: {}", list[0]["backend"]
    );
}

#[tokio::test]
async fn stoat_list_servers() {
    let srv = TestSrv::stoat().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "stoat", "url": srv.base_url, "username": "stoat"
    })).await;

    let result = call(&mut pool, "list_servers", json!({ "backend": "stoat" })).await;
    assert_ok(&result);
    let servers: Vec<Value> = parse_text(&result);
    assert!(!servers.is_empty(), "should have seeded servers");
    let names: Vec<&str> = servers.iter().filter_map(|s| s["name"].as_str()).collect();
    assert!(names.contains(&"The Burrow"), "expected 'The Burrow', got: {names:?}");
    assert!(names.contains(&"Midnight Dumpster"), "expected 'Midnight Dumpster', got: {names:?}");
}

#[tokio::test]
async fn stoat_list_channels() {
    let srv = TestSrv::stoat().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "stoat", "url": srv.base_url, "username": "stoat"
    })).await;

    let result = call(&mut pool, "list_channels", json!({
        "backend": "stoat",
        "server_id": "SRV001"
    })).await;
    assert_ok(&result);
    let channels: Vec<Value> = parse_text(&result);
    assert!(!channels.is_empty(), "SRV001 should have channels");
    let names: Vec<&str> = channels.iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(names.contains(&"general"), "expected 'general', got: {names:?}");
}

#[tokio::test]
async fn stoat_get_messages() {
    let srv = TestSrv::stoat().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "stoat", "url": srv.base_url, "username": "stoat"
    })).await;

    let result = call(&mut pool, "get_messages", json!({
        "backend": "stoat",
        "channel_id": "CH001",
        "limit": 5
    })).await;
    assert_ok(&result);
    let msgs: Vec<Value> = parse_text(&result);
    assert!(!msgs.is_empty(), "CH001 (general) should have seeded messages");
}

#[tokio::test]
async fn stoat_send_message() {
    let srv = TestSrv::stoat().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "stoat", "url": srv.base_url, "username": "stoat"
    })).await;

    let result = call(&mut pool, "send_message", json!({
        "backend": "stoat",
        "channel_id": "CH001",
        "text": "Hello from Stoat MCP test!"
    })).await;
    assert_ok(&result);
}

#[tokio::test]
async fn stoat_list_dms() {
    let srv = TestSrv::stoat().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "stoat", "url": srv.base_url, "username": "stoat"
    })).await;

    let result = call(&mut pool, "list_dms", json!({ "backend": "stoat" })).await;
    assert_ok(&result);
    let dms: Vec<Value> = parse_text(&result);
    assert!(!dms.is_empty(), "stoat user should have a DM with raccoon");
}

// ---------------------------------------------------------------------------
// Matrix MCP tool tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn matrix_test_signin_and_list_accounts() {
    let srv = TestSrv::matrix().await;
    let mut pool = BackendPool::new();

    let result = call(&mut pool, "test_signin", json!({
        "backend": "matrix",
        "url": srv.base_url,
        "username": "owl"
    })).await;
    assert_ok(&result);

    let accounts = call(&mut pool, "list_accounts", json!({})).await;
    assert_ok(&accounts);
    let list: Vec<Value> = parse_text(&accounts);
    assert!(!list.is_empty(), "should have one account");
    assert!(
        list[0]["backend"].as_str().unwrap_or("").contains("matrix"),
        "expected matrix backend, got: {}", list[0]["backend"]
    );
}

#[tokio::test]
async fn matrix_list_servers() {
    let srv = TestSrv::matrix().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "matrix", "url": srv.base_url, "username": "owl"
    })).await;

    let result = call(&mut pool, "list_servers", json!({ "backend": "matrix" })).await;
    assert_ok(&result);
    let servers: Vec<Value> = parse_text(&result);
    assert!(!servers.is_empty(), "should have seeded spaces");
    let names: Vec<&str> = servers.iter().filter_map(|s| s["name"].as_str()).collect();
    assert!(names.contains(&"The Hollow Tree"), "expected 'The Hollow Tree', got: {names:?}");
}

#[tokio::test]
async fn matrix_list_channels() {
    let srv = TestSrv::matrix().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "matrix", "url": srv.base_url, "username": "owl"
    })).await;

    let result = call(&mut pool, "list_channels", json!({
        "backend": "matrix",
        "server_id": "!space1:localhost"
    })).await;
    assert_ok(&result);
    let channels: Vec<Value> = parse_text(&result);
    assert!(!channels.is_empty(), "The Hollow Tree should have rooms");
    let names: Vec<&str> = channels.iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(names.contains(&"general"), "expected 'general', got: {names:?}");
}

#[tokio::test]
async fn matrix_get_messages() {
    let srv = TestSrv::matrix().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "matrix", "url": srv.base_url, "username": "owl"
    })).await;

    let result = call(&mut pool, "get_messages", json!({
        "backend": "matrix",
        "channel_id": "!general1:localhost",
        "limit": 5
    })).await;
    assert_ok(&result);
    let msgs: Vec<Value> = parse_text(&result);
    assert!(!msgs.is_empty(), "general1 should have seeded messages");
}

#[tokio::test]
async fn matrix_send_message() {
    let srv = TestSrv::matrix().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "matrix", "url": srv.base_url, "username": "owl"
    })).await;

    let result = call(&mut pool, "send_message", json!({
        "backend": "matrix",
        "channel_id": "!general1:localhost",
        "text": "Hello from Matrix MCP test!"
    })).await;
    assert_ok(&result);
}

// ---------------------------------------------------------------------------
// list_plugins — verify all compiled-in chat backends are reported
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_plugins_reports_all_compiled_backends() {
    let mut pool = BackendPool::new();
    let result = call(&mut pool, "list_plugins", json!({})).await;
    assert_ok(&result);

    let plugins: Vec<Value> = parse_text(&result);
    let ids: std::collections::HashSet<String> = plugins
        .iter()
        .filter_map(|p| p.get("id").and_then(|v| v.as_str()).map(String::from))
        .collect();

    // Every backend that's a workspace dependency of poly-chat-mcp must show up.
    for expected in [
        "stoat", "matrix", "discord", "teams", "lemmy",
        "hackernews", "github", "poly",
    ] {
        assert!(
            ids.contains(expected),
            "expected list_plugins to include {expected}, got {ids:?}"
        );
    }

    // Each entry has manifest fields populated (description is mandatory in the
    // builtin manifests; http_hosts/exec_programs are arrays).
    for p in &plugins {
        let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        assert!(
            p.get("name").and_then(|v| v.as_str()).is_some(),
            "{id} missing 'name'"
        );
        assert!(
            p.get("description").and_then(|v| v.as_str()).is_some(),
            "{id} missing 'description'"
        );
        assert!(
            p.get("http_hosts").and_then(|v| v.as_array()).is_some(),
            "{id} 'http_hosts' is not an array"
        );
        assert!(
            p.get("exec_programs").and_then(|v| v.as_array()).is_some(),
            "{id} 'exec_programs' is not an array"
        );
    }

    // Discord and Teams (the dev-only plugins from the user's question) must
    // both expose non-empty http_hosts so the manifest is actually informative.
    for dev_plugin in ["discord", "teams"] {
        let entry = plugins
            .iter()
            .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(dev_plugin))
            .unwrap_or_else(|| panic!("{dev_plugin} entry missing"));
        let hosts = entry.get("http_hosts").and_then(|v| v.as_array()).unwrap();
        assert!(!hosts.is_empty(), "{dev_plugin} should declare http_hosts");
    }
}

// ---------------------------------------------------------------------------
// WP-8 — list_plugin_tools + capability-gated NotSupported errors
// ---------------------------------------------------------------------------

fn parse_tool_names(result: &Value) -> Vec<String> {
    let text = text_of(result);
    let json_start = text.find('[').unwrap_or(0);
    serde_json::from_str(&text[json_start..]).expect("parse tool list as JSON")
}

#[tokio::test]
async fn list_plugin_tools_hackernews_omits_social_tools() {
    let mut pool = BackendPool::new();
    let result = call(&mut pool, "list_plugin_tools", json!({
        "backend": "hackernews"
    })).await;
    assert_ok(&result);
    let tools: Vec<String> = parse_tool_names(&result);
    assert!(tools.contains(&"list_servers".to_string()), "HN should still expose list_servers");
    assert!(tools.contains(&"get_messages".to_string()), "HN should still expose get_messages");
    assert!(!tools.contains(&"list_friends".to_string()), "HN has no friends model");
    assert!(!tools.contains(&"send_message".to_string()), "HN is read-only");
    assert!(!tools.contains(&"list_dms".to_string()), "HN has no DMs");
}

#[tokio::test]
async fn list_plugin_tools_discord_includes_all_social_tools() {
    let mut pool = BackendPool::new();
    let result = call(&mut pool, "list_plugin_tools", json!({
        "backend": "discord"
    })).await;
    assert_ok(&result);
    let tools: Vec<String> = parse_tool_names(&result);
    for expected in ["list_servers", "get_messages", "send_message", "list_dms", "list_friends", "list_notifications"] {
        assert!(tools.contains(&expected.to_string()), "discord should expose {expected}, got {tools:?}");
    }
}

#[tokio::test]
async fn list_plugin_tools_github_exposes_notifications_but_no_send() {
    let mut pool = BackendPool::new();
    let result = call(&mut pool, "list_plugin_tools", json!({
        "backend": "github"
    })).await;
    assert_ok(&result);
    let tools: Vec<String> = parse_tool_names(&result);
    assert!(tools.contains(&"list_notifications".to_string()), "github should expose list_notifications");
    assert!(!tools.contains(&"send_message".to_string()), "github is read-only");
    assert!(!tools.contains(&"list_friends".to_string()), "github has no friends model");
}

#[tokio::test]
async fn list_friends_on_hackernews_returns_not_supported_error() {
    let srv = TestSrv::hackernews().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "hackernews", "url": srv.base_url, "username": "anonymous"
    })).await;

    let result = call(&mut pool, "list_friends", json!({ "backend": "hackernews" })).await;
    assert_err(&result);
    let msg = text_of(&result).to_lowercase();
    assert!(
        msg.contains("not supported") || msg.contains("not_supported"),
        "expected a 'not supported' error message, got: {}", text_of(&result)
    );
}

#[tokio::test]
async fn send_message_on_hackernews_returns_not_supported_error() {
    let srv = TestSrv::hackernews().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "hackernews", "url": srv.base_url, "username": "anonymous"
    })).await;

    let result = call(&mut pool, "send_message", json!({
        "backend": "hackernews",
        "channel_id": "hn-top",
        "text": "should be rejected"
    })).await;
    assert_err(&result);
    let msg = text_of(&result).to_lowercase();
    assert!(
        msg.contains("not supported") || msg.contains("not_supported") || msg.contains("read-only") || msg.contains("read only"),
        "expected a 'not supported' or 'read-only' error message, got: {}", text_of(&result)
    );
}

// ---------------------------------------------------------------------------
// WP-8 — Client-provided UI surface via MCP
// ---------------------------------------------------------------------------

#[tokio::test]
async fn context_menu_tool_returns_plugin_items() {
    // Discord plugin declares `invite-people`, `privacy-settings` etc. on Server
    // targets. This exercises the MCP->ClientBackend round-trip for menus.
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": srv.base_url, "username": "koala"
    })).await;

    let result = call(&mut pool, "context_menu_server", json!({
        "backend": "discord",
        "target_id": "100"
    })).await;
    assert_ok(&result);
    let items: Vec<Value> = parse_text(&result);
    assert!(!items.is_empty(), "discord should declare server-target menu items");
    let ids: Vec<&str> = items.iter().filter_map(|i| i["id"].as_str()).collect();
    assert!(
        ids.contains(&"invite-people"),
        "expected 'invite-people' in discord server menu, got: {ids:?}"
    );
}

#[tokio::test]
async fn invoke_context_action_via_mcp_roundtrip() {
    // Discord's `invite-people` -> Ok(Noop). Round-trip asserts the MCP
    // handler invoked the plugin and serialized the outcome.
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": srv.base_url, "username": "koala"
    })).await;

    let result = call(&mut pool, "invoke_context_action", json!({
        "backend": "discord",
        "action_id": "invite-people",
        "target_kind": "server",
        "target_id": "100"
    })).await;
    assert_ok(&result);
    let outcome = text_of(&result);
    assert!(
        outcome.contains("Noop"),
        "expected Noop ActionOutcome, got: {outcome}"
    );
}

#[tokio::test]
async fn invoke_context_action_unknown_id_errors() {
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": srv.base_url, "username": "koala"
    })).await;

    let result = call(&mut pool, "invoke_context_action", json!({
        "backend": "discord",
        "action_id": "definitely-not-a-real-action",
        "target_kind": "server",
        "target_id": "100"
    })).await;
    assert_err(&result);
}

#[tokio::test]
async fn plugin_settings_sections_via_mcp() {
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": srv.base_url, "username": "koala"
    })).await;

    let result = call(&mut pool, "plugin_settings_sections", json!({
        "backend": "discord"
    })).await;
    assert_ok(&result);
    let sections: Vec<Value> = parse_text(&result);
    assert!(!sections.is_empty(), "discord should declare settings sections");
    // Discord declares a per-server 'profile' section; verify at least one section
    // has a section_key we recognize.
    let keys: Vec<&str> = sections.iter().filter_map(|s| s["section_key"].as_str()).collect();
    assert!(
        keys.iter().any(|k| *k == "profile" || *k == "notification-rules" || *k == "privacy"),
        "expected one of the discord-declared section keys, got: {keys:?}"
    );
}

#[tokio::test]
async fn plugin_setting_get_returns_default() {
    // Discord `get_setting_value` falls back to the declared default when no
    // kv is wired. We don't care about the exact value — just that the
    // MCP path round-trips to the backend.
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": srv.base_url, "username": "koala"
    })).await;

    let result = call(&mut pool, "plugin_setting_get", json!({
        "backend": "discord",
        "scope": "per-server",
        "scope_id": "100",
        "key": "mentions-only"
    })).await;
    assert_ok(&result);
}

#[tokio::test]
async fn sidebar_declaration_via_mcp() {
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": srv.base_url, "username": "koala"
    })).await;

    let result = call(&mut pool, "sidebar_declaration", json!({
        "backend": "discord"
    })).await;
    assert_ok(&result);
    let text = text_of(&result);
    assert!(
        text.contains("layout"),
        "sidebar declaration should have a 'layout' field, got: {text}"
    );
}

#[tokio::test]
async fn composer_buttons_via_mcp() {
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": srv.base_url, "username": "koala"
    })).await;

    let result = call(&mut pool, "composer_buttons", json!({
        "backend": "discord",
        "channel_id": "200"
    })).await;
    assert_ok(&result);
    let btns: Vec<Value> = parse_text(&result);
    // Discord declares a stickers button.
    assert!(!btns.is_empty(), "discord declares at least one composer button");
    let ids: Vec<&str> = btns.iter().filter_map(|b| b["id"].as_str()).collect();
    assert!(ids.contains(&"stickers"), "expected 'stickers' button, got: {ids:?}");
}

#[tokio::test]
async fn message_actions_and_invoke_via_mcp() {
    let srv = TestSrv::discord().await;
    let mut pool = BackendPool::new();
    call(&mut pool, "test_signin", json!({
        "backend": "discord", "url": srv.base_url, "username": "koala"
    })).await;

    let list = call(&mut pool, "message_actions", json!({
        "backend": "discord",
        "channel_id": "200",
        "message_id": "m1"
    })).await;
    assert_ok(&list);
    let items: Vec<Value> = parse_text(&list);
    assert!(!items.is_empty());
    let ids: Vec<&str> = items.iter().filter_map(|i| i["id"].as_str()).collect();
    assert!(ids.contains(&"pin-message"), "expected 'pin-message' in list, got: {ids:?}");

    let invoke = call(&mut pool, "invoke_message_action", json!({
        "backend": "discord",
        "action_id": "pin-message",
        "channel_id": "200",
        "message_id": "m1"
    })).await;
    assert_ok(&invoke);
    assert!(text_of(&invoke).contains("Noop"));
}

#[tokio::test]
async fn mcp_tools_new_surfaces_are_queryable() {
    // Meta test: every new WP-8 tool name is registered in `tool_list()` so
    // MCP `tools/list` advertises them to clients.
    let names: std::collections::HashSet<String> = poly_chat_mcp::tools::tool_list()
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
        .collect();

    for expected in [
        "context_menu_server",
        "context_menu_channel",
        "context_menu_user",
        "context_menu_message",
        "context_menu_dm",
        "context_menu_category",
        "invoke_context_action",
        "plugin_settings_sections",
        "plugin_setting_get",
        "plugin_setting_set",
        "sidebar_declaration",
        "invoke_sidebar_action",
        "channel_view",
        "view_rows",
        "composer_buttons",
        "message_actions",
        "invoke_composer_action",
        "invoke_message_action",
        // Phase C tools
        "poll_events",
        "subscribe_events",
        "unsubscribe_events",
    ] {
        assert!(
            names.contains(expected),
            "tool '{expected}' missing from tool_list(); have: {names:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Phase B — Draft queue integration tests (B.8)
// ---------------------------------------------------------------------------

/// Helper: open an in-memory MemoryDb for draft tests.
fn draft_mem() -> MemoryDb {
    MemoryDb::open(":memory:").expect("in-memory MemoryDb for drafts")
}

#[tokio::test]
async fn draft_create_then_list_returns_pending() {
    let mut pool = BackendPool::new();
    let mem = draft_mem();

    // Create a draft (no backend needed — draft_create only needs account_id, chat_id, body, suggested_by).
    let result = tools::dispatch(
        "draft_create",
        &json!({
            "account_id":   "test-account",
            "chat_id":      "test-chat",
            "body":         "Hello from test-agent",
            "suggested_by": "test-agent"
        }),
        &mut pool,
        &mem,
    ).await;
    assert_ok(&result);
    let text = text_of(&result);
    let json: Value = serde_json::from_str(&text).expect("draft_create should return JSON");
    let draft_id = json["draft_id"].as_i64().expect("draft_id should be i64");
    assert!(draft_id > 0);

    // draft_list should return 1 pending draft.
    let list_result = tools::dispatch(
        "draft_list",
        &json!({
            "account_id": "test-account",
            "chat_id":    "test-chat",
            "status":     "pending"
        }),
        &mut pool,
        &mem,
    ).await;
    assert_ok(&list_result);
    let drafts: Vec<Value> = parse_text(&list_result);
    assert_eq!(drafts.len(), 1, "expected 1 pending draft");
    assert_eq!(drafts[0]["body"], "Hello from test-agent");
    assert_eq!(drafts[0]["suggested_by"], "test-agent");
    assert_eq!(drafts[0]["status"], "pending");
}

#[tokio::test]
async fn draft_edit_updates_body() {
    let mut pool = BackendPool::new();
    let mem = draft_mem();

    let create = tools::dispatch("draft_create", &json!({
        "account_id": "acc1", "chat_id": "chat1",
        "body": "Original body", "suggested_by": "bot"
    }), &mut pool, &mem).await;
    assert_ok(&create);
    let draft_id: i64 = serde_json::from_str::<Value>(&text_of(&create)).unwrap()["draft_id"].as_i64().unwrap();

    let edit = tools::dispatch("draft_edit", &json!({
        "draft_id": draft_id,
        "new_body": "Updated body"
    }), &mut pool, &mem).await;
    assert_ok(&edit);
    assert!(text_of(&edit).contains("updated"), "expected 'updated' in response");

    // Verify via draft_list.
    let list = tools::dispatch("draft_list", &json!({
        "account_id": "acc1", "chat_id": "chat1"
    }), &mut pool, &mem).await;
    let drafts: Vec<Value> = parse_text(&list);
    assert_eq!(drafts[0]["body"], "Updated body");
}

#[tokio::test]
async fn draft_discard_removes_from_pending() {
    let mut pool = BackendPool::new();
    let mem = draft_mem();

    let create = tools::dispatch("draft_create", &json!({
        "account_id": "acc1", "chat_id": "chat1",
        "body": "to discard", "suggested_by": "bot"
    }), &mut pool, &mem).await;
    let draft_id: i64 = serde_json::from_str::<Value>(&text_of(&create)).unwrap()["draft_id"].as_i64().unwrap();

    let discard = tools::dispatch("draft_discard", &json!({ "draft_id": draft_id }), &mut pool, &mem).await;
    assert_ok(&discard);

    let list = tools::dispatch("draft_list", &json!({
        "account_id": "acc1", "chat_id": "chat1", "status": "pending"
    }), &mut pool, &mem).await;
    let pending: Vec<Value> = parse_text(&list);
    assert!(pending.is_empty(), "discarded draft should not appear in pending list");
}

#[tokio::test]
async fn draft_cancel_autosend_clears_timer() {
    let mut pool = BackendPool::new();
    let mem = draft_mem();

    // Manually insert a draft with auto_send_at via the MemoryDb API.
    let draft_id = mem.draft_insert(
        "acc1", "chat1", "body", "bot",
        Some("2090-01-01T00:00:00Z")
    ).expect("insert draft");

    let cancel = tools::dispatch(
        "draft_cancel_autosend",
        &json!({ "draft_id": draft_id }),
        &mut pool,
        &mem,
    ).await;
    assert_ok(&cancel);

    let draft = mem.draft_get(draft_id).expect("get").expect("found");
    assert!(draft["auto_send_at"].is_null(), "auto_send_at should be null after cancel");
}

#[tokio::test]
async fn draft_create_empty_body_returns_error() {
    let mut pool = BackendPool::new();
    let mem = draft_mem();

    let result = tools::dispatch("draft_create", &json!({
        "account_id": "acc1", "chat_id": "chat1",
        "body": "   ", "suggested_by": "bot"
    }), &mut pool, &mem).await;
    assert_err(&result);
}

// ---------------------------------------------------------------------------
// Phase C — event subscription and poll_events integration tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn phase_c_poll_events_empty_on_fresh_pool() {
    let mut pool = BackendPool::new();

    let result = call(&mut pool, "poll_events", json!({
        "since_ms": 0,
        "limit": 100
    })).await;
    assert_ok(&result);

    let text = text_of(&result);
    let parsed: serde_json::Value = serde_json::from_str(
        &text[text.find('{').unwrap_or(0)..]
    ).expect("poll_events response is JSON");

    assert_eq!(
        parsed["count"].as_u64().unwrap_or(1),
        0,
        "fresh pool should have no events"
    );
}

#[tokio::test]
async fn phase_c_subscribe_and_unsubscribe() {
    let mut pool = BackendPool::new();

    // subscribe
    let sub_result = call(&mut pool, "subscribe_events", json!({
        "event_types": ["message_received", "typing_started"]
    })).await;
    assert_ok(&sub_result);

    let text = text_of(&sub_result);
    let parsed: serde_json::Value = serde_json::from_str(
        &text[text.find('{').unwrap_or(0)..]
    ).expect("subscribe_events returns JSON");

    let sub_id = parsed["subscription_id"].as_str().expect("has subscription_id").to_string();
    assert!(!sub_id.is_empty());

    // poll with the subscription id — should return no events yet
    let poll_result = call(&mut pool, "poll_events", json!({
        "since_ms": 0,
        "subscription_id": sub_id
    })).await;
    assert_ok(&poll_result);

    // unsubscribe
    let unsub_result = call(&mut pool, "unsubscribe_events", json!({
        "subscription_id": sub_id
    })).await;
    assert_ok(&unsub_result);

    // poll after unsubscribe should error (unknown subscription)
    let poll_after = call(&mut pool, "poll_events", json!({
        "since_ms": 0,
        "subscription_id": sub_id
    })).await;
    assert_err(&poll_after);
}

/// C.6 Integration test — subscribe → push a message via testhook → poll_events sees it within 2s.
///
/// Uses `BackendPool::insert()` directly (bypassing `test_signin`) so the
/// discord client is constructed with `with_base_url_and_gateway`, which
/// activates the WebSocket gateway event stream. The test server broadcasts
/// a `MESSAGE_CREATE` gateway frame when `POST /api/v10/channels/{id}/messages`
/// is called. The Phase C fan-out task picks it up and stores it in EventStore.
#[tokio::test]
async fn phase_c_discord_message_received_via_poll_events() {
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use poly_test_discord::{DiscordState, router as discord_router};
    use poly_discord::DiscordClient;
    use poly_client::{ClientBackend, AuthCredentials};

    // Spin up the test discord server (gateway WebSocket included).
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().unwrap().port();
    let state = Arc::new(DiscordState::new());
    state.seed();
    // Tell the test server which WS URL to advertise as the gateway.
    {
        let mut gw = state.gateway_url.write().await;
        *gw = format!("ws://127.0.0.1:{port}/gateway/ws");
    }
    let app = discord_router(state.clone());
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async { shutdown_rx.await.ok(); })
            .await
            .ok();
    });

    let base_url = format!("http://127.0.0.1:{port}");
    let ws_url = format!("ws://127.0.0.1:{port}/gateway/ws");

    // Get an auth token from the test server.
    let http_client = reqwest::Client::new();
    let token_resp = http_client
        .post(format!("{base_url}/test/auth/token"))
        .json(&serde_json::json!({ "username": "koala" }))
        .send()
        .await
        .expect("token request");
    let token_body: serde_json::Value = token_resp.json().await.expect("token body");
    let token = token_body["token"].as_str().expect("token field").to_string();

    // Build the discord client with gateway support.
    let mut discord = DiscordClient::with_base_url_and_gateway(base_url.clone(), ws_url);
    let session = discord
        .authenticate(AuthCredentials::Token(token.clone()))
        .await
        .expect("authenticate");

    // Insert into pool — this starts the fan-out task.
    let mut pool = BackendPool::new();
    pool.insert(session, Box::new(discord));

    // Give the WebSocket connection a moment to complete the IDENTIFY handshake.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Register a subscription for message_received events.
    let sub_resp = call(&mut pool, "subscribe_events", json!({
        "event_types": ["message_received"]
    })).await;
    assert_ok(&sub_resp);
    let sub_text = text_of(&sub_resp);
    let sub_json: serde_json::Value = serde_json::from_str(
        &sub_text[sub_text.find('{').unwrap_or(0)..]
    ).unwrap();
    let sub_id = sub_json["subscription_id"].as_str().unwrap().to_string();

    // Record cursor before sending.
    let before_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        - 1;

    // Push a message into channel 200 via HTTP. The test server now broadcasts
    // a MESSAGE_CREATE gateway event to connected WS clients on this call.
    let send_resp = http_client
        .post(format!("{base_url}/api/v10/channels/200/messages"))
        .header("authorization", &token)
        .json(&serde_json::json!({ "content": "hello from phase C test!" }))
        .send()
        .await
        .expect("send message request");
    assert!(send_resp.status().is_success(), "send_message should succeed");

    // Allow up to 2 seconds for the fan-out task to receive the gateway event.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut found = false;
    while tokio::time::Instant::now() < deadline {
        let poll = call(&mut pool, "poll_events", json!({
            "since_ms": before_ms,
            "subscription_id": sub_id
        })).await;
        assert_ok(&poll);

        let poll_text = text_of(&poll);
        let poll_json: serde_json::Value = serde_json::from_str(
            &poll_text[poll_text.find('{').unwrap_or(0)..]
        ).unwrap();

        if poll_json["count"].as_u64().unwrap_or(0) > 0 {
            found = true;
            let events = poll_json["events"].as_array().unwrap();
            assert!(
                events.iter().any(|e| {
                    let kind = e["kind"].as_str().unwrap_or("");
                    kind == "message_received" || kind.contains("MessageReceived")
                }),
                "expected message_received event, got: {poll_json}"
            );
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    assert!(
        found,
        "poll_events should have received a message_received event within 2s after sending"
    );

    let _ = shutdown_tx.send(());
}
