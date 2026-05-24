//! End-to-end tests for the **Teams** WASM client plugin.
//!
//! Spins up a `poly_test_teams` mock server on a random port, configures the
//! WASM guest to point at it via `teams.base_url` plugin storage, then
//! exercises the full `ClientBackend` API through the Component Model host
//! boundary.
//!
//! Enable with: `--features test-teams`

use std::sync::Arc;


use poly_client::{
    AuthCredentials, BackendType, DmsAndGroupsBackend, IsBackend, MessageContent, MessageQuery,
    SocialGraphBackend,
};
use poly_plugin_host::{
    InMemoryPluginStorage, PluginStorageBackend,
    host_impl::PluginHostState,
};
use poly_test_teams::{TeamsState, router};
use tokio::net::TcpListener;

use super::harness;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Test server helpers
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

        let state = Arc::new(TeamsState::new());
        state.seed();

        let app = router(Arc::clone(&state));
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    rx.await.ok();
                })
                .await
                .ok();
        });
        Self {
            base_url,
            _shutdown: tx,
        }
    }

    /// Obtain a bearer token for the given display name from the easy-signin endpoint.
    async fn token_for(&self, display_name: &str) -> String {
        let resp: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/test/auth/token", self.base_url))
            .json(&serde_json::json!({ "username": display_name }))
            .send()
            .await
            .expect("test_auth_token POST")
            .json()
            .await
            .expect("parse token response");
        resp["token"].as_str().expect("token field").to_string()
    }
}

/// Load the Teams WASM plugin with a pre-seeded storage entry pointing at
/// `base_url` so the guest skips `graph.microsoft.com` and hits our mock.
async fn load_teams_with_server(
    base_url: &str,
) -> Result<poly_plugin_host::PluginBackend, Box<dyn std::error::Error>> {
    let storage = Arc::new(InMemoryPluginStorage::new());
    // The guest reads "teams.base_url" from plugin-global storage.
    storage
        .set("teams", "teams.base_url", base_url.as_bytes().to_vec())
        .await
        .map_err(|e| format!("storage pre-seed failed: {e}"))?;

    let host_state = PluginHostState::new_with_storage("teams", storage);
    poly_plugin_loader_tests::load_plugin_with_host_state("teams", "poly_teams.wasm", host_state)
        .await
}

// ---------------------------------------------------------------------------
// Identity (unchanged — deterministic, no auth required)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn teams_backend_type() -> TestResult {
    let backend = poly_plugin_loader_tests::load_plugin("teams", "poly_teams.wasm").await?;
    harness::assert_backend_type(&backend, BackendType::from("teams"));
    Ok(())
}

#[tokio::test]
async fn teams_backend_name() -> TestResult {
    let backend = poly_plugin_loader_tests::load_plugin("teams", "poly_teams.wasm").await?;
    harness::assert_backend_name(&backend, "Teams");
    Ok(())
}

// ---------------------------------------------------------------------------
// Auth — real guest path via mock server
// ---------------------------------------------------------------------------

#[tokio::test]
async fn teams_authenticate_token_succeeds() -> TestResult {
    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;

    let mut backend = load_teams_with_server(&srv.base_url).await?;
    assert!(!backend.is_authenticated());

    let session = backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| format!("Teams token auth should succeed: {e:?}"))?;

    assert!(backend.is_authenticated());
    assert_eq!(session.user.display_name, "Sheep");
    assert_eq!(session.backend, BackendType::from("teams"));
    assert!(!session.id.is_empty(), "session.id non-empty");
    assert!(!session.user.id.is_empty(), "user.id non-empty");
    Ok(())
}

