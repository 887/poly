//! Integration tests for the `poly-hackernews` client.
//!
//! Spins up a `poly-test-hackernews` server in-process and exercises every
//! `ClientBackend` method that `HackerNewsClient` implements.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use poly_client::{AuthCredentials, ClientBackend, MessageQuery};
use poly_hackernews::HackerNewsClient;
use poly_test_hackernews::TestHnServer;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn hn_base_url(server: &TestHnServer) -> String {
    // The HN API client expects the base URL to include the "/v0" prefix,
    // mirroring the real API: "https://hacker-news.firebaseio.com/v0".
    format!("{}/v0", server.base_url)
}

async fn client_connected_to(server: &TestHnServer) -> HackerNewsClient {
    let mut client = HackerNewsClient::with_base_url(hn_base_url(server));
    client
        .authenticate(AuthCredentials::Token(String::new()))
        .await
        .expect("guest authenticate should succeed");
    client
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `authenticate()` with no credentials returns a valid guest session.
#[tokio::test]
async fn test_authenticate_guest() {
    let server = TestHnServer::start().await;
    let mut client = HackerNewsClient::with_base_url(hn_base_url(&server));

    assert!(!client.is_authenticated(), "should start unauthenticated");

    let session = client
        .authenticate(AuthCredentials::Token(String::new()))
        .await
        .expect("authenticate should succeed");

    assert!(!session.id.is_empty(), "session.id must be non-empty");
    assert!(
        !session.user.id.is_empty(),
        "session.user.id must be non-empty"
    );
    assert!(
        !session.user.display_name.is_empty(),
        "session.user.display_name must be non-empty"
    );
    assert_eq!(session.backend, "hackernews");
    assert!(client.is_authenticated(), "should be authenticated after call");
}

/// `get_servers()` returns exactly one virtual "Hacker News" server with ID "hn".
#[tokio::test]
async fn test_get_servers() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let servers = client.get_servers().await.expect("get_servers should succeed");

    assert_eq!(servers.len(), 1, "expected exactly 1 server");
    let hn = &servers[0];
    assert_eq!(hn.id, "hn");
    assert_eq!(hn.name, "Hacker News");
    assert_eq!(hn.backend, "hackernews");
}

/// `get_channels("hn")` returns 6 feed channels with correct names.
#[tokio::test]
async fn test_get_channels() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let channels = client
        .get_channels("hn")
        .await
        .expect("get_channels should succeed");

    assert_eq!(channels.len(), 6, "expected 6 feed channels");

    let names: Vec<&str> = channels.iter().map(|ch| ch.name.as_str()).collect();
    for expected in &["Top", "New", "Best", "Ask HN", "Show HN", "Jobs"] {
        assert!(
            names.contains(expected),
            "missing channel '{expected}'; got: {names:?}"
        );
    }

    for ch in &channels {
        assert!(!ch.id.is_empty(), "channel.id must not be empty");
        assert_eq!(ch.server_id, "hn");
    }
}

/// `get_messages("hn-top", ...)` returns stories from the test server.
#[tokio::test]
async fn test_get_messages_top() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let messages = client
        .get_messages("hn-top", MessageQuery { limit: Some(10), ..Default::default() })
        .await
        .expect("get_messages(hn-top) should succeed");

    assert!(
        !messages.is_empty(),
        "expected at least 1 message from the top feed"
    );

    for msg in &messages {
        assert!(!msg.id.is_empty(), "message.id must not be empty");
        assert!(!msg.author.id.is_empty(), "message.author.id must not be empty");
        // Each message body must contain the story title
        let body = match &msg.content {
            poly_client::MessageContent::Text(t) => t.clone(),
            poly_client::MessageContent::WithAttachments { text, .. } => text.clone(),
        };
        assert!(!body.is_empty(), "message body must not be empty");
    }
}

