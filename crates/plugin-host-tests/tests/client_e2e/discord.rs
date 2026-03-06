//! End-to-end tests for the **Discord** client plugin (stub).
//!
//! Enable with: `--features test-discord`

use poly_client::{BackendType, ClientBackend};

use super::harness;

async fn load_discord() -> poly_plugin_host::PluginBackend {
    poly_plugin_loader_tests::load_plugin("discord", "poly_discord.wasm")
        .await
        .unwrap()
}

#[tokio::test]
async fn discord_backend_type() {
    let backend = load_discord().await;
    harness::assert_backend_type(&backend, BackendType::Discord);
}

#[tokio::test]
async fn discord_backend_name() {
    let backend = load_discord().await;
    harness::assert_backend_name(&backend, "Discord");
}

#[tokio::test]
async fn discord_authenticate_returns_error() {
    let mut backend = load_discord().await;
    harness::authenticate_returns_error(&mut backend).await;
}

#[tokio::test]
async fn discord_is_not_authenticated() {
    let backend = load_discord().await;
    assert!(!backend.is_authenticated());
}

#[tokio::test]
async fn discord_stub_returns_empty_lists() {
    let backend = load_discord().await;
    harness::assert_stub_returns_empty_lists(&backend).await;
}

#[tokio::test]
async fn discord_get_server_not_found() {
    let backend = load_discord().await;
    harness::get_server_not_found(&backend).await;
}

#[tokio::test]
async fn discord_get_channel_not_found() {
    let backend = load_discord().await;
    harness::get_channel_not_found(&backend).await;
}

#[tokio::test]
async fn discord_set_presence_ok() {
    let backend = load_discord().await;
    harness::set_presence(&backend, poly_client::PresenceStatus::Online).await;
}

#[tokio::test]
async fn discord_event_stream() {
    let backend = load_discord().await;
    harness::event_stream_is_valid(&backend);
}

#[tokio::test]
async fn discord_logout() {
    let mut backend = load_discord().await;
    harness::logout_succeeds(&mut backend).await;
}
