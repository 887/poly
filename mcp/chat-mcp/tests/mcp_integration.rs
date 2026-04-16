//! End-to-end MCP tool integration tests.
//!
//! Spins up Discord + Teams + Lemmy + HackerNews + Stoat + Matrix test servers
//! in-process, then exercises the `poly-chat-mcp` tool dispatch layer end-to-end.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;

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

async fn call(pool: &mut BackendPool, tool: &str, args: Value) -> Value {
    tools::dispatch(tool, &args, pool).await
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
