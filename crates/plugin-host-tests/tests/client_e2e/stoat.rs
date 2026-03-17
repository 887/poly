//! End-to-end tests for the **Stoat** client plugin through the real WASM guest path.
//!
//! These tests must exercise the guest through the Component Model host boundary,
//! not the native code path. Deterministic host-api HTTP mocks are used so the
//! tests validate real guest logic without depending on external network access.
//!
//! Enable with: `--features test-stoat`

use poly_client::{BackendType, ClientBackend};
use poly_plugin_host::host_impl::{MockHttpResponse, PluginHostState};

use super::harness;

async fn load_stoat() -> poly_plugin_host::PluginBackend {
    poly_plugin_loader_tests::load_plugin("stoat", "poly_stoat.wasm")
        .await
        .unwrap()
}

async fn load_stoat_with_auth_mocks() -> poly_plugin_host::PluginBackend {
    let host_state = PluginHostState::new("stoat")
        .with_mock_http_response(
            "POST",
            "https://api.stoat.chat/auth/session/login",
            Ok(MockHttpResponse {
                status: 200,
                headers: vec![("content-type".to_string(), "application/json".to_string())],
                body: serde_json::to_vec(&serde_json::json!({
                    "result": "Success",
                    "_id": "session_1",
                    "user_id": "user_1",
                    "token": "test-session-token",
                    "name": "Poly"
                }))
                .unwrap(),
            }),
        )
        .with_mock_http_response(
            "GET",
            "https://api.stoat.chat/users/@me",
            Ok(MockHttpResponse {
                status: 200,
                headers: vec![("content-type".to_string(), "application/json".to_string())],
                body: serde_json::to_vec(&serde_json::json!({
                    "_id": "user_1",
                    "username": "stoaty",
                    "display_name": "Stoaty McStoat",
                    "online": true,
                    "status": { "presence": "Focus" }
                }))
                .unwrap(),
            }),
        );

    poly_plugin_loader_tests::load_plugin_with_host_state("stoat", "poly_stoat.wasm", host_state)
        .await
        .unwrap()
}

#[tokio::test]
async fn stoat_backend_type() {
    let backend = load_stoat().await;
    harness::assert_backend_type(&backend, BackendType::Stoat);
}

#[tokio::test]
async fn stoat_backend_name() {
    let backend = load_stoat().await;
    harness::assert_backend_name(&backend, "Stoat");
}

#[tokio::test]
async fn stoat_authenticate_email_password_uses_real_guest_path() {
    let mut backend = load_stoat_with_auth_mocks().await;

    let session = backend
        .authenticate(poly_client::AuthCredentials::EmailPassword {
            email: "alice@example.test".to_string(),
            password: "secret".to_string(),
        })
        .await
        .expect("Stoat mocked guest auth should succeed");

    assert_eq!(session.id, "session_1");
    assert_eq!(session.user.id, "user_1");
    assert_eq!(session.user.display_name, "Stoaty McStoat");
    assert_eq!(session.backend, BackendType::Stoat);
    assert_eq!(session.icon_emoji.as_deref(), Some("🦦"));
    assert_eq!(
        session.backend_url.as_deref(),
        Some("https://api.stoat.chat")
    );
}

#[tokio::test]
async fn stoat_authenticate_token_uses_real_guest_path() {
    let mut backend = load_stoat_with_auth_mocks().await;

    let session = backend
        .authenticate(poly_client::AuthCredentials::Token(
            "test-session-token".to_string(),
        ))
        .await
        .expect("Stoat mocked guest token auth should succeed");

    assert_eq!(session.user.id, "user_1");
    assert_eq!(session.backend, BackendType::Stoat);
}

#[tokio::test]
async fn stoat_is_not_authenticated() {
    let backend = load_stoat().await;
    assert!(!backend.is_authenticated());
}

#[tokio::test]
async fn stoat_dummy_auth_no_longer_returns_stub_marker() {
    let mut backend = load_stoat().await;
    harness::authenticate_does_not_use_stub_path(
        &mut backend,
        poly_client::AuthCredentials::Token("dummy-token".to_string()),
    )
    .await;
}

#[tokio::test]
async fn stoat_get_server_not_found() {
    let backend = load_stoat().await;
    harness::get_server_not_found(&backend).await;
}

#[tokio::test]
async fn stoat_get_channel_not_found() {
    let backend = load_stoat().await;
    harness::get_channel_not_found(&backend).await;
}

#[tokio::test]
async fn stoat_set_presence_ok() {
    let backend = load_stoat().await;
    harness::set_presence(&backend, poly_client::PresenceStatus::Online).await;
}

#[tokio::test]
async fn stoat_event_stream() {
    let backend = load_stoat().await;
    harness::event_stream_is_valid(&backend);
}

#[tokio::test]
async fn stoat_logout() {
    let mut backend = load_stoat_with_auth_mocks().await;
    let _session = backend
        .authenticate(poly_client::AuthCredentials::Token(
            "test-session-token".to_string(),
        ))
        .await
        .expect("mocked token auth succeeds");
    harness::logout_succeeds(&mut backend).await;
}
