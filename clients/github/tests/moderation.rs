//! Moderation tests for poly-github (Wave 2 / Phase B-GH).
//!
//! Covers: get_my_permissions, delete_message (issue + PR comment),
//! and kick_member returning NotSupported.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use poly_client::{AuthCredentials, ClientBackend, ClientError};
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

async fn authenticated_client(base_url: &str, username: &str) -> GitHubClient {
    let http = reqwest::Client::new();
    let resp = http
        .post(format!("{base_url}/test/auth/token"))
        .json(&serde_json::json!({ "username": username }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let token = body["token"].as_str().unwrap().to_string();

    let mut client = GitHubClient::with_http(base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate must succeed");
    client
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `get_my_permissions` for a repo owner returns admin-level permissions.
#[tokio::test]
async fn test_get_my_permissions_admin() {
    let base_url = start_test_server().await;
    let client = authenticated_client(&base_url, "penguin").await;

    // Populate repo cache so resolve_owner_repo_from_server_id works.
    let servers = client.get_servers().await.expect("get_servers must succeed");
    // Find penguin/iceberg-os (repo id 101 → server id "gh-101")
    let iceberg = servers
        .iter()
        .find(|s| s.name == "penguin/iceberg-os")
        .expect("iceberg-os must be in server list");

    let perms = client
        .get_my_permissions(&iceberg.id, None)
        .await
        .expect("get_my_permissions must succeed for admin");

    assert!(perms.manage_server, "admin has manage_server");
    assert!(perms.manage_channels, "admin has manage_channels");
    assert!(perms.manage_roles, "admin has manage_roles");
    assert!(perms.kick_members, "admin has kick_members");
    assert!(perms.ban_members, "admin has ban_members");
    assert!(perms.manage_messages, "admin has manage_messages");
    assert!(!perms.timeout_members, "GitHub has no timeout concept");
    assert_eq!(perms.display_role, "Admin");
    assert!(perms.power_level.is_none());
}

/// `delete_message` with `"comment:{id}"` prefix calls the issue-comment DELETE endpoint.
#[tokio::test]
async fn test_delete_issue_comment() {
    let base_url = start_test_server().await;
    let client = authenticated_client(&base_url, "penguin").await;

    // channel_id must be an issues forum channel so owner/repo can be parsed.
    let channel_id = "gh-issues-penguin-iceberg-os";
    let message_id = "comment:42";

    let result = client.delete_message(channel_id, message_id).await;
    assert!(
        result.is_ok(),
        "delete_message for issue comment must succeed, got: {result:?}"
    );
}

/// `delete_message` with `"pr-comment:{id}"` prefix calls the PR review-comment DELETE endpoint.
#[tokio::test]
async fn test_delete_pr_comment() {
    let base_url = start_test_server().await;
    let client = authenticated_client(&base_url, "penguin").await;

    let channel_id = "gh-pulls-penguin-iceberg-os";
    let message_id = "pr-comment:99";

    let result = client.delete_message(channel_id, message_id).await;
    assert!(
        result.is_ok(),
        "delete_message for PR comment must succeed, got: {result:?}"
    );
}

/// `kick_member` returns `ClientError::NotSupported` on GitHub.
#[tokio::test]
async fn test_kick_returns_not_supported() {
    let base_url = start_test_server().await;
    let client = authenticated_client(&base_url, "penguin").await;

    let result = client
        .kick_member("gh-101", "chameleon", Some("test reason"))
        .await;

    assert!(
        matches!(result, Err(ClientError::NotSupported(_))),
        "kick_member must return NotSupported on GitHub, got: {result:?}"
    );
}
