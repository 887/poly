//! User-Agent override test for `poly-lemmy`.
//!
//! Phase G.1 / Phase B Fix-up of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! All fetch methods in `LemmyHttpClient` now inject `User-Agent` from the
//! `Arc<RwLock<String>>` field. `set_client_version_override` propagates to the
//! field via `self.http.set_user_agent(ua)`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use poly_client::{AuthCredentials, ClientBackend};
use poly_lemmy::LemmyClient;
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn start_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = poly_test_lemmy::router();
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service()).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    format!("http://127.0.0.1:{}", port)
}

async fn authenticated_client(base_url: &str) -> LemmyClient {
    let mut client = LemmyClient::new(base_url);
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "testuser".to_string(),
            password: "password123".to_string(),
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
    let base_url = start_server().await;
    let client = authenticated_client(&base_url).await;

    client
        .set_client_version_override(Some("test-version/1.2.3".to_string()))
        .await
        .expect("set_client_version_override must not error");

    assert_eq!(
        client.client_version(),
        "test-version/1.2.3",
        "client_version() must return the override string"
    );

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
    const DEFAULT_UA: &str = "poly-lemmy/0.0.0";

    let base_url = start_server().await;
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
