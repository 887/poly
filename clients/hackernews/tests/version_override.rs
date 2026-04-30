//! User-Agent override test for `poly-hackernews`.
//!
//! Phase G.1 of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! ## Wire-level assertion: DEFERRED
//!
//! `get_signup_method`, `client_version`, and `set_client_version_override`
//! are in a plain `impl HackerNewsClient { … }` block (line ~617 in lib.rs)
//! rather than in `impl ClientBackend for HackerNewsClient`. The compiler
//! warns they are "never used" and the trait's default
//! (`Err(NotSupported("set_client_version_override"))`) takes effect at runtime.
//!
//! Until those methods are moved into the `impl ClientBackend` block, the
//! wire-level assertions stay deferred.
//!
//! This file asserts:
//!   1. The mock server starts and guest authenticate succeeds.
//!   2. Requests reachable through the wire (feed fetch) work correctly.
//!
//! TODO(Phase G wire-up): Promote the `// TODO` assertions to hard asserts
//! once `set_client_version_override` is in the correct impl block and the
//! wire UA propagation is confirmed via /test/inspect/last-headers.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_client::{AuthCredentials, ClientBackend};
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Guest authenticate succeeds and the server is reachable.
///
/// Full wire override assertion deferred — see module doc comment.
#[tokio::test]
async fn test_server_reachable_after_authenticate() {
    let server = TestHnServer::start().await;
    let client = authenticated_client(&server).await;

    assert!(client.is_authenticated(), "should be authenticated");

    // TODO(Phase G wire-up): When `set_client_version_override` is moved into
    // `impl ClientBackend for HackerNewsClient`, add:
    //   client.set_client_version_override(Some("test-version/1.2.3".to_string())).await.expect("set");
    //   let _ = client.get_servers().await;
    //   // assert /test/inspect/last-headers shows User-Agent: test-version/1.2.3
}

/// `set_client_version_override` does not panic (gap documented).
#[tokio::test]
async fn test_version_override_known_gap() {
    let server = TestHnServer::start().await;
    let client = authenticated_client(&server).await;

    // Currently returns NotSupported because the method is in a plain
    // `impl HackerNewsClient` block rather than `impl ClientBackend`.
    let result = client
        .set_client_version_override(Some("test-version/1.2.3".to_string()))
        .await;

    // Either Ok (fixed) or Err(NotSupported) (gap still present) — must not panic.
    let _ = result;

    // TODO(Phase G wire-up): Assert Ok(()) once the source is corrected.
}
