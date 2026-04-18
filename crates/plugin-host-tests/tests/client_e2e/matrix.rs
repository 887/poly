//! End-to-end tests for the **Matrix** client plugin (stub).
//!
//! Enable with: `--features test-matrix`

use poly_client::{BackendType, ClientBackend};

use super::harness;

type TestResult = Result<(), Box<dyn std::error::Error>>;

async fn load_matrix() -> Result<poly_plugin_host::PluginBackend, Box<dyn std::error::Error>> {
    poly_plugin_loader_tests::load_plugin("matrix", "poly_matrix.wasm").await
}

#[tokio::test]
async fn matrix_backend_type() -> TestResult {
    let backend = load_matrix().await?;
    harness::assert_backend_type(&backend, BackendType::from("matrix"));
    Ok(())
}

#[tokio::test]
async fn matrix_backend_name() -> TestResult {
    let backend = load_matrix().await?;
    harness::assert_backend_name(&backend, "Matrix");
    Ok(())
}

#[tokio::test]
async fn matrix_authenticate_returns_error() -> TestResult {
    let mut backend = load_matrix().await?;
    harness::authenticate_returns_error(&mut backend).await;
    Ok(())
}

#[tokio::test]
async fn matrix_is_not_authenticated() -> TestResult {
    let backend = load_matrix().await?;
    assert!(!backend.is_authenticated());
    Ok(())
}

#[tokio::test]
async fn matrix_stub_returns_empty_lists() -> TestResult {
    let backend = load_matrix().await?;
    harness::assert_stub_returns_empty_lists(&backend).await
}

#[tokio::test]
async fn matrix_get_server_not_found() -> TestResult {
    let backend = load_matrix().await?;
    harness::get_server_not_found(&backend).await;
    Ok(())
}

#[tokio::test]
async fn matrix_get_channel_not_found() -> TestResult {
    let backend = load_matrix().await?;
    harness::get_channel_not_found(&backend).await;
    Ok(())
}

#[tokio::test]
async fn matrix_set_presence_ok() -> TestResult {
    let backend = load_matrix().await?;
    harness::set_presence(&backend, poly_client::PresenceStatus::Online).await
}

#[tokio::test]
async fn matrix_event_stream() -> TestResult {
    let backend = load_matrix().await?;
    harness::event_stream_is_valid(&backend);
    Ok(())
}

#[tokio::test]
async fn matrix_logout() -> TestResult {
    let mut backend = load_matrix().await?;
    harness::logout_succeeds(&mut backend).await;
    Ok(())
}
