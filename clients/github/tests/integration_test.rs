//! Integration tests for poly-github.
//!
//! Spins up the mock GitHub server in-process and exercises the full
//! `GitHubClient` → `ClientBackend` surface against it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use poly_client::{AuthCredentials, ClientBackend, MessageQuery};
use poly_github::GitHubClient;
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn start_test_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = poly_test_github::router();
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .await
            .unwrap();
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
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);

    let session = client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate should succeed");

    assert_eq!(session.user.id, "penguin");
    assert_eq!(session.backend, "github");
    assert!(client.is_authenticated());
}

/// Authenticate with invalid token → error.
#[tokio::test]
async fn test_authenticate_bad_token() {
    let base_url = start_test_server().await;
    let mut client = GitHubClient::with_http(&base_url);

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
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();

    let servers = client
        .get_servers()
        .await
        .expect("get_servers should succeed");

    assert!(!servers.is_empty(), "penguin should have repos");
    assert!(servers.len() >= 2, "penguin owns at least iceberg-os and fish-tracker");

    let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"penguin/iceberg-os"),
        "iceberg-os should be present"
    );
    assert!(
        names.contains(&"penguin/fish-tracker"),
        "fish-tracker should be present"
    );
}

/// Each repo exposes 4 channels: issues, pull-requests, discussions, code.
#[tokio::test]
async fn test_get_channels() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();

    let servers = client.get_servers().await.unwrap();
    let server = servers
        .iter()
        .find(|s| s.name == "penguin/iceberg-os")
        .expect("iceberg-os should be present");

    let channels = client
        .get_channels(&server.id)
        .await
        .expect("get_channels should succeed");

    assert_eq!(channels.len(), 4, "each repo has 4 channels");

    let channel_names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();
    assert!(channel_names.contains(&"issues"), "issues channel");
    assert!(
        channel_names.contains(&"pull-requests"),
        "pull-requests channel"
    );
    assert!(channel_names.contains(&"discussions"), "discussions channel");
    assert!(channel_names.contains(&"code"), "code channel");
}

/// `get_messages` on the issues channel returns issues (not PRs).
#[tokio::test]
async fn test_get_messages_issues() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let channel_id = "gh-issues-penguin~iceberg-os";
    let messages = client
        .get_messages(channel_id, MessageQuery::default())
        .await
        .expect("get_messages should succeed");

    assert!(!messages.is_empty(), "iceberg-os has issues");
    let all_text = messages
        .iter()
        .map(|m| match &m.content {
            poly_client::MessageContent::Text(t) => t.clone(),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        all_text.contains("Add thermal regulation module"),
        "thermal issue should be present"
    );
    assert!(
        all_text.contains("Memory leak in snowflake allocator"),
        "memory leak issue should be present"
    );
    // PR should NOT appear in issues channel
    assert!(
        !all_text.contains("Implement ice crystal caching"),
        "PR should not appear in issues channel"
    );
}

/// `get_messages` on the pulls channel returns only PRs.
#[tokio::test]
async fn test_get_messages_pulls() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let channel_id = "gh-pulls-penguin~iceberg-os";
    let messages = client
        .get_messages(channel_id, MessageQuery::default())
        .await
        .expect("get_messages should succeed");

    assert!(!messages.is_empty(), "iceberg-os has at least one PR");
    let all_text = messages
        .iter()
        .map(|m| match &m.content {
            poly_client::MessageContent::Text(t) => t.clone(),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        all_text.contains("Implement ice crystal caching"),
        "PR should appear in pulls channel"
    );
}

/// `get_messages` on an issue thread channel returns comments.
#[tokio::test]
async fn test_get_messages_comments() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    // Issue thread channel: gh-issue-{owner}-{repo}-{number}
    let channel_id = "gh-issue-penguin~iceberg-os-1";
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
        all_text.contains("thermal module should integrate with the cooling subsystem"),
        "chameleon's comment should be present"
    );
    assert!(
        all_text.contains("prototype in the next sprint"),
        "penguin's comment should be present"
    );
}

