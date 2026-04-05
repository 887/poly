//! End-to-end tests for the **Matrix** client plugin (stub).
//!
//! Enable with: `--features test-matrix`

use poly_client::{BackendType, ClientBackend};

use super::harness;

async fn load_matrix() -> poly_plugin_host::PluginBackend {
    poly_plugin_loader_tests::load_plugin("matrix", "poly_matrix.wasm")
        .await
        .unwrap()
}

#[tokio::test]
async fn matrix_backend_type() {
    let backend = load_matrix().await;
    harness::assert_backend_type(&backend, BackendType::from("matrix"));
}

#[tokio::test]
async fn matrix_backend_name() {
    let backend = load_matrix().await;
    harness::assert_backend_name(&backend, "Matrix");
}

#[tokio::test]
async fn matrix_authenticate_returns_error() {
    let mut backend = load_matrix().await;
    harness::authenticate_returns_error(&mut backend).await;
}

#[tokio::test]
async fn matrix_is_not_authenticated() {
    let backend = load_matrix().await;
    assert!(!backend.is_authenticated());
}

#[tokio::test]
async fn matrix_stub_returns_empty_lists() {
    let backend = load_matrix().await;
    harness::assert_stub_returns_empty_lists(&backend).await;
}

#[tokio::test]
async fn matrix_get_server_not_found() {
    let backend = load_matrix().await;
    harness::get_server_not_found(&backend).await;
}

#[tokio::test]
async fn matrix_get_channel_not_found() {
    let backend = load_matrix().await;
    harness::get_channel_not_found(&backend).await;
}

#[tokio::test]
async fn matrix_set_presence_ok() {
    let backend = load_matrix().await;
    harness::set_presence(&backend, poly_client::PresenceStatus::Online).await;
}

#[tokio::test]
async fn matrix_event_stream() {
    let backend = load_matrix().await;
    harness::event_stream_is_valid(&backend);
}

#[tokio::test]
async fn matrix_logout() {
    let mut backend = load_matrix().await;
    harness::logout_succeeds(&mut backend).await;
}
