//! End-to-end tests for the **Stoat** client plugin (stub).
//!
//! The Stoat client is a stub — all methods return empty lists or errors.
//! These tests verify the stub conforms to the `ClientBackend` interface
//! contract and returns expected stub behavior.
//!
//! Enable with: `--features test-stoat`

use poly_client::{BackendType, ClientBackend};

use super::harness;

async fn load_stoat() -> poly_plugin_host::PluginBackend {
    poly_plugin_loader_tests::load_plugin("stoat", "poly_stoat.wasm")
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
async fn stoat_authenticate_returns_error() {
    let mut backend = load_stoat().await;
    harness::authenticate_returns_error(&mut backend).await;
}

#[tokio::test]
async fn stoat_authenticate_email_password_returns_error() {
    let mut backend = load_stoat().await;
    let result = backend
        .authenticate(poly_client::AuthCredentials::EmailPassword {
            email: "alice@example.test".to_string(),
            password: "secret".to_string(),
        })
        .await;
    assert!(
        result.is_err(),
        "Stub Stoat guest authenticate(email/password) should return an error"
    );
}

#[tokio::test]
async fn stoat_is_not_authenticated() {
    let backend = load_stoat().await;
    assert!(!backend.is_authenticated());
}

#[tokio::test]
async fn stoat_stub_returns_empty_lists() {
    let backend = load_stoat().await;
    harness::assert_stub_returns_empty_lists(&backend).await;
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
    let mut backend = load_stoat().await;
    harness::logout_succeeds(&mut backend).await;
}
