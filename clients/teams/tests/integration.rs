//! Integration tests for `poly-teams` against the mock Teams/Graph API test server.
//!
//! Spins up `poly_test_teams` in-process, seeds demo data, authenticates,
//! and exercises the full `ClientBackend` API surface.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;


use poly_client::{
    IsBackend, ModerationBackend, SocialGraphBackend, DmsAndGroupsBackend, AuthCredentials, BackendType, ChannelType, ClientError,
    MessageContent, MessageQuery, PresenceStatus, SettingsScope,
};
use poly_teams::TeamsClient;
use poly_test_teams::{TeamsState, router};
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Test server helpers
// ---------------------------------------------------------------------------

struct TestServer {
    base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        let base_url = format!("http://127.0.0.1:{port}");

        let state = Arc::new(TeamsState::new());
        state.seed();

        let app = router(Arc::clone(&state));
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { rx.await.ok(); })
                .await
                .ok();
        });
        Self { base_url, _shutdown: tx }
    }

    async fn token_for(&self, display_name: &str) -> String {
        let resp: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/test/auth/token", self.base_url))
            .json(&serde_json::json!({ "username": display_name }))
            .send()
            .await
            .expect("test_auth_token POST")
            .json()
            .await
            .expect("parse token response");
        resp["token"].as_str().expect("token field").to_string()
    }

    async fn authenticated_client(&self, display_name: &str) -> TeamsClient {
        let token = self.token_for(display_name).await;
        let mut client = TeamsClient::with_base_url(self.base_url.clone());
        client
            .authenticate(AuthCredentials::Token(token))
            .await
            .expect("authenticate");
        client
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_authenticate_and_session() {
    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;
    let mut client = TeamsClient::with_base_url(srv.base_url.clone());

    assert!(!client.is_authenticated());
    let session = client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    assert!(client.is_authenticated());
    assert_eq!(session.user.display_name, "Sheep");
    assert_eq!(session.backend, BackendType::from("teams"));
}

#[tokio::test]
async fn test_authenticate_invalid_token_fails() {
    let srv = TestServer::start().await;
    let mut client = TeamsClient::with_base_url(srv.base_url.clone());
    let result = client
        .authenticate(AuthCredentials::Token("bad-token-xyz".to_string()))
        .await;
    assert!(result.is_err(), "invalid token should fail");
}

#[tokio::test]
async fn test_logout_clears_auth() {
    let srv = TestServer::start().await;
    let mut client = srv.authenticated_client("Sheep").await;
    assert!(client.is_authenticated());
    client.logout().await.expect("logout");
    assert!(!client.is_authenticated());
}

#[tokio::test]
async fn test_get_servers() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;
    let servers = client.get_servers().await.expect("get_servers");
    assert!(!servers.is_empty(), "Sheep should be in at least one team");
    let names: Vec<_> = servers.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Contoso Corp"), "Contoso Corp expected");
    assert!(names.contains(&"Project Alpha"), "Project Alpha expected");
    for s in &servers {
        assert_eq!(s.backend, BackendType::from("teams"));
    }
}

#[tokio::test]
async fn test_get_server_by_id() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;
    let server = client.get_server("T001").await.expect("get_server T001");
    assert_eq!(server.id, "T001");
    assert_eq!(server.name, "Contoso Corp");
}

#[tokio::test]
async fn test_get_channels() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;
    let channels = client.get_channels("T001").await.expect("get_channels T001");
    assert!(!channels.is_empty());
    let names: Vec<_> = channels.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"General"), "General channel expected");
    assert!(names.contains(&"Engineering"), "Engineering channel expected");
    for ch in &channels {
        assert_eq!(ch.channel_type, ChannelType::Text);
        assert_eq!(ch.server_id, "T001");
    }
}

#[tokio::test]
async fn test_get_channel_by_compound_id() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;
    // Teams channels require "team_id/channel_id" round-tripped both ways so
    // subsequent ops (get_messages, send_message, etc.) route to the correct
    // Graph endpoint via the '/' split.
    let ch = client.get_channel("T001/CH001").await.expect("get_channel T001/CH001");
    assert_eq!(ch.id, "T001/CH001");
    assert_eq!(ch.name, "General");
    assert_eq!(ch.server_id, "T001");
}

