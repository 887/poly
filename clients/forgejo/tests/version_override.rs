//! User-Agent override test for `poly-forgejo`.
//!
//! Phase G.1 of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! ## Wire-level assertion: DEFERRED
//!
//! `ForgejoApi` stores `user_agent` by value (`String`, not `Arc<Mutex<String>>`)
//! so `set_user_agent` requires `&mut self`.  `ForgejoClient::set_client_version_override`
//! stores the override in `self.version_override` (so `client_version()` returns it)
//! but does NOT propagate it into the live `ForgejoApi`'s UA field.  A follow-up
//! migration to `Arc<Mutex<String>>` will complete the wire-up; see the comment
//! in `clients/forgejo/src/lib.rs::set_client_version_override`.
//!
//! Until then this file asserts:
//!   1. `client_version()` returns the override string.
//!   2. `client_version()` returns the default after clearing.
//!   3. The mock server is reachable (authenticate succeeds).
//!
//! The wire-level `User-Agent` assertion is left as a `// TODO` and will be
//! promoted to a hard assertion when the wire-up lands.

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `client_version()` returns the override string after `set_client_version_override`.
///
/// Wire-level assertion deferred — see module doc comment.
#[tokio::test]
async fn test_version_override_client_version() {
    let base_url = start_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    client
        .set_client_version_override(Some("test-version/1.2.3".to_string()))
        .await
        .expect("set_client_version_override");

    assert_eq!(
        client.client_version(),
        "test-version/1.2.3",
        "client_version() must return the override string"
    );

    // TODO(Phase G wire-up): When ForgejoApi is migrated to Arc<Mutex<String>>,
    // add a wire assertion here using /test/inspect/last-headers.
}

/// After clearing, `client_version()` returns the default.
#[tokio::test]
async fn test_version_override_clear_restores_default() {
    const DEFAULT_UA: &str = "poly-forgejo/0.0.0";

    let base_url = start_server().await;
    let token = get_test_token(&base_url, "otter").await;
    let mut client = ForgejoClient::new(&base_url);
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
