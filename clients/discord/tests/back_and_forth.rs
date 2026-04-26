//! Back-and-forth chat integration test for poly-discord.
//!
//! Spins up the mock Discord REST API server in-process, authenticates two
//! animal accounts (Koala + Kangaroo), and exercises a full chat sequence:
//!   1. Channel discovery in the shared "Australiana" guild
//!   2. Koala → Kangaroo message
//!   3. Kangaroo replies with send_reply_message
//!   4. Kangaroo sends a mention message
//!
//! Discord's `ClientBackend` does not expose a `send_reaction` method, so the
//! reaction step is skipped with a note.
//!
//! Run with:
//! ```
//! cargo test -p poly-discord --test back_and_forth
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use poly_client::{AuthCredentials, ClientBackend, MessageContent, MessageQuery};
use poly_discord::DiscordClient;
use poly_test_discord::{DiscordState, router};
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

struct TestServer {
    base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        let base_url = format!("http://127.0.0.1:{port}");
        let ws_url = format!("ws://127.0.0.1:{port}/gateway/ws");

        let state = Arc::new(DiscordState::new());
        state.seed();
        state.seed_moderation();
        *state.gateway_url.write().await = ws_url;

        let app = router(Arc::clone(&state));
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { rx.await.ok(); })
                .await
                .ok();
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        Self { base_url, _shutdown: tx }
    }

    async fn authenticated_client(&self, username: &str) -> DiscordClient {
        let resp: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/test/auth/token", self.base_url))
            .json(&serde_json::json!({ "username": username }))
            .send()
            .await
            .expect("test_auth_token POST")
            .json()
            .await
            .expect("parse token response");
        let token = resp["token"].as_str().expect("token field").to_string();

        let mut client = DiscordClient::with_base_url(self.base_url.clone());
        client
            .authenticate(AuthCredentials::Token(token))
            .await
            .expect("authenticate");
        client
    }
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

/// Full back-and-forth chat: Koala (A) ↔ Kangaroo (B) in the test-arena channel.
#[tokio::test]
async fn back_and_forth_koala_kangaroo() {
    let srv = TestServer::start().await;

    // Step 1 — Authenticate both animals.
    let koala = srv.authenticated_client("koala").await;
    let kangaroo = srv.authenticated_client("kangaroo").await;

    assert!(koala.is_authenticated());
    assert!(kangaroo.is_authenticated());

    // Step 2 — Both animals discover the shared test-arena channel in guild 100.
    // Guild 100 "Australiana" has both koala (user 1) and kangaroo (user 2).
    // The test-arena channel has id 250.
    let koala_channels = koala.get_channels("100").await.expect("koala get_channels guild 100");
    let kangaroo_channels = kangaroo.get_channels("100").await.expect("kangaroo get_channels guild 100");

    let koala_arena = koala_channels.iter().find(|c| c.name == "test-arena")
        .expect("Koala should see test-arena channel in guild 100");
    let kangaroo_arena = kangaroo_channels.iter().find(|c| c.name == "test-arena")
        .expect("Kangaroo should see test-arena channel in guild 100");

    assert_eq!(koala_arena.id, kangaroo_arena.id, "Both should reference the same channel ID");
    let arena_id = &koala_arena.id;

    // Step 3 — A → B: Koala sends "hello from koala".
    let koala_msg = koala
        .send_message(arena_id, MessageContent::Text("hello from koala".to_string()))
        .await
        .expect("koala send_message");

    assert!(!koala_msg.id.is_empty(), "Sent message should have an ID");

    // Kangaroo fetches messages and verifies Koala's message is present.
    let messages = kangaroo
        .get_messages(arena_id, MessageQuery { limit: Some(20), ..Default::default() })
        .await
        .expect("kangaroo get_messages");

    let found = messages.iter().find(|m| {
        m.id == koala_msg.id
            && matches!(&m.content, MessageContent::Text(t) if t == "hello from koala")
    });
    assert!(
        found.is_some(),
        "Kangaroo should see Koala's message; messages: {messages:?}"
    );

    // Step 4 — Reaction: Discord's `ClientBackend` does not expose a
    // `send_reaction` method. The reaction endpoint is Discord-internal
    // (PUT /api/v10/channels/{id}/messages/{id}/reactions/{emoji}/@me).
    // Skipping reaction step — no trait method available.

    // Step 5 — Quote/reply: Kangaroo replies to Koala's message.
    let kangaroo_reply = kangaroo
        .send_reply_message(
            arena_id,
            &koala_msg.id,
            MessageContent::Text("hello from kangaroo".to_string()),
        )
        .await
        .expect("kangaroo send_reply_message");

    assert!(!kangaroo_reply.id.is_empty(), "Reply should have an ID");

    // Koala fetches messages; the reply should appear with reply_to.
    let messages_after = koala
        .get_messages(arena_id, MessageQuery { limit: Some(20), ..Default::default() })
        .await
        .expect("koala get_messages after reply");

    let reply_msg = messages_after.iter().find(|m| m.id == kangaroo_reply.id)
        .expect("Koala should see Kangaroo's reply");

    // Discord's ClientBackend uses the default send_reply_message which calls
    // send_message — reply_to threading is tracked by Discord's message_reference
    // field which this mock doesn't populate back. Verify the message was sent.
    assert!(
        matches!(&reply_msg.content, MessageContent::Text(t) if t == "hello from kangaroo"),
        "Kangaroo's reply should contain 'hello from kangaroo'; msg: {reply_msg:?}"
    );

    // Step 6 — Mention: Kangaroo sends a @koala mention.
    let mention_msg = kangaroo
        .send_message(arena_id, MessageContent::Text("<@1> great chat!".to_string()))
        .await
        .expect("kangaroo send mention");

    let messages_with_mention = koala
        .get_messages(arena_id, MessageQuery { limit: Some(20), ..Default::default() })
        .await
        .expect("koala get_messages after mention");

    let mention_found = messages_with_mention.iter().find(|m| m.id == mention_msg.id);
    assert!(
        mention_found.is_some(),
        "Koala should see Kangaroo's mention message"
    );
    assert!(
        matches!(&mention_found.unwrap().content, MessageContent::Text(t) if t.contains("<@1>")),
        "Mention message should contain Discord user mention format <@1>"
    );
}
