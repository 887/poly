//! Integration tests for the `poly-hackernews` client.
//!
//! Spins up a `poly-test-hackernews` server in-process and exercises every
//! `ClientBackend` method that `HackerNewsClient` implements.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]


use poly_client::{
    IsBackend, SocialGraphBackend, DmsAndGroupsBackend, AuthCredentials, MessageQuery, ViewBody, ViewKind, CursorKind, Cursor,
};
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
// Pack E.2 — get_view_rows integration tests (§1.2 layer c)
// ---------------------------------------------------------------------------

/// `get_view_rows("hn-top", ...)` returns non-empty rows with correct fields.
#[tokio::test]
async fn test_get_view_rows_top() {
    use poly_client::IsBackend;

    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let page = client
        .get_view_rows("hn-top", None, None, None, None)
        .await
        .expect("get_view_rows(hn-top) should succeed");

    assert!(!page.rows.is_empty(), "expected at least one row");
    for row in &page.rows {
        assert!(!row.id.is_empty(), "row.id must not be empty");
        assert!(!row.primary_text.is_empty(), "row.primary_text must not be empty");
        // secondary_text is Some (url or "by <author>")
        assert!(row.secondary_text.is_some(), "row.secondary_text must be Some");
        // meta contains score + comments + age
        let meta = row.meta_text.as_deref().expect("meta_text must be Some");
        assert!(meta.contains("pt"), "meta must contain score: {meta}");
        assert!(meta.contains("comments"), "meta must mention comments: {meta}");
    }
}

/// All 6 feed channels produce non-empty ViewRow pages.
#[tokio::test]
async fn test_get_view_rows_all_feeds() {
    use poly_client::IsBackend;

    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let feeds = ["hn-top", "hn-new", "hn-best", "hn-ask", "hn-show", "hn-jobs-ch"];
    for feed_id in &feeds {
        let page = client
            .get_view_rows(feed_id, None, None, None, None)
            .await
            .unwrap_or_else(|e| panic!("get_view_rows({feed_id}) failed: {e:?}"));
        assert!(!page.rows.is_empty(), "feed '{feed_id}' should have at least one row");
    }
}

/// Cursor pagination: second page starts where first page ended.
#[tokio::test]
async fn test_get_view_rows_cursor_pagination() {
    

    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    // Seed has 4 story items for the top feed; page_size is 30 so we get all on page 1.
    // To test pagination, we'd need a seed with >30 items. Instead verify that a cursor
    // with offset=2 returns from position 2 onward and row ids differ from offset=0.
    let page0 = client
        .get_view_rows("hn-top", None, None, None, None)
        .await
        .expect("page 0 should succeed");

    let page1 = client
        .get_view_rows(
            "hn-top",
            Some(Cursor { kind: CursorKind::Offset, value: "2".to_string() }),
            None,
            None,
            None,
        )
        .await
        .expect("page 1 should succeed");

    // page0 has all 4 rows; page1 starts at index 2 so has 2 rows
    assert!(page0.rows.len() >= page1.rows.len(), "page starting at offset 2 should have fewer or equal rows");
    // The first row of page1 should match page0[2]
    if !page1.rows.is_empty() && page0.rows.len() > 2 {
        assert_eq!(page1.rows[0].id, page0.rows[2].id, "offset cursor must skip correctly");
    }
}

/// Unknown channel ID returns a NotFound error.
#[tokio::test]
async fn test_get_view_rows_unknown_channel() {
    use poly_client::IsBackend;

    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let result = client
        .get_view_rows("hn-unknown-feed", None, None, None, None)
        .await;
    assert!(result.is_err(), "unknown channel should return error");
}

// ---------------------------------------------------------------------------
// Pack E.2 — get_view_detail integration tests (§1.2 layer c)
// ---------------------------------------------------------------------------

/// `get_view_detail` for a story with a URL returns a body block with the link.
#[tokio::test]
async fn test_get_view_detail_url_story() {
    use poly_client::IsBackend;

    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    // Story 1001 has url "https://example.com/new-internet" and kids [2001, 2002].
    let detail = client
        .get_view_detail("hn-top", "1001")
        .await
        .expect("get_view_detail(1001) should succeed");

    assert!(
        !detail.body_block.sanitized_html.is_empty(),
        "body_block must not be empty"
    );
    assert!(
        detail.body_block.sanitized_html.contains("example.com"),
        "body_block should reference the story URL"
    );
    // Story 1001 has 2 kids → comments_section should be Some
    assert!(
        detail.comments_section.is_some(),
        "story with kids must have comments_section"
    );
}

