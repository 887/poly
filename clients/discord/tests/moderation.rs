//! Discord moderation round-trip tests (B-DS-11).
//!
//! Exercises kick, ban, unban, timeout, delete_message, update_channel, and
//! get_moderation_log against the in-process mock server.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code
)]

use std::sync::Arc;

use poly_client::{AuthCredentials, ClientBackend, UpdateChannelParams};
use poly_discord::DiscordClient;
use poly_test_discord::{DiscordState, router};
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Shared test-server fixture
// ---------------------------------------------------------------------------

struct TestServer {
    base_url: String,
    state: Arc<DiscordState>,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        let base_url = format!("http://127.0.0.1:{port}");
        let ws_url = format!("ws://127.0.0.1:{port}/gateway/ws");

        let state = Arc::new(DiscordState::new());
        state.seed();
        state.seed_moderation();
        *state.gateway_url.write().await = ws_url.clone();

        let app = router(Arc::clone(&state));
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    rx.await.ok();
                })
                .await
                .ok();
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        Self { base_url, state, _shutdown: tx }
    }

    async fn token_for(&self, username: &str) -> String {
        let resp: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/test/auth/token", self.base_url))
            .json(&serde_json::json!({ "username": username }))
            .send()
            .await
            .expect("test_auth_token POST")
            .json()
            .await
            .expect("parse token response");
        resp["token"].as_str().expect("token field").to_string()
    }

    async fn authenticated_client(&self, username: &str) -> DiscordClient {
        let token = self.token_for(username).await;
        let mut client = DiscordClient::with_base_url(self.base_url.clone());
        client
            .authenticate(AuthCredentials::Token(token))
            .await
            .expect("authenticate");
        client
    }
}

// ---------------------------------------------------------------------------
// B-DS-1: get_my_permissions — guild owner returns all flags true
// ---------------------------------------------------------------------------

/// Koala (user 1) is the owner of guild 100 → all permission flags true.
#[tokio::test]
async fn test_get_my_permissions_owner() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;

    let perms = client
        .get_my_permissions("100", None)
        .await
        .expect("get_my_permissions");

    assert!(perms.manage_server, "owner must have manage_server");
    assert!(perms.manage_channels, "owner must have manage_channels");
    assert!(perms.manage_roles, "owner must have manage_roles");
    assert!(perms.kick_members, "owner must have kick_members");
    assert!(perms.ban_members, "owner must have ban_members");
    assert!(perms.manage_messages, "owner must have manage_messages");
    assert!(perms.timeout_members, "owner must have timeout_members");
    assert_eq!(perms.display_role, "Owner");
}

/// Kangaroo (user 2) is a member (no special roles) in guild 100 → no permissions.
#[tokio::test]
async fn test_get_my_permissions_member() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("kangaroo").await;

    let perms = client
        .get_my_permissions("100", None)
        .await
        .expect("get_my_permissions");

    assert!(!perms.kick_members, "plain member must not have kick_members");
    assert!(!perms.ban_members, "plain member must not have ban_members");
}

// ---------------------------------------------------------------------------
// B-DS-2: kick_member
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_kick_member() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;

    // Guild 100 initially has user 2 (kangaroo) as a member.
    let result = client.kick_member("100", "2", Some("test kick")).await;
    assert!(result.is_ok(), "kick_member should succeed: {result:?}");

    // Verify user 2 is no longer in guild 100 members list.
    let guild = srv.state.guilds.get(&twilight_model::id::Id::new(100)).expect("guild 100");
    assert!(
        !guild.members.contains(&twilight_model::id::Id::new(2)),
        "kangaroo should have been removed from guild 100"
    );
}

// ---------------------------------------------------------------------------
// B-DS-3: ban_member — PUT with delete_message_seconds
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ban_member() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;

    let result = client
        .ban_member("100", "2", Some("spamming"), Some(3600))
        .await;
    assert!(result.is_ok(), "ban_member should succeed: {result:?}");

    // Verify ban is recorded in state.
    let bans = srv
        .state
        .bans
        .get(&twilight_model::id::Id::new(100))
        .expect("bans for guild 100");
    let banned_user_id = twilight_model::id::Id::<twilight_model::id::marker::UserMarker>::new(2);
    assert!(
        bans.iter().any(|b| b.user_id == banned_user_id),
        "kangaroo should appear in the ban list"
    );
}