/// `get_messages("hn-post-1001", ...)` returns child comments for that story.
#[tokio::test]
async fn test_get_messages_story_comments() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    // Story 1001 has kids [2001, 2002] in the seed data.
    let comments = client
        .get_messages(
            "hn-post-1001",
            MessageQuery { limit: Some(10), ..Default::default() },
        )
        .await
        .expect("get_messages(hn-post-1001) should succeed");

    assert_eq!(comments.len(), 2, "story 1001 has 2 direct comment kids");

    for comment in &comments {
        assert!(!comment.id.is_empty(), "comment.id must not be empty");
        assert!(
            !comment.author.id.is_empty(),
            "comment.author.id must not be empty"
        );
    }
}

/// `get_dm_channels()` returns an empty list (HN has no DMs).
#[tokio::test]
async fn test_list_dms() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let dms = client
        .get_dm_channels()
        .await
        .expect("get_dm_channels should succeed");

    assert!(dms.is_empty(), "HN has no DMs, expected empty list");
}

/// `get_friends()` returns an empty list (HN has no friends concept).
#[tokio::test]
async fn test_list_friends() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let friends = client
        .get_friends()
        .await
        .expect("get_friends should succeed");

    assert!(friends.is_empty(), "HN has no friends, expected empty list");
}

/// `get_server("hn")` returns the virtual HN server.
#[tokio::test]
async fn test_get_server_by_id() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let hn = client
        .get_server("hn")
        .await
        .expect("get_server(hn) should succeed");

    assert_eq!(hn.id, "hn");
    assert_eq!(hn.name, "Hacker News");
    assert!(!hn.categories.is_empty(), "server should have categories");
}

/// `get_server` with an unknown ID returns a `NotFound` error.
#[tokio::test]
async fn test_get_server_unknown_id() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let result = client.get_server("does-not-exist").await;
    assert!(result.is_err(), "unknown server should return an error");
}

/// `get_channel("hn-top")` returns the Top feed channel.
#[tokio::test]
async fn test_get_channel_by_slug() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let ch = client
        .get_channel("hn-top")
        .await
        .expect("get_channel(hn-top) should succeed");

    assert_eq!(ch.id, "hn-top");
    assert_eq!(ch.name, "Top");
    assert_eq!(ch.server_id, "hn");
}

/// `get_channel` with an unknown slug returns a `NotFound` error.
#[tokio::test]
async fn test_get_channel_unknown_slug() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let result = client.get_channel("nonexistent-feed").await;
    assert!(result.is_err(), "unknown channel should return an error");
}

/// `get_channels` with an unknown server ID returns a `NotFound` error.
#[tokio::test]
async fn test_get_channels_unknown_server() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let result = client.get_channels("not-a-server").await;
    assert!(result.is_err(), "unknown server should return an error");
}

/// `send_message` always returns a `NotSupported` error — HN is read-only.
#[tokio::test]
async fn test_send_message_not_supported() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let result = client
        .send_message("hn-top", poly_client::MessageContent::Text("hello".to_string()))
        .await;

    assert!(result.is_err(), "send_message should fail for HN (read-only)");
    let err_str = format!("{:?}", result.unwrap_err());
    assert!(
        err_str.contains("read-only") || err_str.contains("NotSupported"),
        "error message should mention read-only: {err_str}"
    );
}

/// `get_messages` with limit=1 returns at most 1 story.
#[tokio::test]
async fn test_get_messages_limit_respected() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let messages = client
        .get_messages("hn-top", MessageQuery { limit: Some(1), ..Default::default() })
        .await
        .expect("get_messages with limit=1 should succeed");

    assert_eq!(messages.len(), 1, "limit=1 should return exactly 1 message");
}

/// Other feed channels work the same as top.
#[tokio::test]
async fn test_get_messages_other_feeds() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    // Use the actual channel IDs as known by the client
    for feed in &["hn-new", "hn-best", "hn-ask", "hn-show"] {
        let msgs = client
            .get_messages(feed, MessageQuery { limit: Some(5), ..Default::default() })
            .await
            .unwrap_or_else(|e| panic!("get_messages({feed}) failed: {e:?}"));
        assert!(!msgs.is_empty(), "feed '{feed}' should have at least one message");
    }
}