/// `get_view_detail` for an Ask HN story returns its text body.
#[tokio::test]
async fn test_get_view_detail_text_story() {
    use poly_client::IsBackend;

    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    // Story 1002 has text content and no URL.
    let detail = client
        .get_view_detail("hn-top", "1002")
        .await
        .expect("get_view_detail(1002) should succeed");

    assert!(
        !detail.body_block.sanitized_html.is_empty(),
        "body_block must not be empty"
    );
    assert!(
        detail.body_block.sanitized_html.contains("<p>"),
        "body should be wrapped in <p>"
    );
}

/// `get_view_detail` for a story with no kids returns None for comments_section.
#[tokio::test]
async fn test_get_view_detail_no_kids() {
    use poly_client::IsBackend;

    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    // Story 1003 has empty kids list.
    let detail = client
        .get_view_detail("hn-top", "1003")
        .await
        .expect("get_view_detail(1003) should succeed");

    assert!(
        detail.comments_section.is_none(),
        "story with no kids must have no comments_section"
    );
}

/// `get_view_detail` with an invalid row id returns an error.
#[tokio::test]
async fn test_get_view_detail_invalid_id() {
    use poly_client::IsBackend;

    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let result = client.get_view_detail("hn-top", "not-a-number").await;
    assert!(result.is_err(), "invalid id should return error");
}

// ---------------------------------------------------------------------------
// get_account_overview_view + overview get_view_rows tests
// ---------------------------------------------------------------------------

/// `get_account_overview_view` returns a FlatList ListBody descriptor.
#[tokio::test]
async fn test_get_account_overview_view_returns_list_body() {
    

    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let desc = client
        .get_account_overview_view()
        .await
        .expect("get_account_overview_view should succeed");

    assert_eq!(desc.kind, ViewKind::FlatList, "overview kind must be FlatList");
    assert!(desc.header.is_some(), "overview must have a header");
    let header = desc.header.unwrap();
    assert!(header.title_key.is_some(), "overview header must have a title_key");

    match desc.body {
        ViewBody::ListBody(spec) => {
            assert!(spec.page_size > 0, "page_size must be positive");
            assert!(!spec.row_template.primary_field.is_empty(), "primary_field must not be empty");
        }
        other => panic!("expected ListBody, got {other:?}"),
    }
}

/// `get_view_rows("")` (overview) returns non-empty rows with the overview row format.
#[tokio::test]
async fn test_get_view_rows_overview_empty_channel_id() {
    use poly_client::IsBackend;

    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let page = client
        .get_view_rows("", None, None, None, None)
        .await
        .expect("get_view_rows('') for overview should succeed");

    assert!(!page.rows.is_empty(), "overview should have at least one row");

    for row in &page.rows {
        assert!(!row.id.is_empty(), "row.id must not be empty");
        assert!(!row.primary_text.is_empty(), "row.primary_text (title) must not be empty");

        // secondary_text = "author · domain" or just "author"
        let secondary = row.secondary_text.as_deref().expect("secondary_text must be Some for overview");
        assert!(!secondary.is_empty(), "secondary_text must not be empty");

        // meta_text = "N points · M comments · age"
        let meta = row.meta_text.as_deref().expect("meta_text must be Some for overview");
        assert!(meta.contains("points"), "meta must contain 'points': {meta}");
        assert!(meta.contains("comments"), "meta must contain 'comments': {meta}");
    }
}

/// Overview secondary_text includes domain when story has a URL.
#[tokio::test]
async fn test_overview_row_secondary_includes_domain() {
    use poly_client::IsBackend;

    let server = TestHnServer::start().await;
    let client = client_connected_to(&server).await;

    let page = client
        .get_view_rows("", None, None, None, None)
        .await
        .expect("get_view_rows('') should succeed");

    // Story 1001 in seed data has url "https://example.com/new-internet".
    // Its secondary_text should contain the domain "example.com" and the author.
    let row_1001 = page.rows.iter().find(|r| r.id == "1001");
    if let Some(row) = row_1001 {
        let secondary = row.secondary_text.as_deref().unwrap_or("");
        assert!(
            secondary.contains("example.com"),
            "secondary_text for a URL story must include the domain: {secondary}"
        );
    }
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
