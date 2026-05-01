//! User-Agent override test for `poly-stoat`.
//!
//! Phase G.1 / Phase B Fix-up of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! `StoatHttpClient::request()` injects `User-Agent` on every outbound call.
//! `set_client_version_override` in `impl ClientBackend for StoatClient` calls
//! `self.http.set_user_agent(ua)` which updates the RwLock-backed field.
//! All subsequent `request()` / `authenticated_request()` calls pick it up.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;

use poly_client::{AuthCredentials, ClientBackend};
use poly_stoat::StoatClient;
use poly_test_common::TestServerBase;
use poly_test_stoat::{StoatState, router};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn start_server() -> (String, tokio::sync::oneshot::Sender<()>) {
    let state = Arc::new(StoatState::new());
    state.seed();

    let base = TestServerBase::bind(0).await.expect("bind random port");
    let base_url = base.base_url();

    let app = router(Arc::clone(&state));
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        axum::serve(base.listener, app)
            .with_graceful_shutdown(async { let _ = rx.await; })
            .await
            .expect("serve");
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    (base_url, tx)
}

async fn authenticated_client(base_url: &str) -> StoatClient {
    let mut client = StoatClient::with_base_url(base_url).expect("valid base url");
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "stoat".into(),
            password: "testpass123".into(),
        })
        .await
        .expect("authenticate");
    client
}

async fn captured_headers(base_url: &str) -> Vec<serde_json::Value> {
    let body: serde_json::Value = reqwest::Client::new()
        .get(format!("{base_url}/test/inspect/last-headers"))
        .send()
        .await
        .expect("GET /test/inspect/last-headers")
        .json()
        .await
        .expect("parse inspect response");
    body.as_array().expect("array").clone()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Override reaches the wire User-Agent header.
#[tokio::test]
async fn test_version_override_reaches_wire() {
    let (base_url, _shutdown) = start_server().await;
    let client = authenticated_client(&base_url).await;

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
    let _ = client.get_servers().await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let entries = captured_headers(&base_url).await;
    let found = entries.iter().any(|e| {
        e["headers"]["user-agent"]
            .as_str()
            .map(|ua| ua == "test-version/1.2.3")
            .unwrap_or(false)
    });

    assert!(
        found,
        "Expected User-Agent: test-version/1.2.3 on wire. Got: {entries:#?}"
    );
}

/// After clearing, `client_version()` returns the default and the wire UA is restored.
#[tokio::test]
async fn test_version_override_clear_restores_default() {
    const DEFAULT_UA: &str = "poly-stoat/0.0.0";

    let (base_url, _shutdown) = start_server().await;
    let client = authenticated_client(&base_url).await;

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

    let entries = captured_headers(&base_url).await;
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
