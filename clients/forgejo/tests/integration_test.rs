//! Integration tests for poly-forgejo.
//!
//! Spins up the mock Forgejo server in-process and exercises the full
//! `ForgejoClient` → `ClientBackend` surface against it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use poly_client::{AuthCredentials, ClientBackend, ClientError, MessageQuery};
use poly_forgejo::ForgejoClient;
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn start_test_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = poly_test_forgejo::router();
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service()).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    format!("http://127.0.0.1:{}", port)
}

async fn get_test_token(base_url: &str, username: &str) -> String {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/test/auth/token", base_url))
        .json(&serde_json::json!({ "username": username }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    body["token"].as_str().unwrap().to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Get token via `/test/auth/token` and authenticate with it.
#[tokio::test]
async fn test_authenticate() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);

    let session = client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate should succeed");

    assert!(!session.token.is_empty(), "token must not be empty");
    assert_eq!(session.backend, "forgejo");
    assert!(client.is_authenticated());
}

/// Authenticate with invalid token → error.
#[tokio::test]
async fn test_authenticate_bad_token() {
    let base_url = start_test_server().await;
    let mut client = ForgejoClient::new(&base_url);

    let result = client
        .authenticate(AuthCredentials::Token("totally-invalid-token".to_string()))
        .await;

    assert!(result.is_err(), "bad token must fail");
    assert!(!client.is_authenticated());
}

/// `get_servers` returns repos owned by the authenticated user.
#[tokio::test]
async fn test_get_servers() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();

    let servers = client.get_servers().await.expect("get_servers should succeed");

    assert!(!servers.is_empty(), "otter should have repos");
    assert!(servers.len() >= 2, "otter owns at least dam-builder and fish-finder");

    let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"otter/dam-builder"),
        "dam-builder should be present"
    );
    assert!(
        names.contains(&"otter/fish-finder"),
        "fish-finder should be present"
    );
}

/// Each repo exposes 3 channels: issues, pull-requests, code.
#[tokio::test]
async fn test_get_channels() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();

    let servers = client.get_servers().await.unwrap();
    // Find dam-builder server
    let server = servers
        .iter()
        .find(|s| s.name == "otter/dam-builder")
        .expect("dam-builder should be present");

    let channels = client
        .get_channels(&server.id)
        .await
        .expect("get_channels should succeed");

    assert!(channels.len() >= 3, "each repo has at least 3 channels");

    let channel_names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();
    assert!(channel_names.contains(&"issues"), "issues channel");
    assert!(channel_names.contains(&"pull-requests"), "pull-requests channel");
    assert!(channel_names.contains(&"code"), "code channel");
}

/// `get_messages` on the issues channel returns issue titles.
#[tokio::test]
async fn test_get_messages_issues() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let channel_id = "fj-issues-otter/dam-builder";
    let messages = client
        .get_messages(channel_id, MessageQuery::default())
        .await
        .expect("get_messages should succeed");

    // Issues only (no PRs)
    assert!(!messages.is_empty(), "dam-builder has issues");
    let all_text = messages
        .iter()
        .map(|m| match &m.content {
            poly_client::MessageContent::Text(t) => t.clone(),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        all_text.contains("Support curved dam designs"),
        "curved dam issue should be present"
    );
    assert!(
        all_text.contains("Water pressure calculations are off"),
        "water pressure issue should be present"
    );
    // PR should NOT appear in issues channel
    assert!(
        !all_text.contains("Add beaver collaboration mode"),
        "PR should not appear in issues channel"
    );
}

/// `get_messages` on the pulls channel returns only PRs.
#[tokio::test]
async fn test_get_messages_pulls() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let channel_id = "fj-pulls-otter/dam-builder";
    let messages = client
        .get_messages(channel_id, MessageQuery::default())
        .await
        .expect("get_messages should succeed");

    assert!(!messages.is_empty(), "dam-builder has at least one PR");
    let all_text = messages
        .iter()
        .map(|m| match &m.content {
            poly_client::MessageContent::Text(t) => t.clone(),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        all_text.contains("Add beaver collaboration mode"),
        "PR should appear in pulls channel"
    );
}

