//! End-to-end tests for the **Stoat** client plugin through the real WASM guest path.
//!
//! These tests must exercise the guest through the Component Model host boundary,
//! not the native code path. Deterministic host-api HTTP mocks are used so the
//! tests validate real guest logic without depending on external network access.
//!
//! Enable with: `--features test-stoat`


use poly_plugin_host::host_impl::{MockHttpResponse, PluginHostState};

use super::harness;

type TestResult = Result<(), Box<dyn std::error::Error>>;

async fn load_stoat() -> Result<poly_plugin_host::PluginBackend, Box<dyn std::error::Error>> {
    poly_plugin_loader_tests::load_plugin("stoat", "poly_stoat.wasm").await
}

async fn load_stoat_with_auth_mocks(
) -> Result<poly_plugin_host::PluginBackend, Box<dyn std::error::Error>> {
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
                .map_err(|e| format!("json serialization failed: {e:?}"))?,
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
                .map_err(|e| format!("json serialization failed: {e:?}"))?,
            }),
        )
        .with_mock_http_response(
            "GET",
            "https://api.stoat.chat/users/user_2",
            Ok(MockHttpResponse {
                status: 200,
                headers: vec![("content-type".to_string(), "application/json".to_string())],
                body: serde_json::to_vec(&serde_json::json!({
                    "_id": "user_2",
                    "username": "otterpal",
                    "display_name": "Otter Pal",
                    "online": true,
                    "status": { "presence": "Idle" }
                }))
                .map_err(|e| format!("json serialization failed: {e:?}"))?,
            }),
        )
        .with_mock_http_response(
            "GET",
            "https://api.stoat.chat/users/user_2/dm",
            Ok(MockHttpResponse {
                status: 200,
                headers: vec![("content-type".to_string(), "application/json".to_string())],
                body: serde_json::to_vec(&serde_json::json!({
                    "channel_type": "DirectMessage",
                    "_id": "dm_1",
                    "active": true,
                    "recipients": ["user_1", "user_2"],
                    "last_message_id": "msg_dm_1"
                }))
                .map_err(|e| format!("json serialization failed: {e:?}"))?,
            }),
        )
        .with_mock_http_response(
            "GET",
            "https://api.stoat.chat/users/user_1/dm",
            Ok(MockHttpResponse {
                status: 200,
                headers: vec![("content-type".to_string(), "application/json".to_string())],
                body: serde_json::to_vec(&serde_json::json!({
                    "channel_type": "SavedMessages",
                    "_id": "saved_1",
                    "user": "user_1",
                    "last_message_id": "msg_saved_1"
                }))
                .map_err(|e| format!("json serialization failed: {e:?}"))?,
            }),
        )
        .with_mock_http_response(
            "PUT",
            "https://api.stoat.chat/channels/group_1/recipients/user_4",
            Ok(MockHttpResponse {
                status: 204,
                headers: vec![],
                body: vec![],
            }),
        )
        .with_mock_http_response(
            "DELETE",
            "https://api.stoat.chat/channels/group_1/recipients/user_3",
            Ok(MockHttpResponse {
                status: 204,
                headers: vec![],
                body: vec![],
            }),
        );

    poly_plugin_loader_tests::load_plugin_with_host_state("stoat", "poly_stoat.wasm", host_state)
        .await
}

#[tokio::test]
async fn stoat_backend_type() -> TestResult {
    let backend = load_stoat().await?;
    harness::assert_backend_type(&backend, BackendType::from("stoat"));
    Ok(())
}

#[tokio::test]
async fn stoat_backend_name() -> TestResult {
    let backend = load_stoat().await?;
    harness::assert_backend_name(&backend, "Stoat");
    Ok(())
}

#[tokio::test]
async fn stoat_authenticate_email_password_uses_real_guest_path() -> TestResult {
    let mut backend = load_stoat_with_auth_mocks().await?;

    let session = backend
        .authenticate(poly_client::AuthCredentials::EmailPassword {
            email: "alice@example.test".to_string(),
            password: "secret".to_string(),
        })
        .await
        .map_err(|e| format!("Stoat mocked guest auth should succeed: {e:?}"))?;

    assert_eq!(session.id, "session_1");
    assert_eq!(session.user.id, "user_1");
    assert_eq!(session.user.display_name, "Stoaty McStoat");
    assert_eq!(session.backend, BackendType::from("stoat"));
    assert_eq!(session.icon_emoji.as_deref(), Some("🦦"));
    assert_eq!(
        session.backend_url.as_deref(),
        Some("https://api.stoat.chat")
    );
    Ok(())
}

