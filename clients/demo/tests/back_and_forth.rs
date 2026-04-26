//! Back-and-forth API-contract test for poly-demo.
//!
//! The demo backend is purely in-memory with static seed data. `DemoClient`
//! (Cat) and `DemoClient2` (Dog) each hold their own independent data sets —
//! there is no shared mutable state between them, so a message sent by Cat
//! is not visible to Dog's `get_messages`. The test therefore validates the
//! API contract rather than true cross-client communication:
//!
//!   1. Both demo animals authenticate (no credentials needed — always succeeds)
//!   2. Server and channel discovery work for both
//!   3. `send_message` returns a Message with the correct content and a non-empty ID
//!   4. `get_messages` returns the seeded messages visible to each client
//!   5. `send_reply_message` with a known seeded message ID sets `reply_to`
//!   6. `send_reply_message` with an unknown ID returns a message with `reply_to = None`
//!      (graceful degradation — no panic, no error)
//!
//! Run with:
//! ```
//! cargo test -p poly-demo --test back_and_forth
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_client::{AuthCredentials, ClientBackend, MessageContent, MessageQuery};
use poly_demo::{DemoClient, DemoClient2};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Authenticate a DemoClient (Cat) — credentials are ignored.
async fn authenticated_cat() -> DemoClient {
    let mut client = DemoClient::new();
    client
        .authenticate(AuthCredentials::Token("ignored".to_string()))
        .await
        .expect("demo cat authenticate");
    client
}

/// Authenticate a DemoClient2 (Dog) — credentials are ignored.
async fn authenticated_dog() -> DemoClient2 {
    let mut client = DemoClient2::new();
    client
        .authenticate(AuthCredentials::Token("ignored".to_string()))
        .await
        .expect("demo dog authenticate");
    client
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

/// API-contract back-and-forth: Cat (A) and Dog (B) in demo channels.
#[tokio::test]
async fn back_and_forth_cat_dog() {
    // Step 1 — Authenticate both demo animals.
    let cat = authenticated_cat().await;
    let dog = authenticated_dog().await;

    assert!(cat.is_authenticated());
    assert!(dog.is_authenticated());

    // Step 2 — Server discovery for both animals.
    let cat_servers = cat.get_servers().await.expect("cat get_servers");
    let dog_servers = dog.get_servers().await.expect("dog get_servers");

    assert!(!cat_servers.is_empty(), "Cat should have at least one server");
    assert!(!dog_servers.is_empty(), "Dog should have at least one server");

    // Step 3 — Channel discovery for Cat's first server.
    let cat_server_id = &cat_servers[0].id;
    let cat_channels = cat
        .get_channels(cat_server_id)
        .await
        .expect("cat get_channels");

    assert!(!cat_channels.is_empty(), "Cat should see channels in their server");

    // Find the general channel (always seeded in the first demo server).
    let cat_general = cat_channels
        .iter()
        .find(|c| c.id == "ch-general")
        .expect("Cat should see ch-general channel");

    assert_eq!(cat_general.id, "ch-general");

    // Step 4 — Cat reads seeded messages from ch-general.
    let cat_messages = cat
        .get_messages("ch-general", MessageQuery { limit: Some(20), ..Default::default() })
        .await
        .expect("cat get_messages ch-general");

    assert!(
        !cat_messages.is_empty(),
        "ch-general should have seeded messages"
    );

    // Step 5 — A → (self): Cat sends a message and verifies the returned Message.
    // Note: the demo backend's send_message doesn't persist — it returns a new
    // Message with the correct content and a random ID, but it won't appear in
    // a subsequent get_messages call (static seed data only).
    let cat_sent = cat
        .send_message("ch-general", MessageContent::Text("hello from cat".to_string()))
        .await
        .expect("cat send_message");

    assert!(
        !cat_sent.id.is_empty(),
        "Sent message should have a non-empty ID"
    );
    assert!(
        matches!(&cat_sent.content, MessageContent::Text(t) if t == "hello from cat"),
        "Sent message should contain 'hello from cat'"
    );

    // Step 6 — Reaction: no send_reaction in ClientBackend; skipped.

    // Step 7 — Reply using a known seeded message ID. The demo
    // `send_reply_message` looks up the reply target in the seeded message
    // list; if found it sets `reply_to`.
    // "msg-ch-general-0" is always present in ch-general seed data.
    let cat_reply = cat
        .send_reply_message(
            "ch-general",
            "msg-ch-general-0",
            MessageContent::Text("replying to msg 0".to_string()),
        )
        .await
        .expect("cat send_reply_message");

    assert!(
        !cat_reply.id.is_empty(),
        "Reply message should have a non-empty ID"
    );
    // The demo reply path populates reply_to when the original message is found.
    assert!(
        cat_reply.reply_to.is_some(),
        "Reply to a seeded message should populate reply_to; msg: {cat_reply:?}"
    );
    assert_eq!(
        cat_reply.reply_to.as_ref().map(|r| r.message_id.as_str()),
        Some("msg-ch-general-0"),
        "reply_to.message_id should point at msg-ch-general-0"
    );

    // Step 8 — Graceful degradation: reply to an unknown ID → reply_to = None.
    let cat_orphan_reply = cat
        .send_reply_message(
            "ch-general",
            "msg-nonexistent-xyz",
            MessageContent::Text("orphan reply".to_string()),
        )
        .await
        .expect("cat send_reply_message with unknown id should not error");

    // No panic or error; reply_to is None since the original can't be found.
    assert!(
        cat_orphan_reply.reply_to.is_none(),
        "Reply to unknown message ID should have reply_to = None"
    );

    // Step 9 — Dog performs the same discovery + read flow.
    let dog_server_id = &dog_servers[0].id;
    let dog_channels = dog
        .get_channels(dog_server_id)
        .await
        .expect("dog get_channels");

    assert!(!dog_channels.is_empty(), "Dog should see channels in their server");

    // Dog's first server is "server-opensource" → ch2-general.
    let dog_general = dog_channels
        .iter()
        .find(|c| c.id == "ch2-general")
        .expect("Dog should see ch2-general channel");

    assert_eq!(dog_general.id, "ch2-general");

    let dog_messages = dog
        .get_messages("ch2-general", MessageQuery { limit: Some(20), ..Default::default() })
        .await
        .expect("dog get_messages ch2-general");

    assert!(
        !dog_messages.is_empty(),
        "ch2-general should have seeded messages"
    );

    let dog_sent = dog
        .send_message("ch2-general", MessageContent::Text("hello from dog".to_string()))
        .await
        .expect("dog send_message");

    assert!(
        !dog_sent.id.is_empty(),
        "Dog's sent message should have a non-empty ID"
    );
    assert!(
        matches!(&dog_sent.content, MessageContent::Text(t) if t == "hello from dog"),
        "Dog's sent message should contain 'hello from dog'"
    );
}
