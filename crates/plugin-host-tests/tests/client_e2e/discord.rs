//! End-to-end tests for the **Discord** client plugin (stub).
//!
//! Enable with: `--features test-discord`



use super::harness;

type TestResult = Result<(), Box<dyn std::error::Error>>;

async fn load_discord() -> Result<poly_plugin_host::PluginBackend, Box<dyn std::error::Error>> {
    poly_plugin_loader_tests::load_plugin("discord", "poly_discord.wasm").await
}

#[tokio::test]
async fn discord_backend_type() -> TestResult {
    let backend = load_discord().await?;
    harness::assert_backend_type(&backend, BackendType::from("discord"));
    Ok(())
}

#[tokio::test]
async fn discord_backend_name() -> TestResult {
    let backend = load_discord().await?;
    harness::assert_backend_name(&backend, "Discord");
    Ok(())
}

#[tokio::test]
async fn discord_authenticate_returns_error() -> TestResult {
    let mut backend = load_discord().await?;
    harness::authenticate_returns_error(&mut backend).await;
    Ok(())
}

#[tokio::test]
async fn discord_is_not_authenticated() -> TestResult {
    let backend = load_discord().await?;
    assert!(!backend.is_authenticated());
    Ok(())
}

#[tokio::test]
async fn discord_stub_returns_empty_lists() -> TestResult {
    let backend = load_discord().await?;
    harness::assert_stub_returns_empty_lists(&backend).await
}

#[tokio::test]
async fn discord_get_server_not_found() -> TestResult {
    let backend = load_discord().await?;
    harness::get_server_not_found(&backend).await;
    Ok(())
}

#[tokio::test]
async fn discord_get_channel_not_found() -> TestResult {
    let backend = load_discord().await?;
    harness::get_channel_not_found(&backend).await;
    Ok(())
}

#[tokio::test]
async fn discord_set_presence_ok() -> TestResult {
    let backend = load_discord().await?;
    harness::set_presence(&backend, poly_client::PresenceStatus::Online).await
}

#[tokio::test]
async fn discord_event_stream() -> TestResult {
    let backend = load_discord().await?;
    harness::event_stream_is_valid(&backend);
    Ok(())
}

#[tokio::test]
async fn discord_logout() -> TestResult {
    let mut backend = load_discord().await?;
    harness::logout_succeeds(&mut backend).await;
    Ok(())
}
