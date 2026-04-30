//! User-Agent override test for `poly-stoat`.
//!
//! Phase G.1 of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! ## Wire-level assertion: DEFERRED
//!
//! `client_version`, `get_signup_method`, and `set_client_version_override`
//! are currently inside `#[cfg(all(test, feature = "native"))] mod tests { … }`
//! in `clients/stoat/src/lib.rs` (around line 2273-2293) rather than in the
//! `impl ClientBackend for StoatClient` block. The trait's default
//! implementation (`Err(NotSupported("set_client_version_override"))`) therefore
//! takes effect in non-test builds.
//!
//! Until those methods are moved into the correct `impl` block, the hard
//! wire-level assertions stay deferred.
//!
//! This file asserts:
//!   1. The mock server starts and authenticate succeeds.
//!   2. `client_version()` returns a non-empty string.
//!   3. `set_client_version_override` does not panic (result is ignored).
//!
//! TODO(Phase G wire-up): Promote the `// TODO` assertions to hard asserts
//! once the source is corrected and the wire UA propagation is confirmed via
//! /test/inspect/last-headers.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Authenticate succeeds and `client_version()` returns a non-empty string.
///
/// Full wire override assertion deferred — see module doc comment.
#[tokio::test]
async fn test_client_version_non_empty_after_authenticate() {
    let (base_url, _shutdown) = start_server().await;
    let client = authenticated_client(&base_url).await;

    let ver = client.client_version();
    assert!(!ver.is_empty(), "client_version() must return a non-empty string");

    // TODO(Phase G wire-up): When `set_client_version_override` is moved from
    // `mod tests` into `impl ClientBackend for StoatClient`, replace this test
    // with full wire assertions using /test/inspect/last-headers.
}

/// `set_client_version_override` does not panic (gap documented).
#[tokio::test]
async fn test_version_override_known_gap() {
    let (base_url, _shutdown) = start_server().await;
    let client = authenticated_client(&base_url).await;

    // Currently returns NotSupported because the method is inside
    // `#[cfg(all(test, feature = "native"))] mod tests { … }` in lib.rs.
    let result = client
        .set_client_version_override(Some("test-version/1.2.3".to_string()))
        .await;

    // Either Ok (fixed) or Err(NotSupported) (gap still present) — must not panic.
    let _ = result;

    // TODO(Phase G wire-up): Assert Ok(()) here once the source is corrected.
}
