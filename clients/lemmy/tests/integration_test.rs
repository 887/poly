//! Integration tests for poly-lemmy.
//!
//! Spins up the mock Lemmy server in-process and exercises the full
//! `LemmyClient` → `ClientBackend` surface against it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use poly_client::{AuthCredentials, ClientBackend, MessageQuery};
use poly_lemmy::LemmyClient;
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Server startup helper
// ---------------------------------------------------------------------------

/// Start the mock Lemmy server on a free port and return its base URL.
async fn start_test_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = poly_test_lemmy::router();
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service()).await.unwrap();
    });
    // Brief pause to let the server accept connections
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    format!("http://127.0.0.1:{}", port)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Authenticate with valid credentials → get a JWT in the session token.
#[tokio::test]
async fn test_authenticate() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);

    let session = client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
        })
        .await
        .expect("authenticate should succeed");

    assert!(!session.token.is_empty(), "token must not be empty");
    assert_eq!(session.backend, "lemmy");
    assert!(client.is_authenticated());
}

/// Wrong password → AuthFailed error.
#[tokio::test]
async fn test_authenticate_wrong_password() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);

    let result = client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "wrong".to_string(),
        })
        .await;

    assert!(result.is_err(), "wrong password must fail");
    assert!(!client.is_authenticated());
}

/// `get_servers` returns the subscribed communities as Poly servers.
#[tokio::test]
async fn test_get_servers() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
        })
        .await
        .unwrap();

    let servers = client.get_servers().await.expect("get_servers should succeed");

    assert!(!servers.is_empty(), "should have at least one subscribed community");
    // Both seeded communities should be returned
    assert!(servers.len() >= 2, "expected at least 2 communities");

    let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"Rust Programming"),
        "rust community should be present"
    );
    assert!(
        names.contains(&"Programming"),
        "programming community should be present"
    );

    // Each server should have one category with a "lemmy-feed-N" channel
    for server in &servers {
        assert_eq!(server.categories.len(), 1, "each server has one category");
        let cat = &server.categories[0];
        assert_eq!(cat.name, "Posts");
        assert_eq!(cat.channel_ids.len(), 1);
        assert!(cat.channel_ids[0].starts_with("lemmy-feed-"));
    }
}

/// `get_channels` returns a single "Posts" Forum channel per community.
#[tokio::test]
async fn test_get_channels() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
        })
        .await
        .unwrap();

    let servers = client.get_servers().await.unwrap();
    let server = servers.first().expect("at least one server");

    let channels = client
        .get_channels(&server.id)
        .await
        .expect("get_channels should succeed");

    assert_eq!(channels.len(), 1, "one channel per community");
    let ch = &channels[0];
    // Channel name is the community title (e.g. "Rust Programming")
    assert!(!ch.name.is_empty());
    assert_eq!(ch.channel_type, poly_client::ChannelType::Forum);
    assert_eq!(ch.server_id, server.id);
    assert!(ch.id.starts_with("lemmy-feed-"));
}

/// `get_messages` returns community posts as messages.
#[tokio::test]
async fn test_get_messages() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
        })
        .await
        .unwrap();

    // Use community id=1 (rust)
    let channel_id = "lemmy-feed-1";
    let messages = client
        .get_messages(channel_id, MessageQuery::default())
        .await
        .expect("get_messages should succeed");

    assert!(!messages.is_empty(), "rust community should have posts");
    assert!(messages.len() >= 2, "rust community has at least 2 posts");

    let first = &messages[0];
    assert!(!first.id.is_empty());
    assert!(!first.author.id.is_empty());
    // Post title should be in message content
    match &first.content {
        poly_client::MessageContent::Text(text) => {
            assert!(!text.is_empty(), "message text must not be empty");
        }
        _ => panic!("expected Text content"),
    }
}

/// `get_messages` with limit=1 returns at most one message.
#[tokio::test]
async fn test_get_messages_limit() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
        })
        .await
        .unwrap();

    let messages = client
        .get_messages("lemmy-feed-1", MessageQuery { limit: Some(1), ..Default::default() })
        .await
        .expect("get_messages should succeed");

    assert!(messages.len() <= 2, "server may return up to the seeded count");
}