/// `list_files` on the code channel returns the root directory listing.
#[tokio::test]
async fn test_list_files() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let channel_id = "gh-code-penguin~iceberg-os";
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

/// `list_files` on a subdirectory returns the subdirectory listing.
#[tokio::test]
async fn test_list_files_subdir() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let channel_id = "gh-code-penguin~iceberg-os";
    let entries = client
        .list_files(channel_id, "src")
        .await
        .expect("list_files src should succeed");

    assert!(!entries.is_empty(), "src dir should have entries");
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"main.rs"), "main.rs should be listed");
    assert!(names.contains(&"thermal.rs"), "thermal.rs should be listed");
}

/// `read_file` on a known file returns decoded content.
#[tokio::test]
async fn test_read_file() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let channel_id = "gh-code-penguin~iceberg-os";
    let content = client
        .read_file(channel_id, "README.md")
        .await
        .expect("read_file should succeed");

    let text = String::from_utf8(content.bytes).expect("content should be valid UTF-8");
    assert!(
        text.contains("Iceberg OS"),
        "README content should include title"
    );
    assert!(
        text.contains("cold environments"),
        "README content should include description"
    );
}

/// `send_message` returns NotSupported — the github backend is read-only.
#[tokio::test]
async fn test_send_message_not_supported() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();

    let result = client
        .send_message(
            "gh-issues-penguin~iceberg-os",
            poly_client::MessageContent::Text("hello".to_string()),
        )
        .await;

    match result {
        Err(poly_client::ClientError::NotSupported(_)) => {}
        other => panic!("expected NotSupported, got {:?}", other),
    }
}

/// `backend_type` is `"github"` and `backend_name` is `"GitHub"`.
#[tokio::test]
async fn test_backend_identity() {
    let base_url = start_test_server().await;
    let client = GitHubClient::with_http(&base_url);

    assert_eq!(
        client.backend_type(),
        poly_client::BackendType::from("github")
    );
    assert_eq!(client.backend_name(), "GitHub");
}

/// Unauthenticated `get_servers` → error.
#[tokio::test]
async fn test_unauthenticated_fails() {
    let base_url = start_test_server().await;
    let client = GitHubClient::with_http(&base_url);

    assert!(!client.is_authenticated());
    let result = client.get_servers().await;
    assert!(result.is_err(), "unauthenticated get_servers must fail");
}

/// `logout` clears session; subsequent calls fail.
#[tokio::test]
async fn test_logout() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();

    assert!(client.is_authenticated());
    client.logout().await.expect("logout should succeed");
    assert!(
        !client.is_authenticated(),
        "should not be authenticated after logout"
    );

    let result = client.get_servers().await;
    assert!(result.is_err(), "get_servers after logout must fail");
}

// ---------------------------------------------------------------------------
// Pack E.3 — get_view_rows + get_view_detail + state-aware menu
// ---------------------------------------------------------------------------

/// `get_view_rows` with tab "issues" returns only real issues, not PRs.
#[tokio::test]
async fn test_get_view_rows_issues_tab() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let channel_id = "gh-issues-penguin~iceberg-os";
    let page = client
        .get_view_rows(channel_id, None, None, None, Some("issues"))
        .await
        .expect("get_view_rows should succeed");

    assert!(!page.rows.is_empty(), "should return issue rows");
    // No PR should appear in the issues tab
    for row in &page.rows {
        assert!(
            !row.primary_text.contains("ice crystal caching"),
            "PR should not appear in issues tab"
        );
    }
    // Issues should be present
    let titles: Vec<&str> = page.rows.iter().map(|r| r.primary_text.as_str()).collect();
    assert!(
        titles.contains(&"Add thermal regulation module"),
        "thermal issue should appear"
    );
    assert!(
        titles.contains(&"Memory leak in snowflake allocator"),
        "memory leak issue should appear"
    );
}

