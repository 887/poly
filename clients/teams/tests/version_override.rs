//! Wire-level User-Agent override test for `poly-teams`.
//!
//! Phase G.1 of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! Spins up the mock Teams/Graph API test server, sets
//! `set_client_version_override`, makes an authenticated request, then
//! verifies the captured `User-Agent` via `/test/inspect/last-headers`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;

use poly_client::{AuthCredentials, ClientBackend};
use poly_teams::TeamsClient;
use poly_test_teams::{TeamsState, router};
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
            .expect("POST /test/auth/token")
            .json()
            .await
            .expect("parse token response");
        resp["token"].as_str().expect("token field").to_string()
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

/// Override reaches the wire User-Agent.
#[tokio::test]
async fn test_version_override_reaches_wire() {
    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;
    let mut client = TeamsClient::with_base_url(srv.base_url.clone());
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    client
        .set_client_version_override(Some("test-version/1.2.3".to_string()))
        .await
        .expect("set_client_version_override");

    // get_servers triggers an HTTP request to the mock server.
    let _ = client.get_servers().await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let entries = srv.captured_headers().await;
    let found = entries.iter().any(|e| {
        e["headers"]["user-agent"]
            .as_str()
            .map(|ua| ua == "test-version/1.2.3")
            .unwrap_or(false)
    });

    assert!(
        found,
        "Expected User-Agent: test-version/1.2.3 after override. Got: {entries:#?}"
    );
}

/// Clearing the override restores the default on the wire.
#[tokio::test]
async fn test_version_override_clear_restores_default() {
    const DEFAULT_UA: &str = "poly-teams/0.0.0";

    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;
    let mut client = TeamsClient::with_base_url(srv.base_url.clone());
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

    let _ = client.get_servers().await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let entries = srv.captured_headers().await;
    let found = entries.iter().any(|e| {
        e["headers"]["user-agent"]
            .as_str()
            .map(|ua| ua == DEFAULT_UA)
            .unwrap_or(false)
    });

    assert!(
        found,
        "Expected default User-Agent after clearing override. Got: {entries:#?}"
    );
    assert_eq!(client.client_version(), DEFAULT_UA);
}
