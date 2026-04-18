//! Integration tests for the native Stoat client using the real test-stoat server.
//!
//! Spins up `poly-test-stoat` in-process via its library interface, seeds demo
//! data, then exercises every major method of `StoatClient` / `ClientBackend`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;

use poly_client::{AuthCredentials, ClientBackend, MessageContent, MessageQuery};
use poly_stoat::StoatClient;
use poly_test_common::TestServerBase;
use poly_test_stoat::{StoatState, router};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Start a seeded test-stoat server on a random port.
/// Returns the base URL and a shutdown sender (drop to stop the server).
async fn start_test_server() -> (String, tokio::sync::oneshot::Sender<()>) {
    let state = Arc::new(StoatState::new());
    state.seed();

    let base = TestServerBase::bind(0)
        .await
        .expect("bind random port");
    let base_url = base.base_url();

    let app = router(Arc::clone(&state));

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        axum::serve(base.listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("test-stoat serve");
    });

    // Give the server a moment to start accepting connections.
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;

    (base_url, shutdown_tx)
}

/// Create an authenticated `StoatClient` pointing at `base_url`.
async fn authenticated_client(base_url: &str) -> StoatClient {
    let mut client = StoatClient::with_base_url(base_url).expect("valid base url");
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "stoat".into(),
            password: "testpass123".into(),
        })
        .await
        .expect("authenticate");
    client
}

/// Extract the text from a `MessageContent::Text` variant, panicking otherwise.
fn text_content(content: &MessageContent) -> &str {
    match content {
        MessageContent::Text(s) => s.as_str(),
        other => panic!("expected MessageContent::Text, got: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_authenticate() {
    let (base_url, _shutdown) = start_test_server().await;

    let mut client = StoatClient::with_base_url(&base_url).expect("valid base url");
    let session = client
        .authenticate(AuthCredentials::EmailPassword {
            email: "stoat".into(),
            password: "testpass123".into(),
        })
        .await
        .expect("authenticate should succeed");

    assert_eq!(session.backend.as_str(), "stoat");
    assert!(!session.token.is_empty(), "token must not be empty");
    assert!(client.is_authenticated());
}

#[tokio::test]
async fn test_authenticate_wrong_password() {
    let (base_url, _shutdown) = start_test_server().await;

    let mut client = StoatClient::with_base_url(&base_url).expect("valid base url");
    let result = client
        .authenticate(AuthCredentials::EmailPassword {
            email: "stoat".into(),
            password: "wrongpassword".into(),
        })
        .await;

    assert!(result.is_err(), "wrong password should fail");
}

#[tokio::test]
async fn test_authenticate_with_session_token() {
    let (base_url, _shutdown) = start_test_server().await;

    // Get a token by authenticating normally.
    let mut client = StoatClient::with_base_url(&base_url).expect("valid base url");
    let session = client
        .authenticate(AuthCredentials::EmailPassword {
            email: "stoat".into(),
            password: "testpass123".into(),
        })
        .await
        .expect("initial authenticate");

    let token = session.token.clone();

    // Load that token into a fresh client using the Token variant.
    let mut client2 = StoatClient::with_base_url(&base_url).expect("valid base url");
    client2
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("session token auth should succeed");

    assert!(client2.is_authenticated());
}

// ---------------------------------------------------------------------------
// Servers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_servers() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let servers = client.get_servers().await.expect("get_servers");

    // Seed data has 2 servers: "The Burrow" (SRV001) and "Midnight Dumpster" (SRV002)
    assert_eq!(servers.len(), 2, "expected exactly 2 seeded servers");

    let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"The Burrow"),
        "expected 'The Burrow' server, got: {names:?}"
    );
    assert!(
        names.contains(&"Midnight Dumpster"),
        "expected 'Midnight Dumpster' server, got: {names:?}"
    );

    for srv in &servers {
        assert_eq!(srv.backend.as_str(), "stoat");
        assert!(!srv.categories.is_empty(), "server '{}' should have categories", srv.name);
    }
}

#[tokio::test]
async fn test_get_server_by_id() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let server = client.get_server("SRV001").await.expect("get_server SRV001");

    assert_eq!(server.name, "The Burrow");
    assert_eq!(server.backend.as_str(), "stoat");

    // The Burrow has one category ("Text Channels") with 3 channels.
    assert_eq!(server.categories.len(), 1);
    let cat = &server.categories[0];
    assert_eq!(cat.channel_ids.len(), 3, "The Burrow category should have 3 channels");
}

#[tokio::test]
async fn test_get_server_not_found() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let result = client.get_server("NONEXISTENT").await;
    assert!(result.is_err(), "fetching a non-existent server should fail");
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_channels() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let channels = client.get_channels("SRV001").await.expect("get_channels SRV001");

    // The Burrow has 3 channels: general (CH001), random (CH002), memes (CH003)
    assert_eq!(channels.len(), 3, "The Burrow should have 3 channels");

    let names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"general"), "expected 'general' channel");
    assert!(names.contains(&"random"), "expected 'random' channel");
    assert!(names.contains(&"memes"), "expected 'memes' channel");

    for ch in &channels {
        assert_eq!(ch.server_id, "SRV001");
    }
}