/// `get_messages` on an issue thread channel returns comments.
#[tokio::test]
async fn test_get_messages_comments() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    // Issue thread channel: fj-issue-{owner}/{repo}-{number}
    let channel_id = "fj-issue-otter/dam-builder-1";
    let messages = client
        .get_messages(channel_id, MessageQuery::default())
        .await
        .expect("get_messages on issue thread should succeed");

    assert_eq!(messages.len(), 2, "issue #1 has 2 comments");
    let all_text = messages
        .iter()
        .map(|m| match &m.content {
            poly_client::MessageContent::Text(t) => t.clone(),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        all_text.contains("curves would improve water flow"),
        "flamingo's comment should be present"
    );
    assert!(
        all_text.contains("let me prototype something"),
        "otter's comment should be present"
    );
}

/// `list_files` on the code channel returns the root directory listing.
#[tokio::test]
async fn test_list_files() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let channel_id = "fj-code-otter/dam-builder";
    let entries = client
        .list_files(channel_id, "")
        .await
        .expect("list_files should succeed");

    assert!(!entries.is_empty(), "root dir should have entries");
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"README.md"), "README.md should be listed");
    assert!(names.contains(&"src"), "src dir should be listed");
    assert!(names.contains(&"Cargo.toml"), "Cargo.toml should be listed");
}

/// `read_file` on a known file returns decoded content.
#[tokio::test]
async fn test_read_file() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let channel_id = "fj-code-otter/dam-builder";
    let content = client
        .read_file(channel_id, "README.md")
        .await
        .expect("read_file should succeed");

    let text = String::from_utf8(content.bytes).expect("content should be valid UTF-8");
    assert!(text.contains("Dam Builder"), "README content should include title");
    assert!(
        text.contains("aquatic habitats"),
        "README content should include description"
    );
}

/// `send_message` returns NotSupported — the forgejo backend is read-only.
#[tokio::test]
async fn test_send_message_not_supported() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();

    let result = client
        .send_message(
            "fj-issues-otter/dam-builder",
            poly_client::MessageContent::Text("hello".to_string()),
        )
        .await;

    match result {
        Err(poly_client::ClientError::NotSupported(_)) => {}
        other => panic!("expected NotSupported, got {:?}", other),
    }
}

/// `backend_type` is `"forgejo"` and `backend_name` is `"Forgejo"`.
#[tokio::test]
async fn test_backend_identity() {
    let base_url = start_test_server().await;
    let client = ForgejoClient::new(&base_url);

    assert_eq!(client.backend_type(), poly_client::BackendType::from("forgejo"));
    assert_eq!(client.backend_name(), "Forgejo");
}

/// Unauthenticated `get_servers` → error.
#[tokio::test]
async fn test_unauthenticated_fails() {
    let base_url = start_test_server().await;
    let client = ForgejoClient::new(&base_url);

    assert!(!client.is_authenticated());
    let result = client.get_servers().await;
    assert!(result.is_err(), "unauthenticated get_servers must fail");
}

/// `logout` clears session; subsequent calls fail.
#[tokio::test]
async fn test_logout() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();

    assert!(client.is_authenticated());
    client.logout().await.expect("logout should succeed");
    assert!(!client.is_authenticated(), "should not be authenticated after logout");

    let result = client.get_servers().await;
    assert!(result.is_err(), "get_servers after logout must fail");
}

// ---------------------------------------------------------------------------
// Pack C.2 — settings storage round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_settings_storage_round_trip() {
    use poly_client::SettingsScope;
    let client = ForgejoClient::codeberg();
    client
        .set_setting_value(SettingsScope::AccountGlobal, "", "show-private-repos", "false")
        .await
        .expect("set_setting_value should succeed");
    let got = client
        .get_setting_value(SettingsScope::AccountGlobal, "", "show-private-repos")
        .await
        .expect("get_setting_value should succeed");
    assert_eq!(got, "false");
}