#[tokio::test]
async fn test_get_messages() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;
    let msgs = client
        .get_messages("T001/CH001", MessageQuery { limit: Some(10), before: None, after: None, around: None })
        .await
        .expect("get_messages");
    assert!(!msgs.is_empty(), "T001/CH001 should have seeded messages");
    let has_morning = msgs.iter().any(|m| {
        matches!(&m.content, MessageContent::Text(t) if t.contains("Good morning"))
    });
    assert!(has_morning, "expected 'Good morning' seeded message");
}

#[tokio::test]
async fn test_send_message() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;
    let msg = client
        .send_message("T001/CH001", MessageContent::Text("Hello Teams!".to_string()))
        .await
        .expect("send_message");
    assert_eq!(msg.content, MessageContent::Text("Hello Teams!".to_string()));
}

#[tokio::test]
async fn test_send_then_read_message() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;
    let sent = client
        .send_message("T001/CH002", MessageContent::Text("Engineering note".to_string()))
        .await
        .expect("send to T001/CH002");

    let msgs = client
        .get_messages("T001/CH002", MessageQuery { limit: Some(20), before: None, after: None, around: None })
        .await
        .expect("get_messages CH002");
    assert!(
        msgs.iter().any(|m| m.id == sent.id),
        "sent message should appear in get_messages"
    );
}

#[tokio::test]
async fn test_get_dm_channels() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;
    let dms = client.get_dm_channels().await.expect("get_dm_channels");
    // Sheep is a member of CHAT001 (oneOnOne with Walrus)
    assert!(!dms.is_empty(), "Sheep should have at least one chat");
    assert_eq!(dms[0].backend, BackendType::from("teams"));
    // F-TE-1: contact display name must be resolved from members, not "Unknown"
    assert_ne!(
        dms[0].user.display_name, "Unknown",
        "DM contact should resolve to a real display name, not 'Unknown'"
    );
    assert_eq!(
        dms[0].user.display_name, "Walrus",
        "Sheep's 1:1 DM contact should be Walrus"
    );
}

#[tokio::test]
async fn test_backend_type_and_name() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;
    assert_eq!(client.backend_type(), BackendType::from("teams"));
    assert_eq!(client.backend_name(), "Teams");
}

#[tokio::test]
async fn test_presence_stubs() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;
    let presence = client.get_presence("U001").await.expect("get_presence");
    assert_eq!(presence, PresenceStatus::Offline);
    // Teams trait-split tier 2 (WritableSocialGraphBackend): Teams has no
    // friend system, and `get_friends` now returns NotSupported instead of
    // an empty list.
    let friends_result = client.get_friends().await;
    match friends_result {
        Ok(friends) => assert!(friends.is_empty(), "expected empty"),
        Err(poly_client::ClientError::NotSupported(_)) => {} // Teams: no friend system
        Err(e) => panic!("get_friends: unexpected error {e:?}"),
    }
}

#[tokio::test]
async fn test_walrus_only_in_contoso() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Walrus").await;
    let servers = client.get_servers().await.expect("get_servers for Walrus");
    let names: Vec<_> = servers.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Contoso Corp"), "Walrus is in Contoso Corp");
    assert!(!names.contains(&"Project Alpha"), "Walrus is NOT in Project Alpha");
}

#[tokio::test]
async fn test_concurrent_sessions() {
    let srv = TestServer::start().await;
    let mut sheep = TeamsClient::with_base_url(srv.base_url.clone());
    let mut walrus = TeamsClient::with_base_url(srv.base_url.clone());

    let tok_s = srv.token_for("Sheep").await;
    let tok_w = srv.token_for("Walrus").await;

    let sess_s = sheep.authenticate(AuthCredentials::Token(tok_s)).await.expect("sheep auth");
    let sess_w = walrus.authenticate(AuthCredentials::Token(tok_w)).await.expect("walrus auth");

    assert_eq!(sess_s.user.display_name, "Sheep");
    assert_eq!(sess_w.user.display_name, "Walrus");
    assert_ne!(sess_s.token, sess_w.token);
}

// ---------------------------------------------------------------------------
// Account overview view (get_account_overview_view + get_view_rows)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_account_overview_view_descriptor() {
    use poly_client::{ViewBody, ViewKind};
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;
    let desc = client
        .get_account_overview_view()
        .await
        .expect("get_account_overview_view");
    assert_eq!(desc.kind, ViewKind::CardGrid);
    assert!(matches!(desc.body, ViewBody::CardBody(_)));
    let header = desc.header.expect("header should be present");
    assert!(
        header.title_key.as_deref().is_some(),
        "overview header should have a title_key"
    );
}