/// `get_messages("hn-jobs-ch", ...)` returns job items.
#[tokio::test]
async fn test_get_messages_jobs_feed() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let msgs = client
        .get_messages("hn-jobs-ch", MessageQuery { limit: Some(5), ..Default::default() })
        .await
        .expect("get_messages(hn-jobs-ch) should succeed");

    assert!(!msgs.is_empty(), "jobs feed should have at least one message");
}

/// `logout()` clears the session so `is_authenticated()` returns false.
#[tokio::test]
async fn test_logout_clears_session() {
    let server = TestHnServer::start().await;
    let mut client = client_connected_to(&server).await;

    assert!(client.is_authenticated(), "should be authenticated before logout");

    client.logout().await.expect("logout should succeed");

    assert!(!client.is_authenticated(), "should be unauthenticated after logout");
}

/// `get_user(id)` returns a `User` with the provided ID.
#[tokio::test]
async fn test_get_user() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let user = client.get_user("pg").await.expect("get_user should succeed");

    assert_eq!(user.id, "pg");
    assert_eq!(user.display_name, "pg");
    assert_eq!(user.backend, "hackernews");
}

/// `get_notifications()` returns an empty list.
#[tokio::test]
async fn test_get_notifications_empty() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let notifications = client
        .get_notifications()
        .await
        .expect("get_notifications should succeed");

    assert!(notifications.is_empty(), "HN has no notifications, expected empty list");
}

/// `get_groups()` returns an empty list.
#[tokio::test]
async fn test_get_groups_empty() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let groups = client.get_groups().await.expect("get_groups should succeed");

    assert!(groups.is_empty(), "HN has no groups, expected empty list");
}

/// Story messages have the correct structure.
#[tokio::test]
async fn test_story_message_structure() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let messages = client
        .get_messages("hn-top", MessageQuery { limit: Some(5), ..Default::default() })
        .await
        .expect("get_messages(hn-top) should succeed");

    for msg in &messages {
        assert!(!msg.id.is_empty(), "story message.id must not be empty");
        assert!(!msg.author.id.is_empty(), "story message.author.id must not be empty");
        assert_eq!(
            msg.author.backend, "hackernews",
            "story author.backend must be 'hackernews'"
        );
        let body = match &msg.content {
            poly_client::MessageContent::Text(t) => t.clone(),
            poly_client::MessageContent::WithAttachments { text, .. } => text.clone(),
        };
        assert!(!body.is_empty(), "story body must not be empty");
    }
}

/// Comment messages for a known story have correct structure.
#[tokio::test]
async fn test_comment_message_structure() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let comments = client
        .get_messages(
            "hn-post-1001",
            MessageQuery { limit: Some(10), ..Default::default() },
        )
        .await
        .expect("get_messages(hn-post-1001) should succeed");

    assert_eq!(comments.len(), 2);
    for comment in &comments {
        assert!(!comment.id.is_empty(), "comment.id must not be empty");
        assert!(!comment.author.id.is_empty(), "comment.author.id must not be empty");
        assert_eq!(comment.author.backend, "hackernews");
    }
}

/// `backend_type()` and `backend_name()` return the expected values.
#[tokio::test]
async fn test_backend_identity() {
    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    assert_eq!(client.backend_type(), "hackernews");
    assert_eq!(client.backend_name(), "Hacker News");
}

/// `with_base_url` constructs a client correctly.
#[tokio::test]
async fn test_base_url() {
    let server = TestHnServer::start().await;
    let client = HackerNewsClient::with_base_url(hn_base_url(&server));
    assert_eq!(client.backend_type(), "hackernews");
}

// ---------------------------------------------------------------------------
// Pack C.2 — settings storage round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_settings_storage_round_trip() {
    use poly_client::SettingsScope;
    let client = HackerNewsClient::new();
    client
        .set_setting_value(SettingsScope::AccountGlobal, "", "default-feed", "\"new\"")
        .await
        .expect("set_setting_value should succeed");
    let got = client
        .get_setting_value(SettingsScope::AccountGlobal, "", "default-feed")
        .await
        .expect("get_setting_value should succeed");
    assert_eq!(got, "\"new\"");
}
