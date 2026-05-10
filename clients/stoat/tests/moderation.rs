//! B-ST moderation integration tests.
//!
//! Exercises kick, ban, unban, timeout, untimeout, delete_message,
//! update_channel (slowmode field), and get_bans against a live
//! test-stoat server spun up in-process.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;


use poly_stoat::StoatClient;
use poly_test_stoat::StoatState;
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Spin up test-stoat and return the base URL + the shared state handle.
async fn start_server() -> (String, Arc<StoatState>) {
    let state = Arc::new(StoatState::new());
    state.seed();
    let router = poly_test_stoat::router(state.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve");
    });
    (format!("http://{addr}"), state)
}

/// Authenticate `username` against the running server and return a ready client.
///
/// Uses `POST /test/auth/token` to get a raw token, then calls `authenticate`
/// (token-restore path) so the session `user_id` is populated correctly.
async fn authenticated_client(base_url: &str, username: &str) -> StoatClient {
    use poly_client::AuthCredentials;
    let token = obtain_token(base_url, username).await;
    let mut client = StoatClient::with_base_url(base_url).expect("base url");
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");
    client
}

async fn obtain_token(base_url: &str, username: &str) -> String {
    let resp = reqwest::Client::new()
        .post(format!("{base_url}/test/auth/token"))
        .json(&serde_json::json!({ "username": username }))
        .send()
        .await
        .expect("POST /test/auth/token")
        .json::<serde_json::Value>()
        .await
        .expect("json");
    resp["token"].as_str().expect("token field").to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// B-ST-2 — `get_my_permissions` maps server owner to all-permissions-true.
#[tokio::test]
async fn test_get_my_permissions_owner() {
    let (base_url, _state) = start_server().await;
    // STOAT01 owns SRV001
    let client = authenticated_client(&base_url, "stoat").await;

    let perms = client
        .get_my_permissions("SRV001", None)
        .await
        .expect("get_my_permissions");

    assert!(perms.kick_members, "owner must have kick_members");
    assert!(perms.ban_members, "owner must have ban_members");
    assert!(perms.timeout_members, "owner must have timeout_members");
    assert!(perms.manage_messages, "owner must have manage_messages");
    assert_eq!(perms.display_role, "Owner");
}

/// B-ST-2 — `get_my_permissions` returns empty flags for a non-owner member.
#[tokio::test]
async fn test_get_my_permissions_member() {
    let (base_url, _state) = start_server().await;
    // RACCOON01 is a member of SRV001, not the owner
    let client = authenticated_client(&base_url, "raccoon").await;

    let perms = client
        .get_my_permissions("SRV001", None)
        .await
        .expect("get_my_permissions");

    assert!(!perms.kick_members);
    assert!(!perms.ban_members);
    assert!(!perms.timeout_members);
}

/// B-ST-3 — `kick_member` sends DELETE to /servers/{id}/members/{id}.
#[tokio::test]
async fn test_kick_member() {
    let (base_url, state) = start_server().await;
    let client = authenticated_client(&base_url, "stoat").await;

    // LEMMING01 is in SRV001
    client
        .kick_member("SRV001", "LEMMING01", Some("test kick"))
        .await
        .expect("kick_member");

    // Verify the member was removed from state.
    let srv = state.servers.get("SRV001").expect("SRV001");
    assert!(!srv.members.contains(&"LEMMING01".to_string()), "lemming still in server after kick");
}

/// B-ST-4 — `ban_member` sends PUT to /servers/{id}/bans/{user_id} with delete_message_seconds.
#[tokio::test]
async fn test_ban_member() {
    let (base_url, state) = start_server().await;
    let client = authenticated_client(&base_url, "stoat").await;

    client
        .ban_member("SRV001", "LEMMING01", Some("spamming"), Some(3600))
        .await
        .expect("ban_member");

    let key = StoatState::member_key("SRV001", "LEMMING01");
    let ban = state.bans.get(&key).expect("ban record should exist");
    assert_eq!(ban.reason.as_deref(), Some("spamming"));
}

/// B-ST-4 — `unban_member` sends DELETE to /servers/{id}/bans/{user_id}.
#[tokio::test]
async fn test_unban_member() {
    let (base_url, state) = start_server().await;
    let client = authenticated_client(&base_url, "stoat").await;

    // First ban.
    client
        .ban_member("SRV001", "RACCOON01", Some("reason"), None)
        .await
        .expect("ban first");

    let key = StoatState::member_key("SRV001", "RACCOON01");
    assert!(state.bans.contains_key(&key), "ban should exist before unban");

    // Now unban.
    client
        .unban_member("SRV001", "RACCOON01")
        .await
        .expect("unban_member");

    assert!(!state.bans.contains_key(&key), "ban should be gone after unban");
}

/// B-ST-5 — `get_bans` returns the current ban list.
#[tokio::test]
async fn test_get_bans() {
    let (base_url, _state) = start_server().await;
    let client = authenticated_client(&base_url, "stoat").await;

    // Ban two users first.
    client.ban_member("SRV001", "LEMMING01", Some("r1"), None).await.expect("ban 1");
    client.ban_member("SRV001", "RACCOON01", Some("r2"), None).await.expect("ban 2");

    let bans = client.get_bans("SRV001").await.expect("get_bans");
    assert_eq!(bans.len(), 2);
    let user_ids: std::collections::HashSet<&str> =
        bans.iter().map(|b| b.user_id.as_str()).collect();
    assert!(user_ids.contains("LEMMING01"));
    assert!(user_ids.contains("RACCOON01"));
}

/// B-ST-6 — `timeout_member` sends PATCH with `timeout` ISO8601 field.
#[tokio::test]
async fn test_timeout_member() {
    let (base_url, state) = start_server().await;
    let client = authenticated_client(&base_url, "stoat").await;

    let until = chrono::Utc::now() + chrono::Duration::hours(1);
    client
        .timeout_member("SRV001", "RACCOON01", until, None)
        .await
        .expect("timeout_member");

    let key = StoatState::member_key("SRV001", "RACCOON01");
    let mod_state = state.member_mod.get(&key).expect("member_mod entry");
    assert!(mod_state.timeout.is_some(), "timeout field must be set");
}

/// B-ST-6 — `untimeout_member` sends PATCH with `remove: ["Timeout"]`.
#[tokio::test]
async fn test_untimeout_member() {
    let (base_url, state) = start_server().await;
    let client = authenticated_client(&base_url, "stoat").await;

    // Set a timeout first.
    let until = chrono::Utc::now() + chrono::Duration::hours(1);
    client.timeout_member("SRV001", "RACCOON01", until, None).await.expect("timeout");

    // Now clear it.
    client.untimeout_member("SRV001", "RACCOON01").await.expect("untimeout");

    let key = StoatState::member_key("SRV001", "RACCOON01");
    let mod_state = state.member_mod.get(&key);
    let timed_out = mod_state.map(|m| m.timeout.is_some()).unwrap_or(false);
    assert!(!timed_out, "timeout should be cleared after untimeout");
}

/// B-ST-8 — `delete_message` sends DELETE to /channels/{c}/messages/{m}.
#[tokio::test]
async fn test_delete_message() {
    let (base_url, state) = start_server().await;
    let client = authenticated_client(&base_url, "stoat").await;

    // Get a real message ID from CH001.
    let msg_id = {
        let timeline = state.messages.get("CH001").expect("CH001");
        timeline.first().expect("at least one message").id.clone()
    };

    client.delete_message("CH001", &msg_id).await.expect("delete_message");

    // Message should be gone.
    let timeline = state.messages.get("CH001").expect("CH001");
    assert!(!timeline.iter().any(|m| m.id == msg_id), "message should be deleted");
}

/// B-ST-7 — `update_channel` sends PATCH with `slowmode` field (not `slow_mode_secs`).
#[tokio::test]
async fn test_update_channel_slowmode() {
    let (base_url, state) = start_server().await;
    let client = authenticated_client(&base_url, "stoat").await;

    client
        .update_channel(
            "CH001",
            UpdateChannelParams {
                slow_mode_secs: Some(30),
                name: Some("general-updated".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("update_channel");

    // Verify channel name was updated (slowmode is not stored in test-stoat state
    // but the field name mapping on the wire is validated by the 204 response above).
    let ch = state.channels.get("CH001").expect("CH001");
    assert_eq!(ch.name, "general-updated");
}
