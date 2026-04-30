//! Phase F end-to-end integration test for the heartbeat scheduler.
//!
//! Spins up `poly_test_discord` in-process, creates a persona with
//! `heartbeat_seconds=2` and `proactivity=drafts-only`, binds channel 200
//! as a source (which has 3 seeded messages including a question), starts
//! the heartbeat, waits for 1 tick (~3 s), then asserts that at least one
//! `heartbeat_run` audit row AND at least one `draft_create` audit row
//! appeared in the DB.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;

use poly_chat_mcp::memory::MemoryDb;
use poly_chat_mcp::persona::heartbeat::HeartbeatRegistry;
use poly_chat_mcp::state::BackendPool;
use poly_chat_mcp::tools;
use poly_discord::DiscordClient;
use poly_client::{ClientBackend, AuthCredentials};
use poly_test_discord::{DiscordState, router as discord_router};
use serde_json::{Value, json};
use tokio::net::TcpListener;

// ── Helper: spin up the Discord test server ──────────────────────────────────

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
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        Self { base_url: format!("http://127.0.0.1:{port}"), _shutdown: tx }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn test_mem() -> MemoryDb {
    MemoryDb::open(":memory:").expect("in-memory MemoryDb")
}

async fn call(pool: &mut BackendPool, mem: &MemoryDb, tool: &str, args: Value) -> Value {
    tools::dispatch(tool, &args, pool, mem).await
}

fn assert_ok(result: &Value) {
    assert!(
        !result.get("isError").and_then(|e| e.as_bool()).unwrap_or(false),
        "tool returned error: {result}"
    );
}

