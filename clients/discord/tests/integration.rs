//! Integration tests for `poly-discord` against the mock Discord test server.
//!
//! Each test spins up a `poly_test_discord` router on a random port, seeds
//! demo data, authenticates via `/test/auth/token`, then exercises the full
//! `ClientBackend` API surface.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use poly_client::{AuthCredentials, BackendType, ChannelType, ClientBackend, MessageContent, MessageQuery, PresenceStatus};
use poly_discord::DiscordClient;
use poly_test_discord::{DiscordState, router};
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

        let state = Arc::new(DiscordState::new());
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

    /// Obtain a user token via the test-only easy-signin endpoint.
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

    /// Build a `DiscordClient` and authenticate as the given username.
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
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_authenticate_and_session() {
    let srv = TestServer::start().await;
    let token = srv.token_for("koala").await;
    let mut client = DiscordClient::with_base_url(srv.base_url.clone());

    assert!(!client.is_authenticated());
    let session = client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    assert!(client.is_authenticated());
    assert_eq!(session.user.display_name, "koala");
    assert_eq!(session.backend, BackendType::from("discord"));
}

#[tokio::test]
async fn test_authenticate_invalid_token_fails() {
    let srv = TestServer::start().await;
    let mut client = DiscordClient::with_base_url(srv.base_url.clone());
    let result = client
        .authenticate(AuthCredentials::Token("not-a-real-token".to_string()))
        .await;
    assert!(result.is_err(), "bad token should fail");
}

#[tokio::test]
async fn test_logout_clears_auth() {
    let srv = TestServer::start().await;
    let mut client = srv.authenticated_client("koala").await;
    assert!(client.is_authenticated());
    client.logout().await.expect("logout");
    assert!(!client.is_authenticated());
}

#[tokio::test]
async fn test_get_servers() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let servers = client.get_servers().await.expect("get_servers");
    assert!(!servers.is_empty(), "should have at least one guild");
    let names: Vec<_> = servers.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Australiana"), "Australiana guild expected");
    assert!(names.contains(&"Wildlife Chat"), "Wildlife Chat guild expected");
    for s in &servers {
        assert_eq!(s.backend, BackendType::from("discord"));
    }
}

#[tokio::test]
async fn test_get_server_by_id() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let server = client.get_server("G001").await.expect("get_server G001");
    assert_eq!(server.id, "G001");
    assert_eq!(server.name, "Australiana");
}

#[tokio::test]
async fn test_get_channels() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let channels = client.get_channels("G001").await.expect("get_channels G001");
    assert!(!channels.is_empty());
    let names: Vec<_> = channels.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"general"), "general channel expected");
    assert!(names.contains(&"random"), "random channel expected");
    for ch in &channels {
        assert_eq!(ch.channel_type, ChannelType::Text);
        assert_eq!(ch.server_id, "G001");
    }
}

#[tokio::test]
async fn test_get_channel_by_id() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let ch = client.get_channel("CH001").await.expect("get_channel CH001");
    assert_eq!(ch.id, "CH001");
    assert_eq!(ch.name, "general");
    assert_eq!(ch.channel_type, ChannelType::Text);
}

#[tokio::test]
async fn test_get_messages() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let msgs = client
        .get_messages("CH001", MessageQuery { limit: Some(10), before: None, after: None, around: None })
        .await
        .expect("get_messages");
    assert!(!msgs.is_empty(), "CH001 should have seeded messages");
    let has_gday = msgs.iter().any(|m| {
        matches!(&m.content, MessageContent::Text(t) if t.contains("G'day"))
    });
    assert!(has_gday, "expected seeded G'day message");
}

#[tokio::test]
async fn test_send_message() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let msg = client
        .send_message("CH001", MessageContent::Text("Hello from test!".to_string()))
        .await
        .expect("send_message");
    assert_eq!(msg.content, MessageContent::Text("Hello from test!".to_string()));
    assert_eq!(msg.author.display_name, "koala");
}

#[tokio::test]
async fn test_send_then_read_message() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let sent = client
        .send_message("CH002", MessageContent::Text("Cross-channel ping!".to_string()))
        .await
        .expect("send to CH002");

    let msgs = client
        .get_messages("CH002", MessageQuery { limit: Some(20), before: None, after: None, around: None })
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
    let client = srv.authenticated_client("koala").await;
    // DM channels may be empty in a seeded state — just check it doesn't error
    let _dms = client.get_dm_channels().await.expect("get_dm_channels");
}

#[tokio::test]
async fn test_get_user() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let user = client.get_user("U002").await.expect("get_user U002");
    assert_eq!(user.id, "U002");
    assert_eq!(user.display_name, "kangaroo");
    assert_eq!(user.backend, BackendType::from("discord"));
}

#[tokio::test]
async fn test_backend_type_and_name() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    assert_eq!(client.backend_type(), BackendType::from("discord"));
    assert_eq!(client.backend_name(), "Discord");
}

#[tokio::test]
async fn test_presence_and_friends_stubs() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let presence = client.get_presence("U001").await.expect("get_presence");
    assert_eq!(presence, PresenceStatus::Offline);
    let friends = client.get_friends().await.expect("get_friends");
    assert!(friends.is_empty());
}

#[tokio::test]
async fn test_concurrent_sessions_isolated() {
    let srv = TestServer::start().await;
    let mut koala = DiscordClient::with_base_url(srv.base_url.clone());
    let mut kangaroo = DiscordClient::with_base_url(srv.base_url.clone());

    let tok_k = srv.token_for("koala").await;
    let tok_r = srv.token_for("kangaroo").await;

    let sess_k = koala.authenticate(AuthCredentials::Token(tok_k)).await.expect("koala auth");
    let sess_r = kangaroo.authenticate(AuthCredentials::Token(tok_r)).await.expect("kangaroo auth");

    assert_eq!(sess_k.user.display_name, "koala");
    assert_eq!(sess_r.user.display_name, "kangaroo");
    assert_ne!(sess_k.token, sess_r.token);
}