#[tokio::test]
async fn teams_authenticate_email_password_succeeds() -> TestResult {
    let srv = TestServer::start().await;
    let mut backend = load_teams_with_server(&srv.base_url).await?;

    let session = backend
        .authenticate(AuthCredentials::EmailPassword {
            email: "sheep@contoso.com".to_string(),
            password: "testpass123".to_string(),
        })
        .await
        .map_err(|e| format!("Teams email+password auth should succeed: {e:?}"))?;

    assert_eq!(session.user.display_name, "Sheep");
    assert_eq!(session.backend, BackendType::from("teams"));
    Ok(())
}

#[tokio::test]
async fn teams_authenticate_bad_token_returns_error() -> TestResult {
    let srv = TestServer::start().await;
    let mut backend = load_teams_with_server(&srv.base_url).await?;

    let result = backend
        .authenticate(AuthCredentials::Token("not-a-real-token".to_string()))
        .await;
    assert!(result.is_err(), "bad token should fail");
    Ok(())
}

#[tokio::test]
async fn teams_is_not_authenticated_before_login() -> TestResult {
    let srv = TestServer::start().await;
    let backend = load_teams_with_server(&srv.base_url).await?;
    assert!(!backend.is_authenticated());
    Ok(())
}

#[tokio::test]
async fn teams_logout_clears_auth() -> TestResult {
    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;
    let mut backend = load_teams_with_server(&srv.base_url).await?;

    backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| format!("auth should succeed: {e:?}"))?;
    assert!(backend.is_authenticated());

    harness::logout_succeeds(&mut backend).await;
    assert!(!backend.is_authenticated());
    Ok(())
}

// ---------------------------------------------------------------------------
// Data retrieval — authenticated, real mock-server responses
// ---------------------------------------------------------------------------

#[tokio::test]
async fn teams_get_servers_returns_real_data() -> TestResult {
    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;
    let mut backend = load_teams_with_server(&srv.base_url).await?;
    backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| format!("auth: {e:?}"))?;

    let servers = backend
        .get_servers()
        .await
        .map_err(|e| format!("get_servers: {e:?}"))?;

    assert!(!servers.is_empty(), "Sheep should have teams");
    let names: Vec<_> = servers.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Contoso Corp"), "Contoso Corp present");
    assert!(names.contains(&"Project Alpha"), "Project Alpha present");
    for s in &servers {
        assert_eq!(s.backend, BackendType::from("teams"));
        assert!(!s.id.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn teams_get_servers_empty_when_unauthenticated() -> TestResult {
    let srv = TestServer::start().await;
    let backend = load_teams_with_server(&srv.base_url).await?;
    // Guest returns empty list (not error) when unauthenticated.
    let servers = backend
        .get_servers()
        .await
        .map_err(|e| format!("get_servers unauthenticated: {e:?}"))?;
    assert!(servers.is_empty(), "unauthenticated guest returns empty list");
    Ok(())
}

#[tokio::test]
async fn teams_get_server_by_id() -> TestResult {
    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;
    let mut backend = load_teams_with_server(&srv.base_url).await?;
    backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| format!("auth: {e:?}"))?;

    let server = backend
        .get_server("T001")
        .await
        .map_err(|e| format!("get_server T001: {e:?}"))?;
    assert_eq!(server.id, "T001");
    assert_eq!(server.name, "Contoso Corp");
    assert_eq!(server.backend, BackendType::from("teams"));
    Ok(())
}

#[tokio::test]
async fn teams_get_server_not_found() -> TestResult {
    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;
    let mut backend = load_teams_with_server(&srv.base_url).await?;
    backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| format!("auth: {e:?}"))?;

    harness::get_server_not_found(&backend).await;
    Ok(())
}

