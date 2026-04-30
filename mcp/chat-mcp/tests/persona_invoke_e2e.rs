//! Phase C end-to-end integration test for `meta_persona_invoke`.
//!
//! Spins up `poly_test_discord` in-process, registers a real `DiscordClient`
//! against it, creates a persona with a Discord channel source, calls the tool
//! dispatcher, and asserts the returned bundle:
//! - is `bundle_version: "v1"`
//! - contains at least one chat with the seeded messages (G'day / Crikey / joey)
//!
//! The test follows the same pattern as `mcp_integration.rs`'s
//! `phase_c_discord_message_received_via_poll_events`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;

use poly_chat_mcp::memory::MemoryDb;
use poly_chat_mcp::state::BackendPool;
use poly_chat_mcp::tools;
use poly_test_discord::{DiscordState, router as discord_router};
use poly_discord::DiscordClient;
use poly_client::{ClientBackend, AuthCredentials};
use serde_json::{Value, json};
use tokio::net::TcpListener;

// ── Helper to spin up a Discord test server ─────────────────────────────────

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
        // Brief sleep to let the server bind.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        Self { base_url: format!("http://127.0.0.1:{port}"), _shutdown: tx }
    }
}

// ── Tool call helpers ────────────────────────────────────────────────────────

fn test_mem() -> MemoryDb {
    MemoryDb::open(":memory:").expect("in-memory MemoryDb")
}

async fn call(pool: &mut BackendPool, mem: &MemoryDb, tool: &str, args: Value) -> Value {
    tools::dispatch(tool, &args, pool, mem).await
}

