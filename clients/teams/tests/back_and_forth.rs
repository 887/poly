//! Back-and-forth chat integration test for poly-teams.
//!
//! Spins up the mock Teams/Graph API server in-process, authenticates two
//! animal accounts (Sheep + Walrus), and exercises a full chat sequence:
//!   1. Channel discovery in the shared "Contoso Corp" team
//!   2. Sheep → Walrus message
//!   3. Reaction via `TeamsClient::react` (Teams-specific, not in ClientBackend trait)
//!   4. Walrus replies (send_message with @-mention text)
//!
//! Teams does not model `reply_to` natively in the `ClientBackend` mapping,
//! so step 5 (quote/reply) uses a regular send_message with a mention prefix.
//!
//! Run with:
//! ```
//! cargo test -p poly-teams --test back_and_forth
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;

use poly_client::{AuthCredentials, ClientBackend, MessageContent, MessageQuery};
use poly_teams::TeamsClient;
use poly_test_teams::{TeamsState, router};
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

        let state = Arc::new(TeamsState::new());
        state.seed();

        let app = router(Arc::clone(&state));
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { rx.await.ok(); })
                .await
                .ok();
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        Self { base_url, _shutdown: tx }
    }

    async fn authenticated_client(&self, display_name: &str) -> TeamsClient {
        let resp: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/test/auth/token", self.base_url))
            .json(&serde_json::json!({ "username": display_name }))
            .send()
            .await
            .expect("test_auth_token POST")
            .json()
            .await
            .expect("parse token response");
        let token = resp["token"].as_str().expect("token field").to_string();

        let mut client = TeamsClient::with_base_url(self.base_url.clone());
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

/// Full back-and-forth chat: Sheep (A) ↔ Walrus (B) in the test-arena channel.
#[tokio::test]
async fn back_and_forth_sheep_walrus() {
    let srv = TestServer::start().await;

    // Step 1 — Authenticate both animals.
    let sheep = srv.authenticated_client("Sheep").await;
    let walrus = srv.authenticated_client("Walrus").await;

    assert!(sheep.is_authenticated());
    assert!(walrus.is_authenticated());

    // Step 2 — Both animals discover the shared test-arena channel.
    // "Contoso Corp" (T001) has both Sheep (U001) and Walrus (U002) as members.
    // The test-arena channel ID is stored as "T001/CH_ARENA".
    let sheep_channels = sheep.get_channels("T001").await.expect("sheep get_channels T001");
    let walrus_channels = walrus.get_channels("T001").await.expect("walrus get_channels T001");

    let sheep_arena = sheep_channels.iter().find(|c| c.name == "test-arena")
        .expect("Sheep should see test-arena channel in T001");
    let walrus_arena = walrus_channels.iter().find(|c| c.name == "test-arena")
        .expect("Walrus should see test-arena channel in T001");

    assert_eq!(sheep_arena.id, walrus_arena.id, "Both should reference the same channel");
    let arena_id = sheep_arena.id.clone(); // "T001/CH_ARENA"

    // Step 3 — A → B: Sheep sends "hello from sheep".
    let sheep_msg = sheep
        .send_message(&arena_id, MessageContent::Text("hello from sheep".to_string()))
        .await
        .expect("sheep send_message");

    assert!(!sheep_msg.id.is_empty(), "Sent message should have an ID");

    // Walrus fetches messages and verifies Sheep's message is present.
    let messages = walrus
        .get_messages(&arena_id, MessageQuery { limit: Some(20), ..Default::default() })
        .await
        .expect("walrus get_messages");

    let found = messages.iter().find(|m| {
        m.id == sheep_msg.id
            && matches!(&m.content, MessageContent::Text(t) if t == "hello from sheep")
    });
    assert!(
        found.is_some(),
        "Walrus should see Sheep's message; messages: {messages:?}"
    );

    // Step 4 — Reaction: Teams exposes react/unreact as TeamsClient-specific methods.
    // The underlying endpoint is /teams/{t}/channels/{c}/messages/{m}/setReaction.
    // The react() call stores the reaction server-side; the Teams client's message
    // mapping currently discards reactions (map_message_to_poly sets reactions: vec![]),
    // so we verify only that react() succeeds without error.
    sheep
        .react(&arena_id, &sheep_msg.id, "like")
        .await
        .expect("Sheep reacts to their own message");

    // Verify the message is still retrievable after the reaction call.
    let messages_after_react = walrus
        .get_messages(&arena_id, MessageQuery { limit: Some(20), ..Default::default() })
        .await
        .expect("walrus get_messages after reaction");

    let _reacted_msg = messages_after_react.iter().find(|m| m.id == sheep_msg.id)
        .expect("Sheep's message should still be present after reaction");

    // Step 5 — Reply: Teams does not model reply_to in ClientBackend::send_reply_message,
    // so we use a regular send_message with a mention prefix as the "reply" convention.
    // Note: send_reply_message defaults to send_message for Teams.
    let reply_msg = walrus
        .send_message(&arena_id, MessageContent::Text("@sheep hello from walrus!".to_string()))
        .await
        .expect("walrus send reply");

    let messages_after_reply = sheep
        .get_messages(&arena_id, MessageQuery { limit: Some(20), ..Default::default() })
        .await
        .expect("sheep get_messages after reply");

    let reply_found = messages_after_reply.iter().find(|m| m.id == reply_msg.id);
    assert!(
        reply_found.is_some(),
        "Sheep should see Walrus's reply message"
    );

    // Step 6 — Mention: Walrus sends a @sheep mention (covered in step 5 above).
    // The message "hello from walrus!" already contains the @sheep mention.
    assert!(
        matches!(
            &reply_found.unwrap().content,
            MessageContent::Text(t) if t.contains("@sheep")
        ),
        "Reply message should contain @sheep mention"
    );
}
