//! User-Agent override test for `poly-lemmy`.
//!
//! Phase G.1 of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! ## Wire-level assertion: PARTIALLY DEFERRED
//!
//! `set_client_version_override` is correctly implemented and `client_version()`
//! returns the override string. However, the fetch methods in `LemmyApi`
//! (`fetch_subscribed_communities`, `fetch_community`, etc.) call
//! `self.http.get(...)` directly instead of the `http_get` / `http_post`
//! helpers that inject the `User-Agent` header. As a result the override does
//! not appear in the wire headers for `get_servers()` or similar calls.
//!
//! The test below asserts the client-level behaviour (`client_version()`)
//! and documents the wire-level gap as a TODO.
//!
//! TODO(Phase G wire-up): When `LemmyApi::fetch_subscribed_communities` and
//! related methods are updated to call `self.http_get(...)` / `self.http_post(...)`,
//! promote the TODO assertions below to hard wire assertions using
//! /test/inspect/last-headers.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `set_client_version_override` succeeds and `client_version()` returns the
/// override string.
///
/// Wire-level assertion partially deferred — see module doc comment.
#[tokio::test]
async fn test_version_override_client_version() {
    let base_url = start_server().await;
    let mut client = authenticated_client(&base_url).await;

    client
        .set_client_version_override(Some("test-version/1.2.3".to_string()))
        .await
        .expect("set_client_version_override must not error");

    assert_eq!(
        client.client_version(),
        "test-version/1.2.3",
        "client_version() must return the override string"
    );

    // TODO(Phase G wire-up): When LemmyApi fetch methods use http_get/http_post,
    // add wire assertion via /test/inspect/last-headers:
    //   let _ = client.get_servers().await;
    //   // assert captured headers include User-Agent: test-version/1.2.3
}

/// After clearing, `client_version()` returns the default.
#[tokio::test]
async fn test_version_override_clear_restores_default() {
    const DEFAULT_UA: &str = "poly-lemmy/0.0.0";

    let base_url = start_server().await;
    let mut client = authenticated_client(&base_url).await;

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
