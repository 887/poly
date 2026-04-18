//! End-to-end tests for the **Teams** client plugin (stub).
//!
//! Enable with: `--features test-teams`

use poly_client::{BackendType, ClientBackend};

use super::harness;

type TestResult = Result<(), Box<dyn std::error::Error>>;

async fn load_teams() -> Result<poly_plugin_host::PluginBackend, Box<dyn std::error::Error>> {
    poly_plugin_loader_tests::load_plugin("teams", "poly_teams.wasm").await
}

#[tokio::test]
async fn teams_backend_type() -> TestResult {
    let backend = load_teams().await?;
    harness::assert_backend_type(&backend, BackendType::from("teams"));
    Ok(())
}

#[tokio::test]
async fn teams_backend_name() -> TestResult {
    let backend = load_teams().await?;
    harness::assert_backend_name(&backend, "Teams");
    Ok(())
}

#[tokio::test]
async fn teams_authenticate_returns_error() -> TestResult {
    let mut backend = load_teams().await?;
    harness::authenticate_returns_error(&mut backend).await;
    Ok(())
}

#[tokio::test]
async fn teams_is_not_authenticated() -> TestResult {
    let backend = load_teams().await?;
    assert!(!backend.is_authenticated());
    Ok(())
}

#[tokio::test]
async fn teams_stub_returns_empty_lists() -> TestResult {
    let backend = load_teams().await?;
    harness::assert_stub_returns_empty_lists(&backend).await
}

#[tokio::test]
async fn teams_get_server_not_found() -> TestResult {
    let backend = load_teams().await?;
    harness::get_server_not_found(&backend).await;
    Ok(())
}

#[tokio::test]
async fn teams_get_channel_not_found() -> TestResult {
    let backend = load_teams().await?;
    harness::get_channel_not_found(&backend).await;
    Ok(())
}

#[tokio::test]
async fn teams_set_presence_ok() -> TestResult {
    let backend = load_teams().await?;
    harness::set_presence(&backend, poly_client::PresenceStatus::Online).await
}

#[tokio::test]
async fn teams_event_stream() -> TestResult {
    let backend = load_teams().await?;
    harness::event_stream_is_valid(&backend);
    Ok(())
}

#[tokio::test]
async fn teams_logout() -> TestResult {
    let mut backend = load_teams().await?;
    harness::logout_succeeds(&mut backend).await;
    Ok(())
}
