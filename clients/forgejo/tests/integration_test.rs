//! Integration tests for poly-forgejo.
//!
//! Spins up the mock Forgejo server in-process and exercises the full
//! `ForgejoClient` → `ClientBackend` surface against it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use poly_client::{AuthCredentials, ClientBackend, MessageQuery};
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
    assert_eq!(servers.len(), 2, "otter owns dam-builder and fish-finder");

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

    assert_eq!(channels.len(), 3, "each repo has 3 channels");

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

    let channel_id = "fj-issues-otter-dam-builder";
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

    let channel_id = "fj-pulls-otter-dam-builder";
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

    // Issue thread channel: fj-issue-{owner}-{repo}-{number}
    let channel_id = "fj-issue-otter-dam-builder-1";
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

    let channel_id = "fj-code-otter-dam-builder";
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

    let channel_id = "fj-code-otter-dam-builder";
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
            "fj-issues-otter-dam-builder",
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
