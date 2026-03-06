//! End-to-end tests for the **Teams** client plugin (stub).
//!
//! Enable with: `--features test-teams`

use poly_client::{BackendType, ClientBackend};

use super::harness;

async fn load_teams() -> poly_plugin_host::PluginBackend {
    poly_plugin_loader_tests::load_plugin("teams", "poly_teams.wasm")
        .await
        .unwrap()
}

#[tokio::test]
async fn teams_backend_type() {
    let backend = load_teams().await;
    harness::assert_backend_type(&backend, BackendType::Teams);
}

#[tokio::test]
async fn teams_backend_name() {
    let backend = load_teams().await;
    harness::assert_backend_name(&backend, "Teams");
}

#[tokio::test]
async fn teams_authenticate_returns_error() {
    let mut backend = load_teams().await;
    harness::authenticate_returns_error(&mut backend).await;
}

#[tokio::test]
async fn teams_is_not_authenticated() {
    let backend = load_teams().await;
    assert!(!backend.is_authenticated());
}

#[tokio::test]
async fn teams_stub_returns_empty_lists() {
    let backend = load_teams().await;
    harness::assert_stub_returns_empty_lists(&backend).await;
}

#[tokio::test]
async fn teams_get_server_not_found() {
    let backend = load_teams().await;
    harness::get_server_not_found(&backend).await;
}

#[tokio::test]
async fn teams_get_channel_not_found() {
    let backend = load_teams().await;
    harness::get_channel_not_found(&backend).await;
}

#[tokio::test]
async fn teams_set_presence_ok() {
    let backend = load_teams().await;
    harness::set_presence(&backend, poly_client::PresenceStatus::Online).await;
}

#[tokio::test]
async fn teams_event_stream() {
    let backend = load_teams().await;
    harness::event_stream_is_valid(&backend);
}

#[tokio::test]
async fn teams_logout() {
    let mut backend = load_teams().await;
    harness::logout_succeeds(&mut backend).await;
}
