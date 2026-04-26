//! Back-and-forth chat integration test for poly-stoat.
//!
//! Spins up the mock Stoat/Revolt server in-process, authenticates two animal
//! accounts (Stoat + Raccoon), and exercises a full chat sequence:
//!   1. Channel discovery in the shared "Test Arena" server
//!   2. Stoat → Raccoon message
//!   3. Raccoon replies using send_reply_message
//!   4. Raccoon sends a mention to Stoat
//!
//! Stoat's `ClientBackend` does not expose a `send_reaction` method, so
//! the reaction step is skipped with a comment.
//!
//! Run with:
//! ```
//! cargo test -p poly-stoat --test back_and_forth
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use poly_client::{AuthCredentials, ClientBackend, MessageContent, MessageQuery};
use poly_stoat::StoatClient;
use poly_test_common::TestServerBase;
use poly_test_stoat::{StoatState, router};

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

async fn start_test_server() -> (String, tokio::sync::oneshot::Sender<()>) {
    let state = Arc::new(StoatState::new());
    state.seed();

    let base = TestServerBase::bind(0).await.expect("bind random port");
    let base_url = base.base_url();
    let app = router(Arc::clone(&state));

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        axum::serve(base.listener, app)
            .with_graceful_shutdown(async { let _ = shutdown_rx.await; })
            .await
            .expect("test-stoat serve");
    });

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    (base_url, shutdown_tx)
}

async fn authenticated_stoat(base_url: &str) -> StoatClient {
    let mut client = StoatClient::with_base_url(base_url).expect("valid base url");
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "stoat".into(),
            password: "testpass123".into(),
        })
        .await
        .expect("stoat authenticate");
    client
}

async fn authenticated_raccoon(base_url: &str) -> StoatClient {
    let mut client = StoatClient::with_base_url(base_url).expect("valid base url");
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "raccoon".into(),
            password: "testpass123".into(),
        })
        .await
        .expect("raccoon authenticate");
    client
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

/// Full back-and-forth chat: Stoat (A) ↔ Raccoon (B) in the test-arena channel.
#[tokio::test]
async fn back_and_forth_stoat_raccoon() {
    let (base_url, _shutdown) = start_test_server().await;

    // Step 1 — Authenticate both animals.
    let stoat = authenticated_stoat(&base_url).await;
    let raccoon = authenticated_raccoon(&base_url).await;

    assert!(stoat.is_authenticated());
    assert!(raccoon.is_authenticated());

    // Step 2 — Both animals discover the shared test-arena channel.
    // The "Test Arena" server (SRV_ARENA) has both as members with a single
    // "test-arena" channel (CH_ARENA).
    let stoat_servers = stoat.get_servers().await.expect("stoat get_servers");
    let raccoon_servers = raccoon.get_servers().await.expect("raccoon get_servers");

    let stoat_arena_srv = stoat_servers.iter().find(|s| s.name == "Test Arena")
        .expect("Stoat should see the 'Test Arena' server");
    let raccoon_arena_srv = raccoon_servers.iter().find(|s| s.name == "Test Arena")
        .expect("Raccoon should see the 'Test Arena' server");

    assert_eq!(stoat_arena_srv.id, raccoon_arena_srv.id, "Both should see the same server ID");

    let stoat_channels = stoat
        .get_channels("SRV_ARENA")
        .await
        .expect("stoat get_channels for Test Arena");
    let raccoon_channels = raccoon
        .get_channels("SRV_ARENA")
        .await
        .expect("raccoon get_channels for Test Arena");

    let stoat_arena_ch = stoat_channels.iter().find(|c| c.name == "test-arena")
        .expect("Stoat should see the test-arena channel");
    let raccoon_arena_ch = raccoon_channels.iter().find(|c| c.name == "test-arena")
        .expect("Raccoon should see the test-arena channel");

    assert_eq!(stoat_arena_ch.id, raccoon_arena_ch.id, "Both see the same channel ID");
    let arena_ch_id = &stoat_arena_ch.id;

    // Step 3 — A → B: Stoat sends "hello from stoat".
    let stoat_msg = stoat
        .send_message(arena_ch_id, MessageContent::Text("hello from stoat".to_string()))
        .await
        .expect("stoat send_message");

    assert!(!stoat_msg.id.is_empty(), "Sent message should have an ID");

    // Raccoon fetches messages and verifies Stoat's message is present.
    let messages = raccoon
        .get_messages(arena_ch_id, MessageQuery { limit: Some(50), ..Default::default() })
        .await
        .expect("raccoon get_messages");

    let found = messages.iter().find(|m| {
        m.id == stoat_msg.id
            && matches!(&m.content, MessageContent::Text(t) if t == "hello from stoat")
    });
    assert!(
        found.is_some(),
        "Raccoon should see Stoat's message; messages: {messages:?}"
    );

    // Step 4 — Reaction: ClientBackend does not expose a `send_reaction` method.
    // Stoat's reaction API is internal (PUT /channels/{id}/messages/{id}/reactions/{emoji}).
    // Skipping reaction step — no trait method available.

    // Step 5 — Quote/reply: Raccoon replies to Stoat's message.
    let raccoon_reply = raccoon
        .send_reply_message(
            arena_ch_id,
            &stoat_msg.id,
            MessageContent::Text("hello from raccoon".to_string()),
        )
        .await
        .expect("raccoon send_reply_message");

    assert!(!raccoon_reply.id.is_empty(), "Reply should have an ID");

    // Stoat fetches messages; the reply should appear with reply_to.
    let messages_after = stoat
        .get_messages(arena_ch_id, MessageQuery { limit: Some(50), ..Default::default() })
        .await
        .expect("stoat get_messages after reply");

    let reply_msg = messages_after.iter().find(|m| m.id == raccoon_reply.id)
        .expect("Stoat should see Raccoon's reply");

    assert!(
        reply_msg.reply_to.is_some(),
        "Reply should have reply_to set; msg: {reply_msg:?}"
    );
    assert_eq!(
        reply_msg.reply_to.as_ref().map(|r| r.message_id.as_str()),
        Some(stoat_msg.id.as_str()),
        "reply_to.message_id should point at Stoat's original message"
    );

    // Step 6 — Mention: Raccoon sends a @stoat mention.
    let mention_text = "@stoat nice to meet you!".to_string();
    let mention_msg = raccoon
        .send_message(arena_ch_id, MessageContent::Text(mention_text.clone()))
        .await
        .expect("raccoon send mention");

    let messages_with_mention = stoat
        .get_messages(arena_ch_id, MessageQuery { limit: Some(50), ..Default::default() })
        .await
        .expect("stoat get_messages after mention");

    let mention_found = messages_with_mention.iter().find(|m| m.id == mention_msg.id);
    assert!(mention_found.is_some(), "Stoat should see Raccoon's mention message");
    assert!(
        matches!(&mention_found.unwrap().content, MessageContent::Text(t) if t.contains("@stoat")),
        "Mention message should contain @stoat"
    );
}