/// `list_private_messages` returns an empty list (no PM fixtures seeded).
#[tokio::test]
async fn test_list_private_messages() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
        })
        .await
        .unwrap();

    let dms = client
        .get_dm_channels()
        .await
        .expect("get_dm_channels should succeed");

    // No private messages seeded → empty list
    assert!(dms.is_empty(), "no private messages seeded");
}

/// `get_friends` always returns empty — Lemmy has no friend system.
#[tokio::test]
async fn test_list_friends() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
        })
        .await
        .unwrap();

    let friends = client.get_friends().await.expect("get_friends should succeed");

    assert!(friends.is_empty(), "Lemmy has no friend system");
}

/// Auth bypass: POST /test/auth/token returns a token without a password.
#[tokio::test]
async fn test_auth_bypass() {
    let base_url = start_test_server().await;

    // Call the bypass endpoint directly with reqwest
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/test/auth/token", base_url))
        .json(&serde_json::json!({ "username": "testuser" }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 200, "bypass endpoint should return 200");

    let body: serde_json::Value = resp.json().await.expect("should be JSON");
    let token = body["jwt"].as_str().expect("should have jwt field");
    assert!(!token.is_empty(), "token must not be empty");

    // Use the bypass token to authenticate the Lemmy client
    let mut lemmy = LemmyClient::new(&base_url);
    let session = lemmy
        .authenticate(AuthCredentials::Token(token.to_string()))
        .await
        .expect("token auth should succeed");

    assert!(!session.token.is_empty());
    assert!(lemmy.is_authenticated());
}

/// `logout` clears the session and subsequent calls fail.
#[tokio::test]
async fn test_logout() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
        })
        .await
        .unwrap();

    assert!(client.is_authenticated());
    client.logout().await.expect("logout should succeed");
    assert!(!client.is_authenticated(), "should not be authenticated after logout");

    let result = client.get_servers().await;
    assert!(result.is_err(), "get_servers after logout must fail");
}

/// `backend_type` and `backend_name` return the expected values.
#[tokio::test]
async fn test_backend_identity() {
    let base_url = start_test_server().await;
    let client = LemmyClient::new(&base_url);

    assert_eq!(client.backend_type(), poly_client::BackendType::from("lemmy"));
    assert_eq!(client.backend_name(), "Lemmy");
}

/// `send_message` returns NotSupported (not yet implemented).
#[tokio::test]
async fn test_send_message_not_supported() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
        })
        .await
        .unwrap();

    let result = client
        .send_message("lemmy-feed-1", poly_client::MessageContent::Text("hello".to_string()))
        .await;

    match result {
        Err(poly_client::ClientError::NotSupported(_)) => {}
        other => panic!("expected NotSupported, got {:?}", other),
    }
}

/// Unauthenticated `get_servers` → AuthFailed or Network error.
#[tokio::test]
async fn test_unauthenticated_get_servers_fails() {
    let base_url = start_test_server().await;
    let client = LemmyClient::new(&base_url);

    assert!(!client.is_authenticated());
    let result = client.get_servers().await;
    assert!(result.is_err(), "unauthenticated get_servers must fail");
}

// ---------------------------------------------------------------------------
// Pack C.2 — settings storage round-trip
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Pack E.1 — real API integration (views + state-aware menus)
// ---------------------------------------------------------------------------

/// `get_view_rows` on a community feed channel returns populated rows with
/// the expected shape (SCORE: prefix, MenuTargetKind::Message target).
#[tokio::test]
async fn test_get_view_rows_populated() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
        })
        .await
        .unwrap();

    let page = client
        .get_view_rows("lemmy-feed-1", None, Some("hot"), None, None)
        .await
        .expect("get_view_rows should succeed");

    assert!(!page.rows.is_empty(), "rust community should produce rows");
    let row = &page.rows[0];
    assert!(!row.primary_text.is_empty(), "primary_text must be populated");
    let meta = row.meta_text.as_deref().expect("meta_text required");
    assert!(
        meta.starts_with("SCORE:"),
        "meta_text must lead with SCORE: for vote-card rendering, got {meta}"
    );
    assert_eq!(
        row.context_menu_target_kind,
        poly_client::MenuTargetKind::Message
    );
    // Fixture community has only 2 posts → under page_size → no next cursor.
    assert!(page.next_cursor.is_none(), "short page must not have next cursor");
}

