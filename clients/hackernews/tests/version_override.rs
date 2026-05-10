//! User-Agent override test for `poly-hackernews`.
//!
//! Phase G.1 / Phase B Fix-up of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! `client_version`, `set_client_version_override`, and `get_signup_method`
//! are now in `impl IsBackend for HackerNewsClient`. The UA field is
//! stored on `HnApiClient` behind an `Arc<Mutex<String>>`; every outbound
//! request adds `header("User-Agent", self.ua())`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]


use poly_hackernews::HackerNewsClient;
use poly_test_hackernews::TestHnServer;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn hn_base_url(server: &TestHnServer) -> String {
    format!("{}/v0", server.base_url)
}

async fn authenticated_client(server: &TestHnServer) -> HackerNewsClient {
    let mut client = HackerNewsClient::with_base_url(hn_base_url(server));
    client
        .authenticate(AuthCredentials::Token(String::new()))
        .await
        .expect("guest authenticate");
    client
}

async fn captured_headers(server: &TestHnServer) -> Vec<serde_json::Value> {
    let body: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/test/inspect/last-headers", server.base_url))
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
    let server = TestHnServer::start().await;
    let client = authenticated_client(&server).await;

    client
        .set_client_version_override(Some("test-version/1.2.3".to_string()))
        .await
        .expect("set_client_version_override");

    assert_eq!(
        client.client_version(),
        "test-version/1.2.3",
        "client_version() must return the override string"
    );

    // Trigger a request to the mock server.
    // get_servers() is in-memory; get_messages() with "hn-top" fires get_feed_ids() via HTTP.
    let _ = client.get_messages("hn-top", MessageQuery::default()).await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let entries = captured_headers(&server).await;
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
    const DEFAULT_UA: &str = "poly-hackernews/0.0.0";

    let server = TestHnServer::start().await;
    let client = authenticated_client(&server).await;

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

    let _ = client.get_messages("hn-top", MessageQuery::default()).await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let entries = captured_headers(&server).await;
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