// ---------------------------------------------------------------------------
// Pack E.4 — get_view_rows + get_view_detail + state-aware menu
// ---------------------------------------------------------------------------

/// `get_view_rows` on the issues channel returns non-empty ViewRows.
#[tokio::test]
async fn test_get_view_rows_issues() {
    use poly_client::ClientBackend;
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client.authenticate(AuthCredentials::Token(token)).await.unwrap();
    client.get_servers().await.unwrap();

    let page = client
        .get_view_rows("fj-issues-otter/dam-builder", None, None, Some("open"), Some("issues"))
        .await
        .expect("get_view_rows should succeed");

    assert!(!page.rows.is_empty(), "should have issue rows");
    // 2 issues seeded; no PR in issues channel
    assert_eq!(page.rows.len(), 2, "exactly 2 issues");

    let row = &page.rows[0];
    assert!(!row.primary_text.is_empty(), "primary_text must not be empty");
    assert!(
        row.secondary_text.as_deref().unwrap_or("").contains('#'),
        "secondary_text should contain #N"
    );
    assert!(
        row.meta_text.as_deref().unwrap_or("").contains("comments"),
        "meta_text should mention comments"
    );
    assert!(
        row.meta_text.as_deref().unwrap_or("").starts_with("SCORE:"),
        "meta_text should start with SCORE:"
    );
    assert_eq!(
        page.next_cursor, None,
        "fewer than 30 rows: next_cursor should be None"
    );
}

/// `get_view_rows` on the pulls channel returns only PRs.
#[tokio::test]
async fn test_get_view_rows_pulls() {
    use poly_client::ClientBackend;
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client.authenticate(AuthCredentials::Token(token)).await.unwrap();
    client.get_servers().await.unwrap();

    let page = client
        .get_view_rows("fj-pulls-otter/dam-builder", None, None, Some("open"), Some("pulls"))
        .await
        .expect("get_view_rows pulls should succeed");

    assert!(!page.rows.is_empty(), "should have PR rows");
    // 1 PR seeded in dam-builder
    assert_eq!(page.rows.len(), 1, "exactly 1 PR");
    assert!(
        page.rows[0].primary_text.contains("beaver collaboration"),
        "PR title should match"
    );
}

/// `get_view_rows` for discussions tab returns empty (Forgejo has no discussions API).
#[tokio::test]
async fn test_get_view_rows_discussions_empty() {
    use poly_client::ClientBackend;
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client.authenticate(AuthCredentials::Token(token)).await.unwrap();
    client.get_servers().await.unwrap();

    let page = client
        .get_view_rows("fj-issues-otter/dam-builder", None, None, None, Some("discussions"))
        .await
        .expect("discussions tab should succeed (empty)");

    assert!(page.rows.is_empty(), "discussions returns empty for Forgejo");
}

/// `get_view_detail` returns a ViewDetail with a non-empty body block.
#[tokio::test]
async fn test_get_view_detail() {
    use poly_client::ClientBackend;
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client.authenticate(AuthCredentials::Token(token)).await.unwrap();
    client.get_servers().await.unwrap();

    let detail = client
        .get_view_detail("fj-issues-otter/dam-builder", "1")
        .await
        .expect("get_view_detail should succeed");

    assert!(
        !detail.body_block.sanitized_html.is_empty(),
        "body_block should have content"
    );
    assert!(
        detail.body_block.sanitized_html.starts_with("<p"),
        "body should be HTML-wrapped"
    );
    // Issue #1 has 2 comments — comments_section should be Some
    assert!(
        detail.comments_section.is_some(),
        "issue with comments should have comments_section"
    );
    assert_eq!(
        detail.comments_section.unwrap().root_page_size,
        2,
        "comment count should match seeded data"
    );
}

