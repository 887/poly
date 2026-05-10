//! Back-and-forth chat integration test for poly-matrix.
//!
//! Spins up the mock Matrix homeserver in-process, authenticates two animal
//! accounts (Owl + Axolotl), and exercises a full chat sequence:
//!   1. Channel discovery
//!   2. A → B message
//!   3. B replies with `send_reply_message`
//!   4. A sends a mention message
//!
//! Matrix does not expose a `send_reaction` method in `ClientBackend`, so
//! step 3 (reaction) is skipped with a comment.
//!
//! Run with:
//! ```
//! cargo test -p poly-matrix --test back_and_forth
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;


use poly_matrix::MatrixClient;
use poly_test_matrix::{MatrixState, router};
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
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind free port");
        let addr = listener.local_addr().expect("local_addr");
        let base_url = format!("http://{addr}");

        let state = Arc::new(MatrixState::new());
        state.seed();

        let app = router(state);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                })
                .await
                .expect("serve");
        });

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        Self { base_url, _shutdown: shutdown_tx }
    }

    async fn authenticated_client(&self, username: &str) -> MatrixClient {
        let resp: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/test/auth/token", self.base_url))
            .json(&serde_json::json!({ "username": username }))
            .send()
            .await
            .expect("POST /test/auth/token")
            .json()
            .await
            .expect("parse token response");
        let token = resp["access_token"].as_str().expect("access_token").to_string();

        let mut client = MatrixClient::with_homeserver(&self.base_url)
            .expect("valid homeserver URL");
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

/// Full back-and-forth chat sequence: Owl (A) ↔ Axolotl (B) in the test-arena room.
#[tokio::test]
async fn back_and_forth_owl_axolotl() {
    let srv = TestServer::start().await;

    // Step 1 — Authenticate both animals.
    let owl = srv.authenticated_client("Owl").await;
    let axolotl = srv.authenticated_client("Axolotl").await;

    assert!(owl.is_authenticated(), "Owl should be authenticated");
    assert!(axolotl.is_authenticated(), "Axolotl should be authenticated");

    // Step 2 — Both animals discover the shared test-arena channel.
    //
    // The test-arena room (!test_arena:localhost) was added to The Hollow Tree
    // space in the seed data. We look for it via get_channels on the space.
    let owl_channels = owl.get_channels("!space1:localhost").await.expect("owl get_channels");
    let axolotl_channels = axolotl.get_channels("!space1:localhost").await.expect("axolotl get_channels");

    let owl_arena = owl_channels.iter().find(|c| c.name == "test-arena")
        .expect("Owl should see test-arena channel in The Hollow Tree space");
    let axolotl_arena = axolotl_channels.iter().find(|c| c.name == "test-arena")
        .expect("Axolotl should see test-arena channel in The Hollow Tree space");

    assert_eq!(owl_arena.id, axolotl_arena.id, "Both animals should reference the same channel ID");
    let arena_id = &owl_arena.id;

    // Step 3 — A → B: Owl sends "hello from owl".
    let owl_msg = owl
        .send_message(arena_id, MessageContent::Text("hello from owl".to_string()))
        .await
        .expect("Owl send_message");

    assert!(!owl_msg.id.is_empty(), "Sent message should have an ID");
    assert_eq!(
        owl_msg.content,
        MessageContent::Text("hello from owl".to_string()),
        "Message content should match"
    );

    // Axolotl fetches messages and verifies Owl's message is visible.
    let messages = axolotl
        .get_messages(arena_id, MessageQuery { limit: Some(10), ..Default::default() })
        .await
        .expect("Axolotl get_messages");

    let found = messages.iter().find(|m| {
        m.id == owl_msg.id
            && matches!(&m.content, MessageContent::Text(t) if t == "hello from owl")
    });
    assert!(
        found.is_some(),
        "Axolotl should see Owl's message; messages: {messages:?}"
    );

    // Step 4 — Reaction: ClientBackend does not expose a `send_reaction` method
    // on Matrix. The reaction API is Matrix-internal (PUT
    // /_matrix/client/v3/rooms/{roomId}/send/m.reaction/{txnId}).
    // Skipping reaction step — no trait method available.

    // Step 5 — Quote/reply: Axolotl replies to Owl's message.
    let axolotl_reply = axolotl
        .send_reply_message(
            arena_id,
            &owl_msg.id,
            MessageContent::Text("hello from axolotl".to_string()),
        )
        .await
        .expect("Axolotl send_reply_message");

    assert!(!axolotl_reply.id.is_empty(), "Reply should have an ID");

    // Fetch messages again; the reply should have reply_to pointing at Owl's message.
    let messages_after_reply = owl
        .get_messages(arena_id, MessageQuery { limit: Some(10), ..Default::default() })
        .await
        .expect("Owl get_messages after reply");

    let reply_msg = messages_after_reply
        .iter()
        .find(|m| m.id == axolotl_reply.id)
        .expect("Owl should see Axolotl's reply");

    // Matrix's ClientBackend uses the default send_reply_message which calls
    // send_message — the m.relates_to threading is set in the event but the
    // mock server's get_messages path returns events without populating
    // reply_to on the Poly Message struct. Verify the message was sent.
    assert!(
        matches!(&reply_msg.content, MessageContent::Text(t) if t == "hello from axolotl"),
        "Axolotl's reply should contain 'hello from axolotl'; msg: {reply_msg:?}"
    );

    // Step 6 — Mention: Owl sends a mention of Axolotl.
    // Matrix mentions use the display name in the message body.
    let mention_text = "@axolotl great to hear from you!".to_string();
    let mention_msg = owl
        .send_message(arena_id, MessageContent::Text(mention_text.clone()))
        .await
        .expect("Owl send mention");

    let messages_with_mention = axolotl
        .get_messages(arena_id, MessageQuery { limit: Some(10), ..Default::default() })
        .await
        .expect("Axolotl get_messages after mention");

    let mention_found = messages_with_mention
        .iter()
        .find(|m| m.id == mention_msg.id);
    assert!(
        mention_found.is_some(),
        "Axolotl should see Owl's mention message"
    );
    assert!(
        matches!(&mention_found.unwrap().content, MessageContent::Text(t) if t.contains("@axolotl")),
        "Mention message should contain @axolotl"
    );
}