// ── The integration test ─────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn persona_heartbeat_e2e_creates_audit_and_draft() {
    // 1. Start the Discord test server.
    let srv = TestSrv::discord().await;

    // 2. Authenticate a DiscordClient.
    let http_client = reqwest::Client::new();
    let token_resp = http_client
        .post(format!("{}/test/auth/token", srv.base_url))
        .json(&json!({ "username": "koala" }))
        .send()
        .await
        .expect("token request");
    let token_body: Value = token_resp.json().await.expect("token body");
    let token = token_body["token"].as_str().expect("token field").to_string();

    let mut discord = DiscordClient::with_base_url(srv.base_url.clone());
    let session = discord
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");
    let account_id = session.user.id.clone();

    let mut pool = BackendPool::new();
    pool.insert(session, Box::new(discord));

    // 3. Create persona with heartbeat_seconds=2, proactivity=drafts-only.
    let mem = test_mem();

    let create = call(&mut pool, &mem, "meta_persona_create", json!({
        "slug": "hb-test",
        "name": "HB Test",
        "system_prompt": "You are a heartbeat test persona.",
        "heartbeat_interval_secs": 2,
        "proactivity": "drafts-only",
    })).await;
    assert_ok(&create);

    // 4. Bind channel 200 (#general in guild 100) — has seeded messages
    //    including "Has anyone seen my joey?" which contains '?'.
    let set_src = call(&mut pool, &mem, "meta_persona_set_sources", json!({
        "slug": "hb-test",
        "sources": [{
            "account_id": account_id,
            "selector_kind": "channel",
            "selector_value": "200",
            "include": true
        }]
    })).await;
    assert_ok(&set_src);

    // 5. Start the heartbeat using a test-helper path (direct API, not MCP tool).
    //    We build a BackendPoolProvider and wrap it in Arc.
    // BackendPoolProvider borrows pool — cannot pass to an async task that
    // outlives this frame. We use OwningProvider (below) instead.
    // an async task that outlives this function.  Instead, we use the
    // HeartbeatRegistry's `start` method with a real provider, which is
    // the load-bearing production code path.
    //
    // To make the borrow work, we clone the pool into a standalone provider
    // via an adapter shim that owns the pool, mirroring how the real host
    // would do it.
    //
    // OwningProvider — test helper that holds its own BackendPool clone.
    struct OwningProvider {
        pool: BackendPool,
    }

    #[async_trait::async_trait]
    impl poly_chat_mcp::persona::context::PersonaBackendProvider for OwningProvider {
        fn account_ids(&self) -> Vec<String> {
            self.pool
                .list_accounts()
                .into_iter()
                .filter_map(|v| v.get("user_id").and_then(|u| u.as_str()).map(|s| s.to_string()))
                .collect()
        }

        async fn list_servers(&self, account_id: &str) -> anyhow::Result<Vec<(String, String)>> {
            let backend = self
                .pool
                .find_by_account(account_id)
                .map(|e| std::sync::Arc::clone(&e.backend))
                .ok_or_else(|| anyhow::anyhow!("no backend for {account_id}"))?;
            let servers = backend.get_servers().await?;
            Ok(servers.into_iter().map(|s| (s.id.to_string(), s.name)).collect())
        }

        async fn list_channels(&self, account_id: &str, server_id: &str) -> anyhow::Result<Vec<(String, String)>> {
            let backend = self
                .pool
                .find_by_account(account_id)
                .map(|e| std::sync::Arc::clone(&e.backend))
                .ok_or_else(|| anyhow::anyhow!("no backend for {account_id}"))?;
            let channels = backend.get_channels(server_id).await?;
            Ok(channels.into_iter().map(|c| (c.id.to_string(), c.name)).collect())
        }

        async fn list_dms(&self, account_id: &str) -> anyhow::Result<Vec<(String, String)>> {
            let backend = self
                .pool
                .find_by_account(account_id)
                .map(|e| std::sync::Arc::clone(&e.backend))
                .ok_or_else(|| anyhow::anyhow!("no backend for {account_id}"))?;
            let dms = backend.get_dm_channels().await?;
            Ok(dms.into_iter().map(|d| (d.id.to_string(), d.user.display_name)).collect())
        }

        async fn fetch_messages(&self, account_id: &str, chat_id: &str, limit: usize) -> anyhow::Result<Vec<poly_chat_mcp::persona::context::MessageBrief>> {
            let backend = self
                .pool
                .find_by_account(account_id)
                .map(|e| std::sync::Arc::clone(&e.backend))
                .ok_or_else(|| anyhow::anyhow!("no backend for {account_id}"))?;
            let query = poly_client::MessageQuery { limit: Some(limit as u32), ..Default::default() };
            let messages = backend.get_messages(chat_id, query).await?;
            Ok(messages.into_iter().map(|m| poly_chat_mcp::persona::context::MessageBrief {
                from: m.author.display_name,
                ts: m.timestamp.to_rfc3339(),
                text: match m.content {
                    poly_client::MessageContent::Text(t) => t,
                    _ => String::new(),
                },
            }).collect())
        }
    }

    // Rebuild pool as owned (pool.insert uses &mut self and we can't clone the
    // Box<dyn ClientBackend>, so we make a fresh DiscordClient for the provider).
    let http_client2 = reqwest::Client::new();
    let token_resp2 = http_client2
        .post(format!("{}/test/auth/token", srv.base_url))
        .json(&json!({ "username": "koala" }))
        .send()
        .await
        .expect("token request 2");
    let token_body2: Value = token_resp2.json().await.expect("token body 2");
    let token2 = token_body2["token"].as_str().expect("token2").to_string();

    let mut discord2 = DiscordClient::with_base_url(srv.base_url.clone());
    let session2 = discord2
        .authenticate(AuthCredentials::Token(token2))
        .await
        .expect("authenticate 2");

    let mut owned_pool = BackendPool::new();
    owned_pool.insert(session2, Box::new(discord2));

    let provider = Arc::new(OwningProvider { pool: owned_pool });

    // 6. Start heartbeat with 2-second interval, last_run_at=None → fires immediately.
    let mut registry = HeartbeatRegistry::new();
    registry.start("hb-test", 2, None, mem.clone(), provider);

    // 7. Wait long enough for at least one tick to complete (~3 s).
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // 8. Tear down.
    registry.stop("hb-test");

    // 9. Assertions.
    let audit = mem.list_persona_audit("hb-test", 50).expect("list audit");

    let heartbeat_run_rows: Vec<&Value> = audit
        .iter()
        .filter(|r| r.get("action").and_then(|a| a.as_str()) == Some("heartbeat_run"))
        .collect();
    assert!(
        !heartbeat_run_rows.is_empty(),
        "expected ≥1 heartbeat_run audit row; got {} total rows: {audit:?}",
        audit.len()
    );

    // With proactivity=drafts-only, draft_create rows should appear because
    // channel 200 has "Has anyone seen my joey?" (contains '?').
    let draft_create_rows: Vec<&Value> = audit
        .iter()
        .filter(|r| r.get("action").and_then(|a| a.as_str()) == Some("draft_create"))
        .collect();
    assert!(
        !draft_create_rows.is_empty(),
        "expected ≥1 draft_create audit row; got {} total rows: {audit:?}",
        audit.len()
    );

    // The draft body should reference the question message.
    let drafts = mem.draft_list(None, Some("200"), None).expect("draft_list");
    assert!(
        !drafts.is_empty(),
        "expected ≥1 draft in the drafts table for channel 200"
    );
    let draft_body = drafts[0]["body"].as_str().unwrap_or("");
    assert!(
        draft_body.contains("joey") || draft_body.contains("?"),
        "draft body should reference the seeded question message; got: {draft_body}"
    );
}