/// `get_view_detail` with an invalid row_id returns an error.
#[tokio::test]
async fn test_get_view_detail_not_found() {
    use poly_client::ClientBackend;
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client.authenticate(AuthCredentials::Token(token)).await.unwrap();
    client.get_servers().await.unwrap();

    let result = client
        .get_view_detail("fj-issues-otter/dam-builder", "9999")
        .await;

    assert!(result.is_err(), "unknown issue number should return error");
}

// ---------------------------------------------------------------------------
// Pack E.4 — unit tests for mapping functions
// ---------------------------------------------------------------------------

/// `map_issue_to_viewrow` produces the correct ViewRow shape from a fixture.
#[test]
fn test_map_issue_to_viewrow_unit() {
    let fixture: Vec<serde_json::Value> = serde_json::from_str(
        include_str!("fixtures/issues.json"),
    )
    .expect("fixture should be valid JSON");

    let issue: poly_forgejo::ForgejoIssue =
        serde_json::from_value(fixture[0].clone()).expect("first issue should deserialize");

    let row = poly_forgejo::map_issue_to_viewrow(&issue);

    assert_eq!(row.id, "1");
    assert_eq!(row.primary_text, "Support curved dam designs");
    assert_eq!(row.secondary_text.as_deref(), Some("#1 by otter"));
    assert!(
        row.meta_text.as_deref().unwrap_or("").starts_with("SCORE:0"),
        "meta should start with SCORE:0"
    );
    assert!(
        row.meta_text.as_deref().unwrap_or("").contains("2 comments"),
        "meta should include comment count"
    );
    assert_eq!(row.badge.as_deref(), Some("open"));
}

/// State-aware menu: unstarred repo shows "star-repo" label key.
#[tokio::test]
async fn test_context_menu_unstarred() {
    use poly_client::{ClientBackend, MenuTargetKind};
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client.authenticate(AuthCredentials::Token(token)).await.unwrap();
    let servers = client.get_servers().await.unwrap();

    // dam-builder is NOT starred by otter
    let server = servers.iter().find(|s| s.name == "otter/dam-builder").unwrap();
    let items = client
        .get_context_menu_items(MenuTargetKind::Server, &server.id)
        .await
        .expect("menu items should succeed");

    let star_item = items.iter().find(|i| i.id == "star-repo").unwrap();
    assert_eq!(
        star_item.label_key,
        "plugin-forgejo-menu-star-repo-label",
        "unstarred repo should show star label"
    );
}

// ---------------------------------------------------------------------------
// Wave 2 / Phase B-FJ — moderation tests
// ---------------------------------------------------------------------------

/// `get_my_permissions` for a repo owner returns `manage_messages = true`.
#[tokio::test]
async fn test_get_my_permissions_admin() {
    use poly_client::ClientBackend;
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client.authenticate(AuthCredentials::Token(token)).await.unwrap();
    let servers = client.get_servers().await.unwrap();

    // otter owns dam-builder → should get admin permissions
    let server = servers.iter().find(|s| s.name == "otter/dam-builder").unwrap();
    let perms = client
        .get_my_permissions(&server.id, None)
        .await
        .expect("get_my_permissions should succeed");

    assert!(perms.manage_messages, "repo owner should have manage_messages");
    assert!(perms.manage_server, "repo owner should have manage_server");
    assert_eq!(perms.display_role, "Admin");
    // Forgejo has no kick/ban/timeout
    assert!(!perms.kick_members);
    assert!(!perms.ban_members);
    assert!(!perms.timeout_members);
}

