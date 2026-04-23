//! Banner round-trip tests for the Discord client.
//!
//! Exercises `ClientBackend::update_server_banner` against the in-process mock
//! Discord server and verifies the CDN-URL mapping for guild banner hashes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code
)]

use std::sync::Arc;

use poly_client::{AuthCredentials, ClientBackend};
use poly_discord::DiscordClient;
use poly_test_discord::{DiscordState, router};
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Test server helpers
// ---------------------------------------------------------------------------

struct TestServer {
    base_url: String,
    ws_url: String,
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
        *state.gateway_url.write().await = ws_url.clone();

        let app = router(Arc::clone(&state));
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { rx.await.ok(); })
                .await
                .ok();
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        Self { base_url, ws_url: ws_url.clone(), state, _shutdown: tx }
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
// Tests
// ---------------------------------------------------------------------------

/// Set a banner (using a hash-like token) and verify the CDN URL is built
/// from it in `get_server`.
///
/// The test server stores the banner value as-is; the Poly Discord client
/// maps it as a hash and builds `{cdn_base}/banners/{guild_id}/{hash}.png`.
#[tokio::test]
async fn set_banner_url_persists() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;

    // Guild 100 (Australiana) — use a hash-like token so we can predict
    // the CDN URL the client will construct.
    let server_id = "100";
    let banner_hash = "abc123def456";

    let result = client.update_server_banner(server_id, Some(banner_hash)).await;
    assert!(result.is_ok(), "update_server_banner should succeed: {result:?}");

    let server = client
        .get_server(server_id)
        .await
        .expect("get_server after banner update");

    // The client builds: `{cdn_base}/banners/{id}/{hash}.png`
    let expected = format!("{}/banners/{server_id}/{banner_hash}.png", srv.base_url);
    assert_eq!(
        server.banner_url.as_deref(),
        Some(expected.as_str()),
        "banner_url should be the CDN-constructed URL"
    );
}

/// Clear a banner by passing `None`.
#[tokio::test]
async fn clear_banner_url() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;

    let server_id = "101";

    client
        .update_server_banner(server_id, Some("somehash"))
        .await
        .expect("set banner");

    client
        .update_server_banner(server_id, None)
        .await
        .expect("clear banner");

    let server = client
        .get_server(server_id)
        .await
        .expect("get_server after clear");
    assert!(
        server.banner_url.is_none(),
        "banner_url should be None after clearing, got: {:?}",
        server.banner_url
    );
}

/// Update requires authentication — unauthenticated client returns an error.
#[tokio::test]
async fn unauthenticated_banner_update_fails() {
    let srv = TestServer::start().await;
    let client = DiscordClient::with_base_url(srv.base_url.clone());

    let result = client
        .update_server_banner("100", Some("https://cdn.example.com/banner.png"))
        .await;
    assert!(
        result.is_err(),
        "unauthenticated update should fail, got Ok"
    );
}

/// Banner hash in guild response is mapped to a CDN URL by `get_servers`.
///
/// The test server stores the raw value; the Poly Discord client treats it
/// as a hash and builds `{cdn_base}/banners/{id}/{hash}.png`.
#[tokio::test]
async fn get_servers_includes_banner_url_when_set() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;

    let banner_hash = "deadbeef1234";
    client
        .update_server_banner("100", Some(banner_hash))
        .await
        .expect("set banner");

    // List all servers — guild 100 should have the CDN-constructed banner URL.
    let servers = client.get_servers().await.expect("get_servers");
    let australiana = servers
        .iter()
        .find(|s| s.id == "100")
        .expect("guild 100 not found");

    let expected = format!("{}/banners/100/{banner_hash}.png", srv.base_url);
    assert_eq!(
        australiana.banner_url.as_deref(),
        Some(expected.as_str()),
        "banner_url should be the CDN URL built from the hash"
    );
}