#[tokio::test]
async fn test_get_view_rows_overview_returns_teams() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;
    // Empty channel_id triggers the account-overview path.
    let page = client
        .get_view_rows("", None, None, None, None)
        .await
        .expect("get_view_rows overview");
    assert!(!page.rows.is_empty(), "overview should return at least one team card");
    let names: Vec<_> = page.rows.iter().map(|r| r.primary_text.as_str()).collect();
    assert!(names.contains(&"Contoso Corp"), "Contoso Corp card expected");
    assert!(names.contains(&"Project Alpha"), "Project Alpha card expected");
    // Each row meta_text should mention "channel" count.
    for row in &page.rows {
        let meta = row.meta_text.as_deref().expect("meta_text should be set");
        assert!(
            meta.contains("channel"),
            "meta_text should mention channel count: {meta}"
        );
        assert!(
            meta.contains("unread"),
            "meta_text should mention unread count: {meta}"
        );
        assert!(
            meta.contains('@'),
            "meta_text should mention mention count: {meta}"
        );
    }
    assert!(page.next_cursor.is_none(), "overview has no pagination");
}

#[tokio::test]
async fn test_get_view_rows_non_overview_returns_not_supported() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;
    let result = client
        .get_view_rows("T001/CH001", None, None, None, None)
        .await;
    assert!(
        result.is_err(),
        "non-overview channel_id should return an error"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(err, poly_client::ClientError::NotSupported(_)),
        "expected NotSupported, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Pack C.2 — settings storage round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_settings_storage_round_trip() {
    
    let client = poly_teams::TeamsClient::new();
    client
        .set_setting_value(SettingsScope::PerServer, "team1", "display-name", "teams-nick")
        .await
        .expect("set_setting_value should succeed");
    let got = client
        .get_setting_value(SettingsScope::PerServer, "team1", "display-name")
        .await
        .expect("get_setting_value should succeed");
    assert_eq!(got, "teams-nick");
}

// ---------------------------------------------------------------------------
// Follow-up gaps — visual-teams.md audit
// ---------------------------------------------------------------------------

/// `search_messages` is not supported by Teams.
///
/// Teams Graph search requires a delegated `Mail.Read` / `ChannelMessage.Read`
/// scope that the plugin intentionally does not request. Until a dedicated
/// `/search/query` path is wired up, the backend does not implement
/// `MessagingBackend` (which carries `search_messages`), so `as_messaging()`
/// returns `None`.
#[tokio::test]
async fn test_search_messages_returns_not_supported() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;

    assert!(
        IsBackend::as_messaging(&client).is_none(),
        "TeamsClient should not implement MessagingBackend (no search_messages support)"
    );
}

/// `timeout_member` returns NotSupported — Teams has no per-user timeout concept.
#[tokio::test]
async fn test_timeout_member_returns_not_supported() {
    
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;

    let until = chrono::Utc::now() + chrono::Duration::hours(1);
    let result = client.timeout_member("T001", "U002", until, Some("test")).await;

    assert!(
        matches!(result, Err(ClientError::NotSupported(_))),
        "timeout_member should return NotSupported; got: {result:?}"
    );
}

/// `untimeout_member` returns NotSupported — Teams has no timeout concept.
#[tokio::test]
async fn test_untimeout_member_returns_not_supported() {
    
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("Sheep").await;

    let result = client.untimeout_member("T001", "U002").await;

    assert!(
        matches!(result, Err(ClientError::NotSupported(_))),
        "untimeout_member should return NotSupported; got: {result:?}"
    );
}

/// `backend_capabilities` correctly gates ban/timeout as absent.
///
/// Load-bearing for the moderation UI: it reads these flags before showing
/// ban/timeout menu items. Regression guard.
#[test]
fn test_backend_capabilities_no_ban_no_timeout() {
    use poly_client::IsBackend;
    let client = poly_teams::TeamsClient::new();
    let caps = client.backend_capabilities();
    assert!(!caps.has_ban, "Teams must not claim has_ban");
    assert!(!caps.has_timed_ban, "Teams must not claim has_timed_ban");
    assert!(caps.has_kick, "Teams must claim has_kick (owner can remove members)");
    assert!(!caps.has_moderation_log, "Teams must not claim has_moderation_log");
}
