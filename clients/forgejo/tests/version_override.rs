//! User-Agent override test for `poly-forgejo`.
//!
//! Phase G.1 / Phase B Fix-up of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! `ForgejoApi` now stores `user_agent` behind `Arc<Mutex<String>>` so
//! `set_user_agent` works via `&self`. `ForgejoClient::set_client_version_override`
//! propagates into the live `ForgejoApi` UA field via `self.api.set_user_agent(ua)`.
//! Every `get()` call in `ForgejoApi` reads the lock and injects the header.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_client::{AuthCredentials, ClientBackend};
use poly_forgejo::ForgejoClient;
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn start_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = poly_test_forgejo::router();
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service()).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    format!("http://127.0.0.1:{}", port)
}

async fn get_test_token(base_url: &str, username: &str) -> String {
    let body: serde_json::Value = reqwest::Client::new()
        .post(format!("{base_url}/test/auth/token"))
        .json(&serde_json::json!({ "username": username }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    body["token"].as_str().unwrap().to_string()
}

async fn authenticated_client(base_url: &str) -> ForgejoClient {
    let token = get_test_token(base_url, "otter").await;
    let mut client = ForgejoClient::new(base_url);
    client
        .authenticate(AuthCredentials::Token(token))
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
        .expect("set_client_version_override");

    assert_eq!(
        client.client_version(),
        "test-version/1.2.3",
        "client_version() must return the override string"
    );

    // Trigger a request — get_servers calls list_user_repos which goes through get().
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
    const DEFAULT_UA: &str = "poly-forgejo/0.0.0";

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