#[tokio::test]
async fn teams_get_channels_returns_real_data() -> TestResult {
    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;
    let mut backend = load_teams_with_server(&srv.base_url).await?;
    backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| format!("auth: {e:?}"))?;

    let channels = backend
        .get_channels("T001")
        .await
        .map_err(|e| format!("get_channels T001: {e:?}"))?;

    assert!(!channels.is_empty(), "T001 has channels");
    let names: Vec<_> = channels.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"General"), "General present");
    assert!(names.contains(&"Engineering"), "Engineering present");
    for ch in &channels {
        assert_eq!(ch.server_id, "T001");
        assert!(!ch.id.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn teams_get_channel_not_found() -> TestResult {
    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;
    let mut backend = load_teams_with_server(&srv.base_url).await?;
    backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| format!("auth: {e:?}"))?;

    harness::get_channel_not_found(&backend).await;
    Ok(())
}

#[tokio::test]
async fn teams_get_messages_returns_real_data() -> TestResult {
    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;
    let mut backend = load_teams_with_server(&srv.base_url).await?;
    backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| format!("auth: {e:?}"))?;

    let msgs = backend
        .get_messages(
            "T001/CH001",
            MessageQuery {
                limit: Some(10),
                before: None,
                after: None,
                around: None,
            },
        )
        .await
        .map_err(|e| format!("get_messages T001/CH001: {e:?}"))?;

    assert!(!msgs.is_empty(), "T001/CH001 has seeded messages");
    let has_morning = msgs.iter().any(|m| {
        matches!(&m.content, MessageContent::Text(t) if t.contains("Good morning"))
    });
    assert!(has_morning, "seeded 'Good morning' message present");
    Ok(())
}

#[tokio::test]
async fn teams_send_message_returns_sent_message() -> TestResult {
    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;
    let mut backend = load_teams_with_server(&srv.base_url).await?;
    backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| format!("auth: {e:?}"))?;

    let sent = harness::send_text_message(&backend, "T001/CH001", "WASM e2e test message").await?;
    assert_eq!(
        sent.content,
        MessageContent::Text("WASM e2e test message".to_string())
    );
    assert!(!sent.id.is_empty(), "sent message has id");
    Ok(())
}

#[tokio::test]
async fn teams_send_then_read_message() -> TestResult {
    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;
    let mut backend = load_teams_with_server(&srv.base_url).await?;
    backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| format!("auth: {e:?}"))?;

    let sent = harness::send_text_message(&backend, "T001/CH002", "roundtrip check").await?;

    let msgs = backend
        .get_messages(
            "T001/CH002",
            MessageQuery {
                limit: Some(20),
                before: None,
                after: None,
                around: None,
            },
        )
        .await
        .map_err(|e| format!("get_messages after send: {e:?}"))?;

    assert!(
        msgs.iter().any(|m| m.id == sent.id),
        "sent message appears in subsequent get_messages"
    );
    Ok(())
}

#[tokio::test]
async fn teams_set_presence_ok() -> TestResult {
    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;
    let mut backend = load_teams_with_server(&srv.base_url).await?;
    backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| format!("auth: {e:?}"))?;

    harness::set_presence(&backend, poly_client::PresenceStatus::Online).await
}

#[tokio::test]
async fn teams_set_presence_unauthenticated_is_noop() -> TestResult {
    // Guest preserves stub no-op when unauthenticated — set_presence returns Ok(()).
    let srv = TestServer::start().await;
    let backend = load_teams_with_server(&srv.base_url).await?;
    harness::set_presence(&backend, poly_client::PresenceStatus::Online).await
}

#[tokio::test]
async fn teams_event_stream() -> TestResult {
    let backend = poly_plugin_loader_tests::load_plugin("teams", "poly_teams.wasm").await?;
    harness::event_stream_is_valid(&backend);
    Ok(())
}

#[tokio::test]
async fn teams_get_user() -> TestResult {
    let srv = TestServer::start().await;
    let token = srv.token_for("Sheep").await;
    let mut backend = load_teams_with_server(&srv.base_url).await?;
    backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| format!("auth: {e:?}"))?;

    let user = backend
        .get_user("U002")
        .await
        .map_err(|e| format!("get_user U002: {e:?}"))?;
    assert_eq!(user.id, "U002");
    assert_eq!(user.display_name, "Walrus");
    assert_eq!(user.backend, BackendType::from("teams"));
    Ok(())
}
