//! User-Agent override test for `poly-forgejo`.
//!
//! Phase G.1 / Phase B Fix-up of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! `ForgejoApi` now stores `user_agent` behind `Arc<Mutex<String>>` so
//! `set_user_agent` works via `&self`. `ForgejoClient::set_client_version_override`
//! propagates into the live `ForgejoApi` UA field via `self.api.set_user_agent(ua)`.
//! Every `get()` call in `ForgejoApi` reads the lock and injects the header.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]


use poly_client::{
    IsBackend, AuthCredentials,
};
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
            .is_some_and(|ua| ua == "test-version/1.2.3")
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
            .is_some_and(|ua| ua == DEFAULT_UA)
    });

    assert!(
        found,
        "Expected default User-Agent after clearing override. Got: {entries:#?}"
    );
}

/// The override must reach EVERY request, not just the ones routed through
/// `ForgejoApi::get()`. `is_starred` built its request by hand and omitted the
/// User-Agent header, so a UA-filtering instance rejected the star probe while
/// repo browsing succeeded.
#[tokio::test]
async fn test_version_override_reaches_is_starred_probe() {
    let base_url = start_server().await;
    let client = authenticated_client(&base_url).await;

    client
        .set_client_version_override(Some("test-version/4.5.6".to_string()))
        .await
        .expect("set_client_version_override");

    // The repo context menu probes star state via `ForgejoApi::is_starred`.
    let servers = client.get_servers().await.expect("get_servers");
    let server = servers.first().expect("test fixture seeds at least one repo");
    let _items = client
        .get_context_menu_items(poly_client::MenuTargetKind::Server, &server.id)
        .await
        .expect("context menu should succeed");
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let entries = captured_headers(&base_url).await;
    let starred_requests: Vec<&serde_json::Value> = entries
        .iter()
        .filter(|e| {
            e["path"]
                .as_str()
                .is_some_and(|p| p.contains("/user/starred/"))
        })
        .collect();
    assert!(
        !starred_requests.is_empty(),
        "the context menu must have probed /user/starred/. Got: {entries:#?}"
    );
    for entry in starred_requests {
        assert_eq!(
            entry["headers"]["user-agent"].as_str(),
            Some("test-version/4.5.6"),
            "is_starred must carry the overridden User-Agent: {entry:#?}"
        );
    }
}
