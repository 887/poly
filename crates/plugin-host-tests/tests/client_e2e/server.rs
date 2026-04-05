//! End-to-end tests for the **Poly Server** client plugin (stub).
//!
//! Enable with: `--features test-server`

use poly_client::{BackendType, ClientBackend};

use super::harness;

async fn load_server() -> poly_plugin_host::PluginBackend {
    poly_plugin_loader_tests::load_plugin("server", "poly_server_client.wasm")
        .await
        .unwrap()
}

#[tokio::test]
async fn server_backend_type() {
    let backend = load_server().await;
    harness::assert_backend_type(&backend, BackendType::from("poly"));
}

#[tokio::test]
async fn server_backend_name() {
    let backend = load_server().await;
    harness::assert_backend_name(&backend, "Poly Server");
}

#[tokio::test]
async fn server_authenticate_returns_error() {
    let mut backend = load_server().await;
    harness::authenticate_returns_error(&mut backend).await;
}

#[tokio::test]
async fn server_is_not_authenticated() {
    let backend = load_server().await;
    assert!(!backend.is_authenticated());
}

#[tokio::test]
async fn server_stub_returns_empty_lists() {
    let backend = load_server().await;
    harness::assert_stub_returns_empty_lists(&backend).await;
}

#[tokio::test]
async fn server_get_server_not_found() {
    let backend = load_server().await;
    harness::get_server_not_found(&backend).await;
}

#[tokio::test]
async fn server_get_channel_not_found() {
    let backend = load_server().await;
    harness::get_channel_not_found(&backend).await;
}

#[tokio::test]
async fn server_set_presence_ok() {
    let backend = load_server().await;
    harness::set_presence(&backend, poly_client::PresenceStatus::Online).await;
}

#[tokio::test]
async fn server_event_stream() {
    let backend = load_server().await;
    harness::event_stream_is_valid(&backend);
}

#[tokio::test]
async fn server_logout() {
    let mut backend = load_server().await;
    harness::logout_succeeds(&mut backend).await;
}
