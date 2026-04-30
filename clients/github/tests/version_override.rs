//! User-Agent override test for `poly-github`.
//!
//! Phase G.1 of `docs/plans/plan-client-version-override-and-sandbox.md`.
//!
//! ## Wire-level assertion: DEFERRED
//!
//! `GitHubClient` uses the `gh` CLI as transport — it does not send HTTP
//! requests directly; it spawns `gh api <endpoint>` as a subprocess, which
//! sets its own `User-Agent` internally.  In HTTP-test mode (`GhCli::with_http`),
//! the `api_raw_http` helper creates a plain `HttpClient` without any
//! User-Agent override surface.  Additionally, `GitHubClient` does not
//! implement `set_client_version_override` (the `ClientBackend` default impl
//! returns `Ok(())` as a no-op).
//!
//! Wire-level User-Agent override for GitHub therefore cannot be tested until
//! one of:
//! - `GhCli::with_http` is extended to accept and propagate a UA string, OR
//! - A dedicated HTTP transport is added alongside the CLI transport.
//!
//! Until then this file verifies only that `client_version()` returns a
//! non-empty string (smoke test), confirming the backend compiles correctly.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_client::ClientBackend;
use poly_github::GitHubClient;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `client_version()` returns a non-empty string (smoke test).
///
/// Wire-level override assertion is deferred — see module doc comment.
#[tokio::test]
async fn test_client_version_is_non_empty() {
    let client = GitHubClient::dotcom();
    let ver = client.client_version();
    assert!(!ver.is_empty(), "client_version() must return a non-empty string");
    assert_eq!(ver, "poly-github/0.0.0");
}
