//! User-Agent override test for `poly-discord`.
//!
//! Phase G.1 of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! ## Wire-level assertion: DEFERRED
//!
//! `DiscordHttpClient` has an `apply_version_headers()` helper that would add
//! `User-Agent` + `X-Super-Properties` per-request, but that helper is never
//! invoked from the actual `get()` / `post_json()` request methods. As a result,
//! `set_client_version_override` stores the override (so `client_version()` is
//! correct) but the value does not reach the wire.
//!
//! Until `DiscordHttpClient::get` / `post_json` are updated to call
//! `apply_version_headers`, the wire assertion stays deferred.
//!
//! This file asserts:
//!   1. `set_client_version_override(Some(_))` does not error.
//!   2. `client_version()` returns the override string.
//!   3. `client_version()` returns the default after clearing.
//!   4. The mock server is reachable (authenticate succeeds).
//!
//! The wire-level `User-Agent` assertion (including `X-Super-Properties`) is
//! left as a `// TODO` and will be promoted to a hard assertion when the
//! wire-up lands in `clients/discord/src/http.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use poly_client::{AuthCredentials, ClientBackend};
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
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `set_client_version_override` succeeds and `client_version()` returns the
/// override string.
///
/// Wire-level assertion deferred — see module doc comment.
#[tokio::test]
async fn test_version_override_client_version() {
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

    // TODO(Phase G wire-up): When DiscordHttpClient::get/post_json call
    // apply_version_headers(), add wire assertions using /test/inspect/last-headers.
    // Discord-specific: also assert X-Super-Properties alongside User-Agent.
}

/// After clearing, `client_version()` returns the default User-Agent.
#[tokio::test]
async fn test_version_override_clear_restores_default() {
    const DEFAULT_UA: &str = "poly-discord/0.0.0 (DiscordBot https://github.com/poly-app; 10)";

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
}
