//! User-Agent override test for `poly-discord`.
//!
//! Phase G.1 / Phase B Fix-up of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! Verifies that `set_client_version_override` propagates the override string
//! to the wire `User-Agent` header on every outbound request.
//! `apply_version_headers()` is now called from all request methods in
//! `DiscordHttpClient` — both `get()` / `post_json()` and the ad-hoc builders.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::let_underscore_must_use,
    clippy::map_unwrap_or
)]

use std::sync::Arc;


use poly_discord::DiscordClient;
use poly_test_discord::{DiscordState, router};
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

        let state = Arc::new(DiscordState::new());
        state.seed();
        *state.gateway_url.write().await =
            format!("ws://127.0.0.1:{port}/gateway/ws");

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

    async fn token_for(&self, username: &str) -> String {
        let resp: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/test/auth/token", self.base_url))
            .json(&serde_json::json!({ "username": username }))
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

/// Override reaches the wire User-Agent header.
#[tokio::test]
async fn test_version_override_reaches_wire() {
    let srv = TestServer::start().await;
    let token = srv.token_for("koala").await;
    let mut client = DiscordClient::with_base_url(srv.base_url.clone());
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    client
        .set_client_version_override(Some("test-version/1.2.3".to_string()))
        .await
        .expect("set_client_version_override must not error");

    assert_eq!(
        client.client_version(),
        "test-version/1.2.3",
        "client_version() must return the override string"
    );

    // Trigger a request to the mock server.
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
        "Expected User-Agent: test-version/1.2.3 on wire after override. Got: {entries:#?}"
    );
}

/// After clearing, `client_version()` returns the default User-Agent.
#[tokio::test]
async fn test_version_override_clear_restores_default() {
    // Phase B: DEFAULT_CLIENT_VERSION is now the browser-style UA (no DiscordBot).
    const DEFAULT_UA: &str =
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
         (KHTML, like Gecko) discord/0.0.354133 Chrome/130.0.0.0 Electron/32.2.7 Safari/537.36";

    let srv = TestServer::start().await;
    let token = srv.token_for("koala").await;
    let mut client = DiscordClient::with_base_url(srv.base_url.clone());
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
}