/// `get_view_rows` with tab "pulls" returns only PRs.
#[tokio::test]
async fn test_get_view_rows_pulls_tab() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let channel_id = "gh-pulls-penguin~iceberg-os";
    let page = client
        .get_view_rows(channel_id, None, None, None, Some("pulls"))
        .await
        .expect("get_view_rows pulls should succeed");

    assert!(!page.rows.is_empty(), "should return PR rows");
    assert_eq!(page.rows.len(), 1, "iceberg-os has one PR");
    assert_eq!(page.rows[0].primary_text, "Implement ice crystal caching");
    assert_eq!(page.rows[0].badge, Some("open".to_string()));
}

/// `get_view_rows` with tab "discussions" returns empty.
#[tokio::test]
async fn test_get_view_rows_discussions_tab_empty() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let page = client
        .get_view_rows(
            "gh-issues-penguin~iceberg-os",
            None,
            None,
            None,
            Some("discussions"),
        )
        .await
        .expect("discussions tab should return empty page, not error");

    assert!(page.rows.is_empty(), "discussions tab must return empty rows");
}

/// ViewRow fields follow the expected shape.
#[tokio::test]
async fn test_get_view_rows_row_fields() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let page = client
        .get_view_rows(
            "gh-issues-penguin~iceberg-os",
            None,
            None,
            None,
            Some("issues"),
        )
        .await
        .unwrap();

    let row = page
        .rows
        .iter()
        .find(|r| r.primary_text == "Add thermal regulation module")
        .expect("thermal issue row must be present");

    // id = issue number
    assert_eq!(row.id, "1");
    // secondary_text contains "#1 by penguin"
    let sec = row.secondary_text.as_deref().unwrap_or("");
    assert!(sec.contains("#1"), "secondary_text should contain #1");
    assert!(sec.contains("penguin"), "secondary_text should contain author");
    // meta_text contains score + comments
    let meta = row.meta_text.as_deref().unwrap_or("");
    assert!(meta.contains("SCORE:5"), "meta_text should contain SCORE:5");
    assert!(meta.contains("2 comments"), "meta_text should contain comment count");
    // badge = state
    assert_eq!(row.badge, Some("open".to_string()));
}

/// `get_view_detail` returns a CustomBlock with the issue body.
#[tokio::test]
async fn test_get_view_detail() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let detail = client
        .get_view_detail("gh-issues-penguin~iceberg-os", "1")
        .await
        .expect("get_view_detail should succeed");

    assert!(
        detail.body_block.sanitized_html.contains("heat management"),
        "detail body should contain issue body text"
    );
    // Issue #1 has 2 comments → comments_section should be Some
    assert!(
        detail.comments_section.is_some(),
        "issue with comments should have comments_section"
    );
}

/// `get_view_detail` for an issue with no comments returns None comments_section.
#[tokio::test]
async fn test_get_view_detail_no_comments() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    // PR #3 has 0 comments
    let detail = client
        .get_view_detail("gh-issues-penguin~iceberg-os", "3")
        .await
        .expect("get_view_detail for PR should succeed");

    assert!(
        detail.comments_section.is_none(),
        "item with 0 comments should have no comments_section"
    );
}

/// `get_context_menu_items` shows "unstar" when repo is already starred.
#[tokio::test]
async fn test_context_menu_star_state_aware() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    // Populate repos cache so server_id → owner/repo lookup works
    client.get_servers().await.unwrap();

    // Find the server ID for penguin/iceberg-os
    let servers = client.get_servers().await.unwrap();
    let iceberg = servers
        .iter()
        .find(|s| s.name == "penguin/iceberg-os")
        .expect("iceberg-os must be present");

    let items = client
        .get_context_menu_items(poly_client::MenuTargetKind::Server, &iceberg.id)
        .await
        .expect("get_context_menu_items should succeed");

    let star_item = items
        .iter()
        .find(|i| i.id == "star-repo")
        .expect("star-repo item must be present");

    // penguin has starred iceberg-os in seeded data
    assert_eq!(
        star_item.label_key,
        "plugin-github-menu-unstar-repo-label",
        "starred repo should show unstar label"
    );
}

