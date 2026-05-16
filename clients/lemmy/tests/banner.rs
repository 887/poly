//! Banner round-trip tests for the Lemmy client.
//!
//! Exercises `ClientBackend::update_server_banner` against the in-process mock
//! Lemmy server and verifies that the change is visible in a subsequent
//! `get_server` call.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]


use poly_client::{
    IsBackend,
    ServerAdminBackend, AuthCredentials,
};
use poly_lemmy::LemmyClient;
use tokio::net::TcpListener;

async fn start_test_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = poly_test_lemmy::router();
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .await
            .unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    format!("http://127.0.0.1:{port}")
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

/// Set a banner URL and verify it round-trips through `get_server`.
#[tokio::test]
async fn set_banner_url_persists() {
    let base_url = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    // Community 1 (Rust) is seeded with no banner.
    let server_id = "lemmy-community-1";

    let result = client
        .update_server_banner(server_id, Some("https://example.com/rust-banner.png"))
        .await;
    assert!(result.is_ok(), "update_server_banner should succeed: {result:?}");

    // Re-fetch and verify the banner is present.
    let server = client
        .get_server(server_id)
        .await
        .expect("get_server after banner update");
    assert_eq!(
        server.banner_url.as_deref(),
        Some("https://example.com/rust-banner.png"),
        "banner_url should reflect the update"
    );
}

/// Clear a banner by passing `None`.
#[tokio::test]
async fn clear_banner_url() {
    let base_url = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let server_id = "lemmy-community-2";

    // Set, then clear.
    client
        .update_server_banner(server_id, Some("https://example.com/prog-banner.png"))
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

/// Invalid server ID returns an error, not a panic.
#[tokio::test]
async fn invalid_server_id_returns_error() {
    let base_url = start_test_server().await;
    let client = authenticated_client(&base_url).await;

    let result = client
        .update_server_banner("not-a-lemmy-id", Some("https://example.com/banner.png"))
        .await;
    assert!(
        result.is_err(),
        "invalid server_id should return Err, not Ok"
    );
}