#[tokio::test]
async fn stoat_authenticate_token_uses_real_guest_path() -> TestResult {
    let mut backend = load_stoat_with_auth_mocks().await?;

    let session = backend
        .authenticate(poly_client::AuthCredentials::Token(
            "test-session-token".to_string(),
        ))
        .await
        .map_err(|e| format!("Stoat mocked guest token auth should succeed: {e:?}"))?;

    assert_eq!(session.user.id, "user_1");
    assert_eq!(session.backend, BackendType::from("stoat"));
    Ok(())
}

#[tokio::test]
async fn stoat_is_not_authenticated() -> TestResult {
    let backend = load_stoat().await?;
    assert!(!backend.is_authenticated());
    Ok(())
}

#[tokio::test]
async fn stoat_dummy_auth_no_longer_returns_stub_marker() -> TestResult {
    let mut backend = load_stoat().await?;
    harness::authenticate_does_not_use_stub_path(
        &mut backend,
        poly_client::AuthCredentials::Token("dummy-token".to_string()),
    )
    .await;
    Ok(())
}

#[tokio::test]
async fn stoat_get_server_not_found() -> TestResult {
    let backend = load_stoat().await?;
    harness::get_server_not_found(&backend).await;
    Ok(())
}

#[tokio::test]
async fn stoat_get_channel_not_found() -> TestResult {
    let backend = load_stoat().await?;
    harness::get_channel_not_found(&backend).await;
    Ok(())
}

#[tokio::test]
async fn stoat_set_presence_ok() -> TestResult {
    let backend = load_stoat().await?;
    harness::set_presence(&backend, poly_client::PresenceStatus::Online).await
}

#[tokio::test]
async fn stoat_event_stream() -> TestResult {
    let backend = load_stoat().await?;
    harness::event_stream_is_valid(&backend);
    Ok(())
}

#[tokio::test]
async fn stoat_logout() -> TestResult {
    let mut backend = load_stoat_with_auth_mocks().await?;
    backend
        .authenticate(poly_client::AuthCredentials::Token(
            "test-session-token".to_string(),
        ))
        .await
        .map_err(|e| format!("mocked token auth succeeds: {e:?}"))?;
    harness::logout_succeeds(&mut backend).await;
    Ok(())
}

#[tokio::test]
async fn stoat_open_direct_message_channel_uses_real_guest_path() -> TestResult {
    let mut backend = load_stoat_with_auth_mocks().await?;
    backend
        .authenticate(poly_client::AuthCredentials::Token(
            "test-session-token".to_string(),
        ))
        .await
        .map_err(|e| format!("mocked token auth succeeds: {e:?}"))?;

    let dm = harness::open_direct_message_channel(&backend, "user_2").await?;
    assert_eq!(dm.id, "dm_1");
    assert_eq!(dm.user.id, "user_2");
    assert_eq!(dm.user.display_name, "Otter Pal");
    Ok(())
}

#[tokio::test]
async fn stoat_open_saved_messages_channel_uses_real_guest_path() -> TestResult {
    let mut backend = load_stoat_with_auth_mocks().await?;
    backend
        .authenticate(poly_client::AuthCredentials::Token(
            "test-session-token".to_string(),
        ))
        .await
        .map_err(|e| format!("mocked token auth succeeds: {e:?}"))?;

    let dm = harness::open_saved_messages_channel(&backend).await?;
    assert_eq!(dm.id, "saved_1");
    assert_eq!(dm.user.id, "user_1");
    assert_eq!(dm.user.display_name, "Stoaty McStoat");
    Ok(())
}

#[tokio::test]
async fn stoat_group_member_mutations_use_real_guest_path() -> TestResult {
    let mut backend = load_stoat_with_auth_mocks().await?;
    backend
        .authenticate(poly_client::AuthCredentials::Token(
            "test-session-token".to_string(),
        ))
        .await
        .map_err(|e| format!("mocked token auth succeeds: {e:?}"))?;

    backend
        .add_group_member("group_1", "user_4")
        .await
        .map_err(|e| format!("guest add_group_member succeeds: {e:?}"))?;
    backend
        .remove_group_member("group_1", "user_3")
        .await
        .map_err(|e| format!("guest remove_group_member succeeds: {e:?}"))?;
    Ok(())
}
