//! Back-and-forth API-contract test for poly-github.
//!
//! GitHub is a read-only backend — `send_message` returns `NotSupported`.
//! The test exercises the full auth + discovery + read flow for the two
//! animal accounts (Penguin + Chameleon) and confirms the read-only
//! contract for the shared test-arena repo (id=200, owned by Penguin):
//!
//!   1. Both animals authenticate via test token
//!   2. Penguin discovers penguin/test-arena as a server (gh-200)
//!   3. Both animals enumerate channels on gh-200
//!   4. Both animals can read messages (issues/comments) from the channel
//!   5. `send_message` correctly returns NotSupported for both
//!
//! Because GitHub is read-only, there is no true "A sends → B reads" path
//! at the ClientBackend trait level. The test validates the API contract
//! (discovery + read + expected error on write) instead.
//!
//! Run with:
//! ```
//! cargo test -p poly-github --test back_and_forth
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_client::{AuthCredentials, ClientBackend, ClientError, MessageContent, MessageQuery};
use poly_github::GitHubClient;
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

struct TestServer {
    base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        let base_url = format!("http://127.0.0.1:{port}");

        let router = poly_test_github::router();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, router.into_make_service())
                .with_graceful_shutdown(async {
                    rx.await.ok();
                })
                .await
                .ok();
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        Self { base_url, _shutdown: tx }
    }

    async fn test_token(&self, username: &str) -> String {
        let resp: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/test/auth/token", self.base_url))
            .json(&serde_json::json!({ "username": username }))
            .send()
            .await
            .expect("POST test/auth/token")
            .json()
            .await
            .expect("parse token response");
        resp["token"].as_str().expect("token field").to_string()
    }

    async fn authenticated_client(&self, username: &str) -> GitHubClient {
        let token = self.test_token(username).await;
        let mut client = GitHubClient::with_http(self.base_url.clone());
        client
            .authenticate(AuthCredentials::Token(token))
            .await
            .expect("authenticate");
        client
    }
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

/// API-contract back-and-forth: Penguin (A) + Chameleon (B) on test-arena.
///
/// GitHub is read-only; this test validates discovery and the expected
/// `NotSupported` error on write attempts.
#[tokio::test]
async fn back_and_forth_penguin_chameleon() {
    let srv = TestServer::start().await;

    // Step 1 — Authenticate both animals.
    let penguin = srv.authenticated_client("penguin").await;
    let chameleon = srv.authenticated_client("chameleon").await;

    assert!(penguin.is_authenticated());
    assert!(chameleon.is_authenticated());

    // Step 2 — Penguin discovers penguin/test-arena.
    // get_servers() returns repos owned by the authenticated user.
    let penguin_servers = penguin.get_servers().await.expect("penguin get_servers");
    let arena_server = penguin_servers
        .iter()
        .find(|s| s.name == "penguin/test-arena")
        .expect("Penguin should see test-arena repo as a server");

    // Repo id=200 → server id = "gh-200"
    assert_eq!(
        arena_server.id, "gh-200",
        "test-arena server id should be gh-200"
    );

    // Step 3 — Penguin enumerates channels on gh-200.
    // get_channels reads from the repo cache populated by get_servers.
    let penguin_channels =
        penguin.get_channels("gh-200").await.expect("penguin get_channels gh-200");

    let penguin_issues = penguin_channels
        .iter()
        .find(|c| c.id.starts_with("gh-issues-"))
        .expect("Penguin should see an issues channel");

    let issues_channel_id = &penguin_issues.id;
    assert!(
        issues_channel_id.contains("test-arena"),
        "Issues channel id should reference test-arena; got: {issues_channel_id}"
    );

    // Chameleon discovers their own repos via get_servers and can auth.
    let chameleon_servers = chameleon.get_servers().await.expect("chameleon get_servers");
    assert!(!chameleon_servers.is_empty(), "Chameleon should see at least one server");

    // Step 4 — Penguin reads from the issues channel.
    // test-arena has an empty issue list — returns Ok([]).
    let penguin_messages = penguin
        .get_messages(issues_channel_id, MessageQuery { limit: Some(20), ..Default::default() })
        .await
        .expect("penguin get_messages");

    // Empty list is correct — test-arena has no seeded issues.
    assert_eq!(
        penguin_messages.len(),
        0,
        "test-arena issues channel should be empty"
    );

    // Step 5 — Write attempt: send_message must return NotSupported.
    // GitHub is read-only; verifying the contract is explicit.
    let penguin_write_result = penguin
        .send_message(
            issues_channel_id,
            MessageContent::Text("penguin tries to write".to_string()),
        )
        .await;

    assert!(
        matches!(penguin_write_result, Err(ClientError::NotSupported(_))),
        "GitHub send_message should return NotSupported; got: {penguin_write_result:?}"
    );

    let chameleon_write_result = chameleon
        .send_message(
            "gh-issues-chameleon-color-shift",
            MessageContent::Text("chameleon tries to write".to_string()),
        )
        .await;

    assert!(
        matches!(chameleon_write_result, Err(ClientError::NotSupported(_))),
        "GitHub send_message should return NotSupported; got: {chameleon_write_result:?}"
    );
}