/// `delete_message` with a `fj-comment-{id}` message ID calls the API correctly.
#[tokio::test]
async fn test_delete_comment_via_message_id_prefix() {
    use poly_client::ClientBackend;
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client.authenticate(AuthCredentials::Token(token)).await.unwrap();
    client.get_servers().await.unwrap();

    // Comment 1001 is on issue #1 of otter/dam-builder
    // channel_id = fj-issue-otter/dam-builder-1, message_id = fj-comment-1001
    let result = client
        .delete_message("fj-issue-otter/dam-builder-1", "fj-comment-1001")
        .await;

    assert!(result.is_ok(), "delete_message should succeed for owner: {:?}", result);

    // Confirm comment is no longer returned
    let messages = client
        .get_messages("fj-issue-otter/dam-builder-1", MessageQuery::default())
        .await
        .expect("get_messages should still work after delete");
    assert_eq!(messages.len(), 1, "one comment should remain after deleting comment 1001");
}

/// `kick_member` returns NotSupported.
#[tokio::test]
async fn test_kick_member_returns_not_supported() {
    use poly_client::ClientBackend;
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client.authenticate(AuthCredentials::Token(token)).await.unwrap();
    let servers = client.get_servers().await.unwrap();

    let server = servers.iter().find(|s| s.name == "otter/dam-builder").unwrap();
    let result = client.kick_member(&server.id, "flamingo", None).await;

    match result {
        Err(ClientError::NotSupported(_)) => {}
        other => panic!("expected NotSupported, got {:?}", other),
    }
}

/// `get_account_overview_view` returns a CardGrid descriptor.
#[tokio::test]
async fn test_get_account_overview_view() {
    use poly_client::{ClientBackend, ViewBody};
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client.authenticate(AuthCredentials::Token(token)).await.unwrap();

    let descriptor = client
        .get_account_overview_view()
        .await
        .expect("get_account_overview_view should succeed");

    assert!(
        matches!(descriptor.body, ViewBody::CardBody(_)),
        "overview body must be CardBody"
    );
    assert!(descriptor.header.is_some(), "header should be set");
    let header = descriptor.header.unwrap();
    assert_eq!(
        header.title_key.as_deref(),
        Some("plugin-forgejo-overview-title"),
        "title_key must match FTL key"
    );
}

/// `get_view_rows` on the overview channel returns one row per cached repo
/// with stars/forks/open-issues in meta_text.
#[tokio::test]
async fn test_get_view_rows_overview() {
    use poly_client::ClientBackend;
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client.authenticate(AuthCredentials::Token(token)).await.unwrap();
    // Populate repo cache
    client.get_servers().await.unwrap();

    let page = client
        .get_view_rows("fj-overview", None, None, None, None)
        .await
        .expect("get_view_rows overview should succeed");

    // otter owns at least 2 repos (dam-builder + fish-finder)
    assert!(page.rows.len() >= 2, "otter has at least 2 repos");

    for row in &page.rows {
        assert!(!row.primary_text.is_empty(), "primary_text (repo name) must not be empty");
        let meta = row.meta_text.as_deref().unwrap_or("");
        assert!(meta.contains("open issues"), "meta should include open issues count");
        assert!(meta.contains('·'), "meta should have separator between stats");
    }
    assert_eq!(page.next_cursor, None, "2 repos fits in one page");
}

/// State-aware menu: starred repo shows "unstar-repo" label key.
#[tokio::test]
async fn test_context_menu_starred() {
    use poly_client::{ClientBackend, MenuTargetKind};
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client.authenticate(AuthCredentials::Token(token)).await.unwrap();
    let servers = client.get_servers().await.unwrap();

    // fish-finder IS starred by otter
    let server = servers.iter().find(|s| s.name == "otter/fish-finder").unwrap();
    let items = client
        .get_context_menu_items(MenuTargetKind::Server, &server.id)
        .await
        .expect("menu items should succeed");

    let star_item = items.iter().find(|i| i.id == "star-repo").unwrap();
    assert_eq!(
        star_item.label_key,
        "plugin-forgejo-menu-unstar-repo-label",
        "starred repo should show unstar label"
    );
}