// ---------------------------------------------------------------------------
// B-DS-4: unban_member
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_unban_member() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;

    // First ban, then unban.
    client
        .ban_member("100", "2", None, None)
        .await
        .expect("ban");
    let result = client.unban_member("100", "2").await;
    assert!(result.is_ok(), "unban_member should succeed: {result:?}");

    let bans = srv
        .state
        .bans
        .get(&twilight_model::id::Id::new(100))
        .map(|v| v.clone())
        .unwrap_or_default();
    let banned_user_id = twilight_model::id::Id::<twilight_model::id::marker::UserMarker>::new(2);
    assert!(
        !bans.iter().any(|b| b.user_id == banned_user_id),
        "kangaroo should no longer be banned"
    );
}

// ---------------------------------------------------------------------------
// Timeout: timeout_member + untimeout_member
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_timeout_member() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;

    let until = chrono::Utc::now() + chrono::Duration::hours(1);
    let result = client.timeout_member("100", "2", until, Some("cooling off")).await;
    assert!(result.is_ok(), "timeout_member should succeed: {result:?}");
}

#[tokio::test]
async fn test_untimeout_member() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;

    let result = client.untimeout_member("100", "2").await;
    assert!(result.is_ok(), "untimeout_member should succeed: {result:?}");
}

// ---------------------------------------------------------------------------
// B-DS-6: delete_message
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_delete_message() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;

    // Message 400 is in channel 200.
    let result = client.delete_message("200", "400").await;
    assert!(result.is_ok(), "delete_message should succeed: {result:?}");

    // Verify the message is gone from the state.
    let ch_id = twilight_model::id::Id::<twilight_model::id::marker::ChannelMarker>::new(200);
    let msgs = srv.state.messages.get(&ch_id).expect("channel 200 messages");
    let msg_id = twilight_model::id::Id::<twilight_model::id::marker::MessageMarker>::new(400);
    assert!(
        !msgs.iter().any(|m| m.id == msg_id),
        "message 400 should have been deleted"
    );
}

// ---------------------------------------------------------------------------
// B-DS-7: update_channel — PATCH with name/topic/rate_limit_per_user
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_update_channel() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;

    let params = UpdateChannelParams {
        name: Some("new-general".to_string()),
        topic: Some("Updated topic".to_string()),
        slow_mode_secs: Some(10),
        nsfw: None,
        position: None,
    };

    let result = client.update_channel("200", params).await;
    assert!(result.is_ok(), "update_channel should succeed: {result:?}");

    // Verify channel name was updated.
    let ch_id = twilight_model::id::Id::<twilight_model::id::marker::ChannelMarker>::new(200);
    let ch = srv.state.channels.get(&ch_id).expect("channel 200");
    assert_eq!(ch.name, "new-general", "channel name should be updated");
}

// ---------------------------------------------------------------------------
// B-DS-9: get_moderation_log — maps audit log entries
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_moderation_log() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;

    let entries = client
        .get_moderation_log("100", 10)
        .await
        .expect("get_moderation_log");

    // seed_moderation seeded 3 entries: ban_add, kick, msg_delete.
    assert!(!entries.is_empty(), "should have at least one moderation log entry");

    // Verify action types are correctly mapped.
    let has_ban = entries.iter().any(|e| {
        matches!(e.action, poly_client::ModerationAction::MemberBanned)
    });
    let has_kick = entries.iter().any(|e| {
        matches!(e.action, poly_client::ModerationAction::MemberKicked)
    });
    let has_delete = entries.iter().any(|e| {
        matches!(e.action, poly_client::ModerationAction::MessageDeleted)
    });

    assert!(has_ban, "should have a MemberBanned entry");
    assert!(has_kick, "should have a MemberKicked entry");
    assert!(has_delete, "should have a MessageDeleted entry");
}

// ---------------------------------------------------------------------------
// B-DS-5: get_bans
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_bans() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;

    // Ban user 3 (wallaby) first.
    client
        .ban_member("100", "3", Some("test"), None)
        .await
        .expect("ban");

    let bans = client.get_bans("100").await.expect("get_bans");
    assert!(!bans.is_empty(), "ban list should be non-empty after banning");

    let wallaby_banned = bans.iter().any(|b| b.user_id == "3");
    assert!(wallaby_banned, "wallaby should appear in the ban list");
}
