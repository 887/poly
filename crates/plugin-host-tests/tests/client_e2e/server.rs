//! End-to-end tests for the **Poly Server** client plugin (stub).
//!
//! Enable with: `--features test-server`

use poly_client::{BackendType, ClientBackend};

use super::harness;

type TestResult = Result<(), Box<dyn std::error::Error>>;

async fn load_server() -> Result<poly_plugin_host::PluginBackend, Box<dyn std::error::Error>> {
    poly_plugin_loader_tests::load_plugin("server", "poly_server_client.wasm").await
}

#[tokio::test]
async fn server_backend_type() -> TestResult {
    let backend = load_server().await?;
    harness::assert_backend_type(&backend, BackendType::from("poly"));
    Ok(())
}

#[tokio::test]
async fn server_backend_name() -> TestResult {
    let backend = load_server().await?;
    harness::assert_backend_name(&backend, "Poly Server");
    Ok(())
}

#[tokio::test]
async fn server_authenticate_returns_error() -> TestResult {
    let mut backend = load_server().await?;
    harness::authenticate_returns_error(&mut backend).await;
    Ok(())
}

#[tokio::test]
async fn server_is_not_authenticated() -> TestResult {
    let backend = load_server().await?;
    assert!(!backend.is_authenticated());
    Ok(())
}

#[tokio::test]
async fn server_stub_returns_empty_lists() -> TestResult {
    let backend = load_server().await?;
    harness::assert_stub_returns_empty_lists(&backend).await
}

#[tokio::test]
async fn server_get_server_not_found() -> TestResult {
    let backend = load_server().await?;
    harness::get_server_not_found(&backend).await;
    Ok(())
}

#[tokio::test]
async fn server_get_channel_not_found() -> TestResult {
    let backend = load_server().await?;
    harness::get_channel_not_found(&backend).await;
    Ok(())
}

#[tokio::test]
async fn server_set_presence_ok() -> TestResult {
    let backend = load_server().await?;
    harness::set_presence(&backend, poly_client::PresenceStatus::Online).await
}

#[tokio::test]
async fn server_event_stream() -> TestResult {
    let backend = load_server().await?;
    harness::event_stream_is_valid(&backend);
    Ok(())
}

#[tokio::test]
async fn server_logout() -> TestResult {
    let mut backend = load_server().await?;
    harness::logout_succeeds(&mut backend).await;
    Ok(())
}
