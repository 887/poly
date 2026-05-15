//! Back-and-forth API-contract test for poly-forgejo.
//!
//! Forgejo is a read-only backend — `send_message` returns `NotSupported`.
//! The test exercises the full auth + discovery + read flow for the two
//! animal accounts (Otter + Flamingo) and confirms the read-only contract
//! for the shared test-arena repo (id=10, owned by Otter):
//!
//!   1. Both animals authenticate via test token
//!   2. Otter discovers otter/test-arena as a server (fj-10)
//!   3. Both animals enumerate channels on fj-10
//!   4. Both animals can read messages from the issues channel
//!   5. `send_message` correctly returns NotSupported for both
//!
//! Because Forgejo is read-only, there is no true "A sends → B reads" path
//! at the ClientBackend trait level. The test validates the API contract
//! (discovery + read + expected error on write) instead.
//!
//! Run with:
//! ```
//! cargo test -p poly-forgejo --test back_and_forth
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]


use poly_client::{
    IsBackend, MessagingBackend, ModerationBackend, SocialGraphBackend, DmsAndGroupsBackend,
    ServerAdminBackend, CodeRepoBackend, AuthCredentials, BackendType, ChannelType, ClientError,
    ClientEvent, MessageContent, MessageQuery, PresenceStatus, SettingsScope, ViewBody, ViewKind,
    UpdateChannelParams,
};
use poly_forgejo::ForgejoClient;
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

        let router = poly_test_forgejo::router();
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

    async fn authenticated_client(&self, username: &str) -> ForgejoClient {
        let token = self.test_token(username).await;
        let mut client = ForgejoClient::new(&self.base_url);
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

/// API-contract back-and-forth: Otter (A) + Flamingo (B) on test-arena.
///
/// Forgejo is read-only; this test validates discovery and the expected
/// `NotSupported` error on write attempts.
#[tokio::test]
async fn back_and_forth_otter_flamingo() {
    let srv = TestServer::start().await;

    // Step 1 — Authenticate both animals.
    let otter = srv.authenticated_client("otter").await;
    let flamingo = srv.authenticated_client("flamingo").await;

    assert!(otter.is_authenticated());
    assert!(flamingo.is_authenticated());

    // Step 2 — Otter discovers otter/test-arena.
    // get_servers() returns repos owned by the authenticated user.
    let otter_servers = otter.get_servers().await.expect("otter get_servers");
    let arena_server = otter_servers
        .iter()
        .find(|s| s.name == "otter/test-arena")
        .expect("Otter should see test-arena repo as a server");

    // Repo id=10 → server id = "fj-10"
    assert_eq!(
        arena_server.id, "fj-10",
        "test-arena server id should be fj-10"
    );

    // Step 3 — Otter enumerates channels on fj-10.
    // get_channels reads from the repo cache populated by get_servers.
    let otter_channels =
        otter.get_channels("fj-10").await.expect("otter get_channels fj-10");

    let otter_issues = otter_channels
        .iter()
        .find(|c| c.id.starts_with("fj-issues-"))
        .expect("Otter should see an issues channel");

    let issues_channel_id = &otter_issues.id;
    assert!(
        issues_channel_id.contains("test-arena"),
        "Issues channel id should reference test-arena; got: {issues_channel_id}"
    );

    // Flamingo discovers their own repos via get_servers and can auth.
    let flamingo_servers = flamingo.get_servers().await.expect("flamingo get_servers");
    assert!(!flamingo_servers.is_empty(), "Flamingo should see at least one server");

    // Step 4 — Otter reads from the issues channel.
    // test-arena has an empty issue list — returns Ok([]).
    let otter_messages = otter
        .get_messages(issues_channel_id, MessageQuery { limit: Some(20), ..Default::default() })
        .await
        .expect("otter get_messages");

    // Empty list is correct — test-arena has no seeded issues.
    assert_eq!(
        otter_messages.len(),
        0,
        "test-arena issues channel should be empty"
    );

    // Step 5 — Write attempt: send_message must return NotSupported.
    // Forgejo is read-only; verifying the contract is explicit.
    let otter_write_result = otter
        .send_message(
            issues_channel_id,
            MessageContent::Text("otter tries to write".to_string()),
        )
        .await;

    assert!(
        matches!(otter_write_result, Err(ClientError::NotSupported(_))),
        "Forgejo send_message should return NotSupported; got: {otter_write_result:?}"
    );

    let flamingo_write_result = flamingo
        .send_message(
            "fj-issues-flamingo/pink-css",
            MessageContent::Text("flamingo tries to write".to_string()),
        )
        .await;

    assert!(
        matches!(flamingo_write_result, Err(ClientError::NotSupported(_))),
        "Forgejo send_message should return NotSupported; got: {flamingo_write_result:?}"
    );
}
