//! Phase E integration test for the talk-to overlay MCP wire.
//!
//! Spins up `poly_test_discord` in-process, registers a Discord backend,
//! creates a persona with a Discord channel source, invokes via the same
//! `tools::dispatch` path the UI uses, and asserts the bundle:
//! - has `bundle_version: "v1"`
//! - contains the seeded channel with messages
//!
//! This is the Rust-side integration (no Playwright/UI driving).
//! It proves the full wire from overlay "Send" → `meta_persona_invoke` → bundle.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;

use poly_client::{
    IsBackend, AuthCredentials,
};
use poly_chat_mcp::memory::MemoryDb;
use poly_chat_mcp::state::BackendPool;
use poly_chat_mcp::tools;
use poly_test_discord::{DiscordState, router as discord_router};
use poly_discord::DiscordClient;

use serde_json::{Value, json};
use tokio::net::TcpListener;

// ── Helper: spin up a test Discord server ────────────────────────────────────

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

// ── Tool call helpers ─────────────────────────────────────────────────────────

fn test_mem() -> MemoryDb {
    MemoryDb::open(":memory:").expect("in-memory MemoryDb")
}

async fn call(pool: &mut BackendPool, mem: &MemoryDb, tool: &str, args: Value) -> Value {
    tools::dispatch(tool, &args, pool, mem).await
}

fn assert_ok(result: &Value) {
    assert!(
        !result.get("isError").and_then(serde_json::Value::as_bool).unwrap_or(false),
        "tool returned error: {result}"
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

// ── Phase E integration test ──────────────────────────────────────────────────

/// Simulates the UI overlay "Send" flow:
///   create persona → bind Discord channel source → invoke → assert bundle v1.
///
/// This mirrors exactly what `invoke_and_append` in `talk_to_overlay.rs` does:
/// it calls `meta_persona_invoke` with `slug` + `user_prompt` + `include_summaries`.
#[tokio::test]
async fn persona_talk_e2e_overlay_send_flow() {
    // ── 1. Start the Discord test server (seeded with #general channel 200).
    let srv = TestSrv::discord().await;

    // ── 2. Authenticate and register a Discord backend.
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

    // ── 3. Set up the in-memory DB.
    let mem = test_mem();

    // ── 4. Create a persona.
    let create = call(&mut pool, &mem, "meta_persona_create", json!({
        "slug": "talk-e2e-persona",
        "name": "Talk E2E Persona",
        "system_prompt": "You are a test persona for Phase E overlay integration.",
    })).await;
    assert_ok(&create);

    // ── 5. Bind channel 200 (#general) as a source.
    //
    // The seed data places 3 messages in channel 200:
    //   "G'day everyone!", "Crikey, it's good to be here!", "Has anyone seen my joey?"
    let set_src = call(&mut pool, &mem, "meta_persona_set_sources", json!({
        "slug": "talk-e2e-persona",
        "sources": [
            {
                "account_id": account_id,
                "selector_kind": "channel",
                "selector_value": "200",
                "include": true
            }
        ]
    })).await;
    assert_ok(&set_src);

    // ── 6. Invoke — mirroring the payload `invoke_and_append` sends.
    let invoke = call(&mut pool, &mem, "meta_persona_invoke", json!({
        "slug": "talk-e2e-persona",
        "user_prompt": "What's been happening?",
        "include_summaries": true
    })).await;
    assert_ok(&invoke);

    // ── 7. Assert the returned bundle.
    let text = text_of(&invoke);
    let json_start = text.find('{').unwrap_or(0);
    let bundle: Value = serde_json::from_str(text.get(json_start..).unwrap_or(""))
        .unwrap_or_else(|e| panic!("bundle not valid JSON: {e}\nraw: {text}"));

    // bundle_version must be "v1".
    assert_eq!(
        bundle["bundle_version"].as_str().unwrap_or(""),
        "v1",
        "expected bundle_version v1, got: {bundle}"
    );

    // Persona header present.
    assert_eq!(
        bundle["persona"]["slug"].as_str().unwrap_or(""),
        "talk-e2e-persona"
    );

    // system_prompt carried through.
    assert!(
        bundle["system_prompt"]
            .as_str()
            .unwrap_or("")
            .contains("Phase E overlay integration"),
        "system_prompt missing in bundle"
    );

    // user_prompt echoed.
    assert_eq!(
        bundle["user_prompt"].as_str().unwrap_or(""),
        "What's been happening?"
    );

    // chats non-empty: channel 200 bound.
    let chats = bundle["chats"].as_array().expect("chats array");
    assert!(!chats.is_empty(), "expected at least one chat in bundle");

    // Channel 200 present with seeded messages.
    let chat_200 = chats
        .iter()
        .find(|c| c["chat_id"].as_str().unwrap_or("") == "200")
        .unwrap_or_else(|| panic!("channel 200 not found in chats: {bundle}"));

    let msgs = chat_200["recent_messages"].as_array().expect("recent_messages");
    assert!(!msgs.is_empty(), "expected seeded messages in channel 200");

    let texts: Vec<&str> = msgs.iter()
        .filter_map(|m| m["text"].as_str())
        .collect();
    let has_seeded = texts.iter().any(|t| {
        t.contains("G'day") || t.contains("Crikey") || t.contains("joey")
    });
    assert!(
        has_seeded,
        "seeded messages not found in bundle. Messages: {texts:?}"
    );

    // ── 8. Verify audit rows were written (memory_read per chat read).
    let audit = mem.list_persona_audit("talk-e2e-persona", 50).expect("list audit");
    let memory_reads: Vec<&Value> = audit
        .iter()
        .filter(|r| r["action"].as_str() == Some("memory_read"))
        .collect();
    assert!(
        !memory_reads.is_empty(),
        "expected memory_read audit rows, got: {audit:?}"
    );
}