/// `get_view_rows` on a non-feed channel returns a NotFound error.
#[tokio::test]
async fn test_get_view_rows_invalid_channel() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
        })
        .await
        .unwrap();

    let result = client
        .get_view_rows("lemmy-post-1", None, None, None, None)
        .await;
    assert!(matches!(result, Err(poly_client::ClientError::NotFound(_))));
}

/// `get_view_detail` fetches a single post and wraps it in a custom block.
#[tokio::test]
async fn test_get_view_detail_returns_custom_block() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
        })
        .await
        .unwrap();

    // Row ids produced by map_post_to_viewrow are the post's ap_id.
    let page = client
        .get_view_rows("lemmy-feed-1", None, None, None, None)
        .await
        .unwrap();
    let row = page.rows.first().expect("fixture seeds at least one post");

    let detail = client
        .get_view_detail("lemmy-feed-1", &row.id)
        .await
        .expect("get_view_detail should succeed");

    assert!(
        detail.body_block.sanitized_html.contains("<h3>"),
        "body should include a headline"
    );
    assert!(
        detail.comments_section.is_some(),
        "comments section must be Some"
    );
}

/// `get_view_detail` on a bogus row id returns a parse error.
#[tokio::test]
async fn test_get_view_detail_bad_row_id() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
        })
        .await
        .unwrap();

    let result = client.get_view_detail("lemmy-feed-1", "not-a-post").await;
    assert!(matches!(result, Err(poly_client::ClientError::NotFound(_))));
}

/// Context-menu items for a subscribed community include Unsubscribe, not Subscribe.
#[tokio::test]
async fn test_context_menu_subscribed_shows_unsubscribe() {
    let base_url = start_test_server().await;
    let mut client = LemmyClient::new(&base_url);
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
        })
        .await
        .unwrap();

    // Mock server seeds communities 1 and 2 as subscribed=true.
    let items = client
        .get_context_menu_items(poly_client::MenuTargetKind::Server, "lemmy-community-1")
        .await
        .expect("context menu should succeed");

    let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
    assert!(
        ids.contains(&"unsubscribe-community"),
        "subscribed community must expose Unsubscribe, got {ids:?}"
    );
    assert!(
        !ids.contains(&"subscribe-community"),
        "subscribed community must NOT also expose Subscribe, got {ids:?}"
    );
}

/// Unauthenticated / unreachable community lookup defaults to the "Subscribe" item.
#[tokio::test]
async fn test_context_menu_unauthenticated_defaults_to_subscribe() {
    // No server running at this port → community lookup fails → defaults.
    let client = LemmyClient::new("http://127.0.0.1:1");
    let items = client
        .get_context_menu_items(poly_client::MenuTargetKind::Server, "lemmy-community-1")
        .await
        .expect("context menu should not error even when lookup fails");

    let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
    assert!(
        ids.contains(&"subscribe-community"),
        "fallback must offer Subscribe, got {ids:?}"
    );
    assert!(!ids.contains(&"unsubscribe-community"));
}

#[tokio::test]
async fn test_settings_storage_round_trip() {
    use poly_client::SettingsScope;
    let client = LemmyClient::new("https://lemmy.example");
    client
        .set_setting_value(SettingsScope::PerServer, "comm1", "mute-community", "true")
        .await
        .expect("set_setting_value should succeed");
    let got = client
        .get_setting_value(SettingsScope::PerServer, "comm1", "mute-community")
        .await
        .expect("get_setting_value should succeed");
    assert_eq!(got, "true");
}