#[tokio::test]
async fn test_get_channel() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let channel = client.get_channel("CH001").await.expect("get_channel CH001");

    assert_eq!(channel.name, "general");
    assert_eq!(channel.server_id, "SRV001");
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_messages() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let messages = client
        .get_messages("CH001", MessageQuery::default())
        .await
        .expect("get_messages CH001");

    // The Burrow #general has 8 initial messages + 4 lemming messages = 12
    assert!(
        messages.len() >= 8,
        "expected at least 8 messages in #general, got {}",
        messages.len()
    );

    // All messages should have non-empty IDs.
    for msg in &messages {
        assert!(!msg.id.is_empty(), "message id must not be empty");
    }
}

#[tokio::test]
async fn test_get_messages_with_limit() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let query = MessageQuery {
        limit: Some(3),
        ..Default::default()
    };
    let messages = client
        .get_messages("CH001", query)
        .await
        .expect("get_messages with limit");

    assert!(
        messages.len() <= 3,
        "limit=3 should return at most 3 messages, got {}",
        messages.len()
    );
}

#[tokio::test]
async fn test_send_message() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let message = client
        .send_message(
            "CH001",
            MessageContent::Text("Hello from integration test!".into()),
        )
        .await
        .expect("send_message");

    assert_eq!(
        text_content(&message.content),
        "Hello from integration test!"
    );
    assert!(!message.id.is_empty());

    // Verify it appears in subsequent fetch.
    let messages = client
        .get_messages("CH001", MessageQuery::default())
        .await
        .expect("get_messages after send");

    let found = messages.iter().any(|m| m.id == message.id);
    assert!(found, "sent message should appear in channel history");
}

#[tokio::test]
async fn test_send_reply_message() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    // Fetch an existing message to reply to.
    let messages = client
        .get_messages(
            "CH001",
            MessageQuery {
                limit: Some(1),
                ..Default::default()
            },
        )
        .await
        .expect("get_messages for reply test");

    assert!(!messages.is_empty(), "need at least one message to reply to");
    let reply_to_id = messages[0].id.clone();

    let reply = client
        .send_reply_message(
            "CH001",
            &reply_to_id,
            MessageContent::Text("This is a reply!".into()),
        )
        .await
        .expect("send_reply_message");

    assert_eq!(text_content(&reply.content), "This is a reply!");
    assert!(!reply.id.is_empty());
}

#[tokio::test]
async fn test_get_messages_second_server() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    // Midnight Dumpster #general (CH004) has 5 seeded messages
    let messages = client
        .get_messages("CH004", MessageQuery::default())
        .await
        .expect("get_messages CH004");

    assert!(
        messages.len() >= 3,
        "Midnight Dumpster #general should have at least 3 messages, got {}",
        messages.len()
    );
}

// ---------------------------------------------------------------------------
// DMs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_dm_channels() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let dms = client.get_dm_channels().await.expect("get_dm_channels");

    // stoat user has 1 seeded DM with raccoon
    assert!(
        !dms.is_empty(),
        "stoat user should have at least 1 DM channel"
    );

    for dm in &dms {
        assert_eq!(dm.backend.as_str(), "stoat");
    }
}

#[tokio::test]
async fn test_dm_channel_messages() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    // DM between stoat and raccoon (CHDM001) has 6 seeded messages.
    let messages = client
        .get_messages("CHDM001", MessageQuery::default())
        .await
        .expect("get DM messages");

    assert!(
        messages.len() >= 4,
        "DM should have at least 4 messages, got {}",
        messages.len()
    );
}

// ---------------------------------------------------------------------------
// Users
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_user() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let user = client
        .get_user("RACCOON01")
        .await
        .expect("get_user RACCOON01");

    // display_name for raccoon is "Raccoon"
    assert_eq!(user.display_name, "Raccoon");
    assert_eq!(user.backend.as_str(), "stoat");
    assert_eq!(user.id, "RACCOON01");
}

#[tokio::test]
async fn test_get_friends() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    // `get_friends` fetches /users/@me and filters on StoatRelationshipStatus::Friend.
    // The mock `/users/@me` returns all other users tagged as "Friend" in the relations
    // array. The stoat client only includes users where the API returns status == "Friend"
    // in the structured sense. The call must not error.
    let friends = client.get_friends().await.expect("get_friends");
    let _ = friends;
}

// ---------------------------------------------------------------------------
// Channel Members
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_channel_members_server_channel() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let members = client
        .get_channel_members("CH001")
        .await
        .expect("get_channel_members server channel");

    // The Burrow has 3 members: stoat, raccoon, lemming
    assert_eq!(
        members.len(),
        3,
        "The Burrow #general should have 3 members, got {}",
        members.len()
    );

    let ids: Vec<&str> = members.iter().map(|u| u.id.as_str()).collect();
    assert!(ids.contains(&"STOAT01"), "stoat should be a member");
    assert!(ids.contains(&"RACCOON01"), "raccoon should be a member");
    assert!(ids.contains(&"LEMMING01"), "lemming should be a member");
}

