//! Back-and-forth chat integration test for poly-lemmy.
//!
//! Spins up the mock Lemmy REST API server in-process, authenticates two
//! animal accounts (Hedgehog + Beaver), and exercises a full chat sequence
//! against the shared "test-arena" post (id=10) in the test_arena community:
//!   1. Authentication of both animals
//!   2. Server discovery: test_arena community (lemmy-community-3)
//!   3. Hedgehog → Beaver: sends a comment on the shared post
//!   4. Beaver reads and verifies Hedgehog's comment
//!   5. Beaver replies via send_reply_message (parent_id threading)
//!   6. Hedgehog reads and verifies Beaver's reply exists
//!   7. Beaver sends a "@hedgehog" mention comment
//!   8. Hedgehog verifies the mention comment is visible
//!
//! Note: The Lemmy `ClientBackend` does not populate `reply_to` on comments
//! (Lemmy uses path-based threading, not Poly's reply_to convention).
//! The test therefore checks that the reply comment was stored and returned,
//! not that `reply_to` is set.
//!
//! Reaction step is skipped: `ClientBackend` has no `send_reaction` method
//! for Lemmy (Lemmy vote endpoints are not part of the unified trait).
//!
//! Run with:
//! ```
//! cargo test -p poly-lemmy --test back_and_forth
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]


use poly_lemmy::LemmyClient;
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

        let router = poly_test_lemmy::router();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, router.into_make_service())
                .with_graceful_shutdown(async {
                    rx.await.ok();
                })
                .await
                .ok();
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        Self { base_url, _shutdown: tx }
    }

    async fn authenticated_client(&self, username: &str, password: &str) -> LemmyClient {
        let mut client = LemmyClient::new(&self.base_url);
        client
            .authenticate(AuthCredentials::EmailPassword {
                email: username.to_string(),
                password: password.to_string(),
            })
            .await
            .expect("authenticate");
        client
    }
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

/// Full back-and-forth chat: Hedgehog (A) ↔ Beaver (B) on the test-arena post.
#[tokio::test]
async fn back_and_forth_hedgehog_beaver() {
    let srv = TestServer::start().await;

    // Step 1 — Authenticate both animals.
    let hedgehog = srv.authenticated_client("hedgehog", "testpass123").await;
    let beaver = srv.authenticated_client("beaver", "testpass123").await;

    assert!(hedgehog.is_authenticated());
    assert!(beaver.is_authenticated());

    // Step 2 — Server discovery: both can see test_arena community.
    let hedgehog_servers = hedgehog.get_servers().await.expect("hedgehog get_servers");
    let arena_server = hedgehog_servers
        .iter()
        .find(|s| s.name == "Test Arena")
        .expect("Hedgehog should see Test Arena community as a server");

    // Community 3 → id = "lemmy-community-3"
    assert_eq!(
        arena_server.id, "lemmy-community-3",
        "Test Arena server should have id lemmy-community-3"
    );

    // Step 3 — Use the seeded post (id=10) as a thread channel.
    // The channel id for a post thread is `lemmy-post-{id}`.
    let arena_post_channel = "lemmy-post-10";

    // Step 4 — A → B: Hedgehog posts "hello from hedgehog".
    let hedgehog_comment = hedgehog
        .send_message(
            arena_post_channel,
            MessageContent::Text("hello from hedgehog".to_string()),
        )
        .await
        .expect("hedgehog send_message");

    assert!(
        !hedgehog_comment.id.is_empty(),
        "Sent comment should have an ID"
    );

    // Beaver fetches messages and verifies Hedgehog's comment is present.
    let messages = beaver
        .get_messages(arena_post_channel, MessageQuery { limit: Some(20), ..Default::default() })
        .await
        .expect("beaver get_messages");

    let found = messages.iter().find(|m| {
        m.id == hedgehog_comment.id
            && matches!(&m.content, MessageContent::Text(t) if t == "hello from hedgehog")
    });
    assert!(
        found.is_some(),
        "Beaver should see Hedgehog's comment; messages: {messages:?}"
    );

    // Step 5 — Reaction: ClientBackend has no send_reaction method for Lemmy.
    // Skipping reaction step — no trait method available.

    // Step 6 — Quote/reply: Beaver replies to Hedgehog's comment.
    // send_reply_message sets parent_id on the Lemmy comment for threading.
    let beaver_reply = beaver
        .send_reply_message(
            arena_post_channel,
            &hedgehog_comment.id,
            MessageContent::Text("hello from beaver".to_string()),
        )
        .await
        .expect("beaver send_reply_message");

    assert!(!beaver_reply.id.is_empty(), "Reply should have an ID");

    // Hedgehog fetches messages; Beaver's reply should be present.
    let messages_after = hedgehog
        .get_messages(arena_post_channel, MessageQuery { limit: Some(20), ..Default::default() })
        .await
        .expect("hedgehog get_messages after reply");

    let reply_found = messages_after.iter().find(|m| m.id == beaver_reply.id);
    assert!(
        reply_found.is_some(),
        "Hedgehog should see Beaver's reply comment; messages: {messages_after:?}"
    );

    // Verify the reply text is correct.
    assert!(
        matches!(
            &reply_found.unwrap().content,
            MessageContent::Text(t) if t == "hello from beaver"
        ),
        "Beaver's reply should contain 'hello from beaver'"
    );

    // Step 7 — Mention: Beaver sends a @hedgehog mention.
    let mention_comment = beaver
        .send_message(
            arena_post_channel,
            MessageContent::Text("@hedgehog great chat!".to_string()),
        )
        .await
        .expect("beaver send mention");

    // Step 8 — Hedgehog reads and verifies the mention.
    let messages_with_mention = hedgehog
        .get_messages(arena_post_channel, MessageQuery { limit: Some(20), ..Default::default() })
        .await
        .expect("hedgehog get_messages after mention");

    let mention_found = messages_with_mention.iter().find(|m| m.id == mention_comment.id);
    assert!(
        mention_found.is_some(),
        "Hedgehog should see Beaver's mention comment"
    );
    assert!(
        matches!(
            &mention_found.unwrap().content,
            MessageContent::Text(t) if t.contains("@hedgehog")
        ),
        "Mention comment should contain '@hedgehog'"
    );
}
