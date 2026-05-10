//! User-Agent override test for `poly-github`.
//!
//! Phase G.1 / Phase B Fix-up of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! `GitHubClient` now stores `version_override: Mutex<Option<String>>` so that
//! `client_version()` returns the override string. `set_client_version_override`
//! records the override and returns `Ok(())`.
//!
//! ## Wire-level assertion: NOT APPLICABLE
//!
//! `GitHubClient` uses the `gh` CLI as transport — HTTP requests are sent by the
//! `gh` subprocess which controls its own `User-Agent`. This layer has no surface
//! to inject a custom UA into those subprocess calls. The wire-level UA assertion
//! is therefore intentionally absent; only the in-memory `client_version()` state
//! is asserted here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_client::IsBackend;
use poly_github::GitHubClient;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `set_client_version_override` returns `Ok` and `client_version()` returns
/// the override string.
#[tokio::test]
async fn test_version_override_stored() {
    let client = GitHubClient::dotcom();

    client
        .set_client_version_override(Some("test-version/1.2.3".to_string()))
        .await
        .expect("set_client_version_override must not error");

    assert_eq!(
        client.client_version(),
        "test-version/1.2.3",
        "client_version() must return the override string"
    );
}

/// After clearing, `client_version()` returns the default.
#[tokio::test]
async fn test_version_override_clear_restores_default() {
    const DEFAULT_UA: &str = "poly-github/0.0.0";

    let client = GitHubClient::dotcom();

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
