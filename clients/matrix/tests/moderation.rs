//! Moderation integration tests for poly-matrix (Wave 2 / Phase B-MX).
//!
//! Tests: kick, ban, redact (delete_message), get_my_permissions (admin),
//! and update_channel (name + topic).
//!
//! Run with:
//! ```
//! cargo test -p poly-matrix --features native
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;
use tokio::net::TcpListener;


use poly_matrix::MatrixClient;
use poly_test_matrix::{MatrixState, router};

// ---------------------------------------------------------------------------
// Test harness (same pattern as integration.rs)
// ---------------------------------------------------------------------------

struct TestServer {
    base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind free port");
        let addr = listener.local_addr().expect("local_addr");
        let base_url = format!("http://{addr}");

        let state = Arc::new(MatrixState::new());
        state.seed();

        let app = router(state);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                })
                .await
                .expect("serve");
        });

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        Self {
            base_url,
            _shutdown: shutdown_tx,
        }
    }
}

/// Authenticate `username` via the test helper endpoint and return an authenticated client.
async fn make_authed_client(base_url: &str, username: &str) -> MatrixClient {
    let resp: serde_json::Value = reqwest::Client::new()
        .post(format!("{base_url}/test/auth/token"))
        .json(&serde_json::json!({ "username": username }))
        .send()
        .await
        .expect("POST /test/auth/token")
        .json()
        .await
        .expect("parse token response");

    let token = resp["access_token"]
        .as_str()
        .expect("access_token in response")
        .to_string();

    let mut client = MatrixClient::with_homeserver(base_url).expect("valid homeserver URL");
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");
    client
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `get_my_permissions` for Owl (level 100 in space1) should return all flags true
/// and power_level == Some(100).
#[tokio::test]
async fn test_get_my_permissions_admin() {
    let srv = TestServer::start().await;
    let client = make_authed_client(&srv.base_url, "Owl").await;

    // Space 1: Owl has level 100 (seeded in state.rs).
    let perms = client
        .get_my_permissions("!space1:localhost", None)
        .await
        .expect("get_my_permissions");

    assert!(perms.kick_members, "admin should be able to kick");
    assert!(perms.ban_members, "admin should be able to ban");
    assert!(perms.manage_messages, "admin should be able to redact");
    assert!(perms.manage_channels, "admin should manage channels");
    // No timeout support in Matrix.
    assert!(!perms.timeout_members, "Matrix has no timeout");
    assert_eq!(perms.power_level, Some(100));
    assert_eq!(perms.display_role, "Admin");
}

/// `get_my_permissions` for Axolotl (level 50 in space1) should return moderator flags.
#[tokio::test]
async fn test_get_my_permissions_moderator() {
    let srv = TestServer::start().await;
    let client = make_authed_client(&srv.base_url, "Axolotl").await;

    let perms = client
        .get_my_permissions("!space1:localhost", None)
        .await
        .expect("get_my_permissions");

    // Level 50 meets ban=50, kick=50, redact=50, state_default=50 thresholds.
    assert!(perms.kick_members);
    assert!(perms.ban_members);
    assert!(perms.manage_messages);
    assert_eq!(perms.power_level, Some(50));
    assert_eq!(perms.display_role, "Moderator");
}

/// `kick_member` — POST to /_matrix/client/v3/rooms/{roomId}/kick.
/// Axolotl is kicked from space1 by Owl.
#[tokio::test]
async fn test_kick_member() {
    let srv = TestServer::start().await;
    let client = make_authed_client(&srv.base_url, "Owl").await;

    client
        .kick_member("!space1:localhost", "@axolotl:localhost", Some("test kick"))
        .await
        .expect("kick_member");

    // Verify: Axolotl is no longer a joined member.
    let members = client
        .get_channel_members("!space1:localhost")
        .await
        .expect("get_channel_members");

    let axolotl_joined = members.iter().any(|u| u.id == "@axolotl:localhost");
    assert!(!axolotl_joined, "Axolotl should have been kicked");
}

/// `ban_member` — POST to /_matrix/client/v3/rooms/{roomId}/ban.
/// Then `get_bans` should list Axolotl.
#[tokio::test]
async fn test_ban_member() {
    let srv = TestServer::start().await;
    let client = make_authed_client(&srv.base_url, "Owl").await;

    client
        .ban_member(
            "!space1:localhost",
            "@axolotl:localhost",
            Some("test ban"),
            None,
        )
        .await
        .expect("ban_member");

    let bans = client
        .get_bans("!space1:localhost")
        .await
        .expect("get_bans");

    assert!(
        bans.iter().any(|b| b.user_id == "@axolotl:localhost"),
        "Axolotl should appear in the bans list"
    );
    // Matrix bans are permanent — no expiry.
    let ban = bans.iter().find(|b| b.user_id == "@axolotl:localhost").unwrap();
    assert!(ban.expires_at.is_none(), "Matrix bans have no expiry");
}

/// `unban_member` — lift the ban, then `get_bans` should no longer list the user.
#[tokio::test]
async fn test_unban_member() {
    let srv = TestServer::start().await;
    let client = make_authed_client(&srv.base_url, "Owl").await;

    // Ban first.
    client
        .ban_member("!space1:localhost", "@axolotl:localhost", None, None)
        .await
        .expect("ban_member");

    // Unban.
    client
        .unban_member("!space1:localhost", "@axolotl:localhost")
        .await
        .expect("unban_member");

    let bans = client
        .get_bans("!space1:localhost")
        .await
        .expect("get_bans");

    assert!(
        !bans.iter().any(|b| b.user_id == "@axolotl:localhost"),
        "Axolotl should no longer be banned"
    );
}

/// `delete_message` (= redact) — PUT to /_matrix/client/v3/rooms/{roomId}/redact/{eventId}/{txnId}.
#[tokio::test]
async fn test_redact_via_delete_message() {
    let srv = TestServer::start().await;
    let client = make_authed_client(&srv.base_url, "Owl").await;

    // Send a message so we have a real event_id.
    use poly_client::MessageContent;
    let msg = client
        .send_message("!general1:localhost", MessageContent::Text("to be redacted".to_string()))
        .await
        .expect("send_message");

    let event_id = msg.id;

    // Redact via delete_message.
    client
        .delete_message("!general1:localhost", &event_id)
        .await
        .expect("delete_message (redact)");
    // If the call succeeded without error the redact went through.
}

/// `timeout_member` must return `NotSupported` — Matrix has no native timeout.
#[tokio::test]
async fn test_timeout_member_not_supported() {
    let srv = TestServer::start().await;
    let client = make_authed_client(&srv.base_url, "Owl").await;

    let result = client
        .timeout_member(
            "!space1:localhost",
            "@axolotl:localhost",
            chrono::Utc::now() + chrono::Duration::hours(1),
            None,
        )
        .await;

    assert!(
        matches!(result, Err(poly_client::ClientError::NotSupported(_))),
        "timeout_member must return NotSupported for Matrix"
    );
}

/// `update_channel` — PUT to /state/m.room.name and /state/m.room.topic.
#[tokio::test]
async fn test_update_channel_name_topic() {
    let srv = TestServer::start().await;
    let client = make_authed_client(&srv.base_url, "Owl").await;

    client
        .update_channel(
            "!general1:localhost",
            UpdateChannelParams {
                name: Some("general-renamed".to_string()),
                topic: Some("New topic for testing".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("update_channel");

    // Verify the channel name changed.
    let channel = client
        .get_channel("!general1:localhost")
        .await
        .expect("get_channel");

    assert_eq!(channel.name, "general-renamed");
}

/// `reorder_channels` must return `NotSupported` — Matrix uses space hierarchy state events.
#[tokio::test]
async fn test_reorder_channels_not_supported() {
    let srv = TestServer::start().await;
    let client = make_authed_client(&srv.base_url, "Owl").await;

    let result = client
        .reorder_channels(
            "!space1:localhost",
            vec!["!general1:localhost".to_string()],
        )
        .await;

    assert!(
        matches!(result, Err(poly_client::ClientError::NotSupported(_))),
        "reorder_channels must return NotSupported for Matrix"
    );
}