/// `get_context_menu_items` shows "star" when repo is not starred.
#[tokio::test]
async fn test_context_menu_unstarred_shows_star() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let servers = client.get_servers().await.unwrap();
    let fish = servers
        .iter()
        .find(|s| s.name == "penguin/fish-tracker")
        .expect("fish-tracker must be present");

    let items = client
        .get_context_menu_items(poly_client::MenuTargetKind::Server, &fish.id)
        .await
        .expect("get_context_menu_items should succeed");

    let star_item = items
        .iter()
        .find(|i| i.id == "star-repo")
        .expect("star-repo item must be present");

    assert_eq!(
        star_item.label_key,
        "plugin-github-menu-star-repo-label",
        "unstarred repo should show star label"
    );
}

// ---------------------------------------------------------------------------
// F-GH-1 — repo card metadata fields
// ---------------------------------------------------------------------------

/// `get_servers` populates description / star_count / language / updated_at
/// so the UI can render rich repo cards.
#[tokio::test]
async fn test_get_servers_repo_card_fields() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();

    let servers = client.get_servers().await.expect("get_servers should succeed");

    let iceberg = servers
        .iter()
        .find(|s| s.name == "penguin/iceberg-os")
        .expect("iceberg-os must be present");

    // description
    assert_eq!(
        iceberg.description.as_deref(),
        Some("An operating system designed for extremely cold environments"),
        "description should be populated from repo"
    );

    // star_count (iceberg-os is seeded with 42 stars)
    assert_eq!(
        iceberg.star_count,
        Some(42),
        "star_count should be 42 for iceberg-os"
    );

    // language
    assert_eq!(
        iceberg.language.as_deref(),
        Some("Rust"),
        "language should be Rust for iceberg-os"
    );

    // forks_count
    assert!(
        iceberg.forks_count.is_some(),
        "forks_count should be populated"
    );

    // open_issues_count
    assert!(
        iceberg.open_issues_count.is_some(),
        "open_issues_count should be populated"
    );

    // fish-tracker has different language/stars
    let fish = servers
        .iter()
        .find(|s| s.name == "penguin/fish-tracker")
        .expect("fish-tracker must be present");
    assert_eq!(fish.language.as_deref(), Some("Python"));
    assert_eq!(fish.star_count, Some(7));
}

// ---------------------------------------------------------------------------
// Phase 2 — get_account_overview_view + overview get_view_rows
// ---------------------------------------------------------------------------

/// `get_account_overview_view` returns a CardGrid with correct header keys.
#[tokio::test]
async fn test_get_account_overview_view_descriptor() {
    use poly_client::{ClientBackend, ViewBody, ViewKind};
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();

    let desc = client
        .get_account_overview_view()
        .await
        .expect("get_account_overview_view should succeed");

    assert_eq!(desc.kind, ViewKind::CardGrid);
    let header = desc.header.expect("overview descriptor must have a header");
    assert_eq!(
        header.title_key.as_deref(),
        Some("plugin-github-overview-title"),
        "title_key must be plugin-github-overview-title"
    );
    assert_eq!(
        header.subtitle_key.as_deref(),
        Some("plugin-github-overview-subtitle"),
        "subtitle_key must be plugin-github-overview-subtitle"
    );
    assert!(desc.toolbar.is_none(), "overview has no toolbar");
    match desc.body {
        ViewBody::CardBody(spec) => assert_eq!(spec.primary_field, "name"),
        other => panic!("expected CardBody, got {other:?}"),
    }
}