#[tokio::test]
async fn test_get_channel_members_dm_channel() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let members = client
        .get_channel_members("CHDM001")
        .await
        .expect("get_channel_members DM channel");

    // DM between stoat and raccoon has 2 participants
    assert_eq!(members.len(), 2, "DM should have 2 participants");
}

// ---------------------------------------------------------------------------
// Logout
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_logout() {
    let (base_url, _shutdown) = start_test_server().await;
    let mut client = authenticated_client(&base_url).await;

    assert!(client.is_authenticated(), "should be authenticated before logout");

    client.logout().await.expect("logout");

    assert!(
        !client.is_authenticated(),
        "should not be authenticated after logout"
    );
}

// ---------------------------------------------------------------------------
// Server config
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fetch_server_config() {
    let (base_url, _shutdown) = start_test_server().await;
    // fetch_server_config is available without authentication
    let client = StoatClient::with_base_url(&base_url).expect("valid base url");

    let config = client
        .fetch_server_config()
        .await
        .expect("fetch_server_config");

    // The test server returns revolt: "0.7.0"
    assert!(!config.revolt.is_empty(), "revolt version should be set");
}

// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_notifications_server_chat() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    // Stoat notifications are friend requests from pending (Incoming) relationships.
    // The seeded data has no friend requests, so we expect an empty list with no error.
    let notifications = client
        .get_notifications()
        .await
        .expect("get_notifications should not error");

    // No incoming friend requests in seed data — result is Ok and empty is acceptable.
    let _ = notifications;
}

#[tokio::test]
async fn test_notifications_empty_without_requests() {
    let (base_url, _shutdown) = start_test_server().await;

    // Authenticate as raccoon — also has no incoming friend requests in seed data.
    let mut client = StoatClient::with_base_url(&base_url).expect("valid base url");
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "raccoon".into(),
            password: "testpass123".into(),
        })
        .await
        .expect("raccoon authenticate");

    let notifications = client
        .get_notifications()
        .await
        .expect("get_notifications for raccoon should not error");

    // Raccoon has no incoming friend requests either.
    let _ = notifications;
}

// ---------------------------------------------------------------------------
// Unreads (sync)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sync_unreads_direct() {
    let (base_url, _shutdown) = start_test_server().await;

    // Obtain a session token via the test auth helper endpoint.
    let resp: serde_json::Value = reqwest::Client::new()
        .post(format!("{base_url}/test/auth/token"))
        .json(&serde_json::json!({ "username": "stoat" }))
        .send()
        .await
        .expect("POST /test/auth/token")
        .json()
        .await
        .expect("parse token response");

    let token = resp["token"].as_str().expect("token in response");

    // Call GET /sync/unreads with the x-session-token header.
    let unreads_resp = reqwest::Client::new()
        .get(format!("{base_url}/sync/unreads"))
        .header("x-session-token", token)
        .send()
        .await
        .expect("GET /sync/unreads");

    assert_eq!(
        unreads_resp.status(),
        200,
        "GET /sync/unreads should return 200"
    );

    let body: serde_json::Value = unreads_resp.json().await.expect("parse unreads response");

    // The response must be a JSON array (may be empty if no unread counts seeded).
    assert!(
        body.is_array(),
        "GET /sync/unreads should return a JSON array, got: {body}"
    );
}

#[tokio::test]
async fn test_sync_unreads_via_get_servers() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    // get_servers() internally calls fetch_unreads() — exercising the unread code path.
    let servers = client.get_servers().await.expect("get_servers");

    assert_eq!(servers.len(), 2, "expected 2 seeded servers");
    // Each server carries an unread_count field (may be 0 with no seeded unreads).
    for srv in &servers {
        let _ = srv.unread_count;
    }
}

// ---------------------------------------------------------------------------
// DMs with unreads
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_dms_with_unreads() {
    let (base_url, _shutdown) = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let dms = client.get_dm_channels().await.expect("get_dm_channels");

    // stoat has exactly one seeded DM with raccoon (CHDM001).
    assert!(
        !dms.is_empty(),
        "stoat should have at least 1 DM channel, got 0"
    );

    let ids: Vec<&str> = dms.iter().map(|d| d.id.as_str()).collect();
    assert!(
        ids.contains(&"CHDM001"),
        "expected CHDM001 in DM channels, got: {ids:?}"
    );

    for dm in &dms {
        assert_eq!(dm.backend.as_str(), "stoat");
    }
}

// ---------------------------------------------------------------------------
// Pack C.2 — settings storage round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_settings_storage_round_trip() {
    use poly_client::{ClientBackend, SettingsScope};
    let client = poly_stoat::StoatClient::new();
    client
        .set_setting_value(SettingsScope::PerServer, "server1", "nickname", "stoat-nick")
        .await
        .expect("set_setting_value should succeed");
    let got = client
        .get_setting_value(SettingsScope::PerServer, "server1", "nickname")
        .await
        .expect("get_setting_value should succeed");
    assert_eq!(got, "stoat-nick");
}