fn assert_ok(result: &Value) {
    assert!(
        !result.get("isError").and_then(|e| e.as_bool()).unwrap_or(false),
        "tool returned error: {}",
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

// ── The integration test ─────────────────────────────────────────────────────

#[tokio::test]
async fn persona_invoke_e2e_discord_bundle_v1() {
    // ── 1. Start the Discord test server.
    let srv = TestSrv::discord().await;

    // ── 2. Authenticate a DiscordClient and insert it into a BackendPool.
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

    // ── 3. Create the in-memory DB and a persona with channel 200 as a source.
    //
    // The seed data places 3 messages in channel 200 (#general):
    //   "G'day everyone!", "Crikey, it's good to be here!", "Has anyone seen my joey?"
    let mem = test_mem();
    let create_res = call(&mut pool, &mem, "meta_persona_invoke", json!({ "slug": "not-yet" })).await;
    // Persona doesn't exist yet — expect an error.
    assert!(
        create_res.get("isError").and_then(|e| e.as_bool()).unwrap_or(false),
        "expected error for missing persona"
    );

    // Create the persona.
    let create = call(&mut pool, &mem, "meta_persona_create", json!({
        "slug": "test-watcher",
        "name": "Test Watcher",
        "system_prompt": "You watch the Discord server and report back.",
    })).await;
    assert_ok(&create);

    // Bind channel 200 (#general in guild 100) as a source.
    let set_src = call(&mut pool, &mem, "meta_persona_set_sources", json!({
        "slug": "test-watcher",
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

    // ── 4. Invoke the persona and inspect the bundle.
    let invoke = call(&mut pool, &mem, "meta_persona_invoke", json!({
        "slug": "test-watcher",
        "user_prompt": "What's the latest?",
        "include_summaries": false
    })).await;
    assert_ok(&invoke);

    let text = text_of(&invoke);
    let json_start = text.find('{').unwrap_or(0);
    let bundle: Value = serde_json::from_str(&text[json_start..])
        .unwrap_or_else(|e| panic!("bundle not valid JSON: {e}\nraw: {text}"));

    // bundle_version must be "v1".
    assert_eq!(
        bundle["bundle_version"].as_str().unwrap_or(""),
        "v1",
        "expected bundle_version v1, got: {bundle}"
    );

    // The persona header must be present.
    assert_eq!(
        bundle["persona"]["slug"].as_str().unwrap_or(""),
        "test-watcher"
    );

    // chats must be non-empty (we bound channel 200 which has 3 seeded messages).
    let chats = bundle["chats"].as_array().expect("chats array");
    assert!(!chats.is_empty(), "expected at least one chat in bundle");

    // Channel 200 should appear.
    let chat_200 = chats
        .iter()
        .find(|c| c["chat_id"].as_str().unwrap_or("") == "200")
        .unwrap_or_else(|| panic!("channel 200 not found in chats: {bundle}"));

    // At least one of the seeded messages must appear.
    let msgs = chat_200["recent_messages"].as_array().expect("recent_messages array");
    assert!(!msgs.is_empty(), "expected recent messages in channel 200");

    let texts: Vec<&str> = msgs.iter()
        .filter_map(|m| m["text"].as_str())
        .collect();
    let has_seeded = texts.iter().any(|t| {
        t.contains("G'day") || t.contains("Crikey") || t.contains("joey")
    });
    assert!(
        has_seeded,
        "seeded messages (G'day / Crikey / joey) not found in bundle. Messages: {texts:?}"
    );

    // ── 5. Verify audit rows were written.
    let audit = mem.list_persona_audit("test-watcher", 50).expect("list audit");
    assert!(!audit.is_empty(), "no audit rows written");

    // At least one memory_read row for the account + channel.
    let memory_reads: Vec<&Value> = audit
        .iter()
        .filter(|r| r["action"].as_str() == Some("memory_read"))
        .collect();
    assert!(
        !memory_reads.is_empty(),
        "expected memory_read audit rows, got: {audit:?}"
    );
}

// ── dry_run integration test ─────────────────────────────────────────────────

/// Verify that `meta_persona_invoke` with `dry_run=true`:
///   1. Returns a valid `bundle_v1` (same shape as a normal invocation).
///   2. Sets `dry_run: true` at the top level of the bundle JSON.
///   3. Writes exactly 1 new audit row (the user-initiated `invoke` row),
///      NOT 1 + N (invoke + one `memory_read` per chat).
///
/// This test runs against the same Discord test server as the non-dry-run
/// test — channel 200 has 3 seeded messages, so a normal invoke would write
/// 1 invoke row + 1 memory_read row = 2 rows.  With dry_run=true we expect
/// exactly 1 new row (only the invoke row).
#[tokio::test]
async fn persona_invoke_dry_run_skips_memory_audit() {
    // ── 1. Start the Discord test server.
    let srv = TestSrv::discord().await;

    // ── 2. Authenticate a DiscordClient.
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

    // ── 3. Create in-memory DB + persona + source binding.
    let mem = test_mem();
    let create = call(&mut pool, &mem, "meta_persona_create", json!({
        "slug": "dry-run-watcher",
        "name": "Dry Run Watcher",
        "system_prompt": "You watch and report without recording.",
    })).await;
    assert_ok(&create);

    let set_src = call(&mut pool, &mem, "meta_persona_set_sources", json!({
        "slug": "dry-run-watcher",
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

    // ── 4. Snapshot audit row count BEFORE the dry-run invoke.
    let before_rows = mem
        .list_persona_audit("dry-run-watcher", 500)
        .expect("list audit before")
        .len();

    // ── 5. Invoke with dry_run=true.
    let invoke = call(&mut pool, &mem, "meta_persona_invoke", json!({
        "slug": "dry-run-watcher",
        "user_prompt": "Preview bundle — no audit please.",
        "include_summaries": false,
        "dry_run": true
    })).await;
    assert_ok(&invoke);

    // ── 6. Parse the bundle and verify it is well-formed v1.
    let text = text_of(&invoke);
    let json_start = text.find('{').unwrap_or(0);
    let bundle: Value = serde_json::from_str(&text[json_start..])
        .unwrap_or_else(|e| panic!("bundle not valid JSON: {e}\nraw: {text}"));

    assert_eq!(
        bundle["bundle_version"].as_str().unwrap_or(""),
        "v1",
        "expected bundle_version v1 in dry-run bundle"
    );
    assert_eq!(
        bundle["persona"]["slug"].as_str().unwrap_or(""),
        "dry-run-watcher"
    );

    // dry_run field must be present and true in the returned JSON.
    assert_eq!(
        bundle["dry_run"].as_bool(),
        Some(true),
        "expected dry_run=true in bundle JSON"
    );

    // chats must be non-empty (source is bound — bundle shape is unchanged).
    let chats = bundle["chats"].as_array().expect("chats array");
    assert!(!chats.is_empty(), "expected chats in dry-run bundle (same shape as normal)");

    // ── 7. Verify audit row count increased by exactly 1 (the invoke row only).
    //
    // The rows written BEFORE the dry-run call include the create and set_sources
    // audit entries (which both use action="invoke" in the audit schema).  We
    // therefore use the before/after delta rather than filtering by action name.
    let all_rows_after = mem
        .list_persona_audit("dry-run-watcher", 500)
        .expect("list audit after");
    let after_rows = all_rows_after.len();

    let new_rows = after_rows - before_rows;
    assert_eq!(
        new_rows, 1,
        "dry_run=true should add exactly 1 audit row (the invoke row), got {new_rows}. \
         New rows: {:?}",
        &all_rows_after[before_rows..]
    );

    // ── 8. No memory_read rows exist for this persona at all (we never ran
    //       a non-dry-run invoke, so there are zero memory_read rows).
    let memory_reads: Vec<&Value> = all_rows_after
        .iter()
        .filter(|r| r["action"].as_str() == Some("memory_read"))
        .collect();
    assert!(
        memory_reads.is_empty(),
        "dry_run=true should suppress memory_read audit rows, found: {memory_reads:?}"
    );

    // ── 9. Confirm the one new row is the invoke row (payload contains dry_run).
    let new_row = &all_rows_after[before_rows];
    let payload = new_row["payload_json"].as_str().unwrap_or("");
    assert!(
        payload.contains("dry_run"),
        "new audit row payload should mention dry_run, got: {payload}"
    );
}