/// `get_view_rows` with empty channel_id returns one card per cached repo.
#[tokio::test]
async fn test_get_view_rows_overview_returns_repo_cards() {
    use poly_client::ClientBackend;
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    // Populate repos cache
    client.get_servers().await.unwrap();

    let page = client
        .get_view_rows("", None, None, None, None)
        .await
        .expect("get_view_rows overview should succeed");

    assert!(page.rows.len() >= 2, "penguin has at least 2 repos");
    assert!(page.next_cursor.is_none(), "single-page overview has no cursor");

    let iceberg = page
        .rows
        .iter()
        .find(|r| r.primary_text == "penguin/iceberg-os")
        .expect("iceberg-os card must be present");

    // secondary_text is the repo description
    assert_eq!(
        iceberg.secondary_text.as_deref(),
        Some("An operating system designed for extremely cold environments"),
        "secondary_text should be the repo description"
    );

    // meta_text contains stars, forks, open issues
    let meta = iceberg.meta_text.as_deref().unwrap_or("");
    assert!(meta.contains("★ 42"), "meta_text must contain star count");
    assert!(meta.contains("forks"), "meta_text must contain forks");
    assert!(meta.contains("open"), "meta_text must contain open issues label");

    // badge is the primary language
    assert_eq!(
        iceberg.badge.as_deref(),
        Some("Rust"),
        "badge should be the repo language"
    );

    // context_menu_target_kind is Server
    assert_eq!(
        iceberg.context_menu_target_kind,
        poly_client::MenuTargetKind::Server,
        "context_menu_target_kind must be Server"
    );
}

/// `get_view_rows` overview with no cached repos returns an empty page (not an error).
#[tokio::test]
async fn test_get_view_rows_overview_empty_cache() {
    use poly_client::ClientBackend;
    let base_url = start_test_server().await;
    // Create client but do NOT call get_servers — cache stays empty.
    let client = GitHubClient::with_http(&base_url);

    let page = client
        .get_view_rows("", None, None, None, None)
        .await
        .expect("empty overview should succeed");

    assert!(page.rows.is_empty(), "no cached repos → empty page");
}

// ---------------------------------------------------------------------------
// Pack C.2 — settings storage round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_settings_storage_round_trip() {
    use poly_client::SettingsScope;
    let client = GitHubClient::dotcom();
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
// PR detail + Discussions detail (follow-up gaps — visual-github.md audit)
// ---------------------------------------------------------------------------

/// `get_view_detail` on a pulls channel returns the PR body.
#[tokio::test]
async fn test_get_view_detail_pull_request() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    // PR #3 in penguin/iceberg-os is "Implement ice crystal caching"
    let detail = client
        .get_view_detail("gh-pulls-penguin~iceberg-os", "3")
        .await
        .expect("get_view_detail for PR should succeed");

    assert!(
        detail.body_block.sanitized_html.contains("ice crystal"),
        "PR body should be present in detail view"
    );
    // PR #3 has 0 comments
    assert!(
        detail.comments_section.is_none(),
        "PR with 0 comments should have no comments_section"
    );
}

/// `get_view_rows` on a `gh-discussions-*` channel returns rows via GraphQL.
///
/// The mock server returns an empty discussions list for any GraphQL query,
/// so this test verifies the call succeeds and returns an empty page.
#[tokio::test]
async fn test_get_view_rows_discussions_channel() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let page = client
        .get_view_rows("gh-discussions-penguin~iceberg-os", None, None, None, None)
        .await
        .expect("get_view_rows for discussions channel should succeed (empty)");

    // Mock server returns empty discussions.
    assert!(
        page.rows.is_empty(),
        "mock server returns empty discussions; expected no rows"
    );
}

/// `get_view_detail` on a discussions channel returns NotSupported.
///
/// GitHub discussions use GraphQL and have a separate number space from
/// issues/PRs. The backend explicitly gates this with NotSupported so the
/// UI can show an "open in browser" fallback instead of a wrong detail pane.
#[tokio::test]
async fn test_get_view_detail_discussions_returns_not_supported() {
    let base_url = start_test_server().await;
    let token = get_test_token(&base_url, "penguin").await;
    let mut client = GitHubClient::with_http(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .unwrap();
    client.get_servers().await.unwrap();

    let result = client
        .get_view_detail("gh-discussions-penguin~iceberg-os", "1")
        .await;

    assert!(
        matches!(result, Err(poly_client::ClientError::NotSupported(_))),
        "discussions detail must return NotSupported; got: {result:?}"
    );
}
