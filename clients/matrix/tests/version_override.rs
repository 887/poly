//! User-Agent override test for `poly-matrix`.
//!
//! Phase G.1 / Phase B Fix-up of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! `MatrixHttpClient::request()` injects `User-Agent` on every outbound call.
//! `set_client_version_override` in `impl IsBackend for MatrixClient` calls
//! `self.http.set_user_agent(ua)` which updates the RwLock-backed field.
//! All subsequent `request()` / `authenticated_request()` calls pick it up.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;


use poly_client::{
    IsBackend, MessagingBackend, ModerationBackend, SocialGraphBackend, DmsAndGroupsBackend,
    ServerAdminBackend, AuthCredentials, BackendType, ChannelType, ClientError, ClientEvent,
    MessageContent, MessageQuery, PresenceStatus, SettingsScope, ViewBody, ViewKind,
    UpdateChannelParams,
};
use poly_matrix::MatrixClient;
use poly_test_matrix::{MatrixState, router};
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Helpers
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

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { rx.await.ok(); })
                .await
                .expect("serve");
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        Self { base_url, _shutdown: tx }
    }

    async fn token_for(&self, username: &str) -> String {
        let resp: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/test/auth/token", self.base_url))
            .json(&serde_json::json!({ "username": username }))
            .send()
            .await
            .expect("POST /test/auth/token")
            .json()
            .await
            .expect("parse token");
        resp["access_token"].as_str().expect("access_token").to_string()
    }

    async fn captured_headers(&self) -> Vec<serde_json::Value> {
        let body: serde_json::Value = reqwest::Client::new()
            .get(format!("{}/test/inspect/last-headers", self.base_url))
            .send()
            .await
            .expect("GET /test/inspect/last-headers")
            .json()
            .await
            .expect("parse inspect response");
        body.as_array().expect("array").clone()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Override reaches the wire User-Agent header.
#[tokio::test]
async fn test_version_override_reaches_wire() {
    let srv = TestServer::start().await;
    let token = srv.token_for("Owl").await;

    let mut client =
        MatrixClient::with_homeserver(&srv.base_url).expect("valid homeserver URL");
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    client
        .set_client_version_override(Some("test-version/1.2.3".to_string()))
        .await
        .expect("set_client_version_override");

    assert_eq!(
        client.client_version(),
        "test-version/1.2.3",
        "client_version() must return the override string"
    );

    // Trigger any authenticated request.
    drop(client.get_servers().await);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let entries = srv.captured_headers().await;
    let found = entries.iter().any(|e| {
        e["headers"]["user-agent"].as_str() == Some("test-version/1.2.3")
    });

    assert!(
        found,
        "Expected User-Agent: test-version/1.2.3 on wire. Got: {entries:#?}"
    );
}

/// After clearing, `client_version()` returns the default and the wire UA is restored.
#[tokio::test]
async fn test_version_override_clear_restores_default() {
    const DEFAULT_UA: &str = "poly-matrix/0.0.0";

    let srv = TestServer::start().await;
    let token = srv.token_for("Owl").await;

    let mut client =
        MatrixClient::with_homeserver(&srv.base_url).expect("valid homeserver URL");
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    client
        .set_client_version_override(Some("test-version/1.2.3".to_string()))
        .await
        .expect("set override");
    client
        .set_client_version_override(None)
        .await
        .expect("clear override");

    assert_eq!(
        client.client_version(),
        DEFAULT_UA,
        "client_version() must return the default after clearing"
    );

    drop(client.get_servers().await);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let entries = srv.captured_headers().await;
    let found = entries.iter().any(|e| {
        e["headers"]["user-agent"].as_str() == Some(DEFAULT_UA)
    });

    assert!(
        found,
        "Expected default User-Agent after clearing override. Got: {entries:#?}"
    );
}
