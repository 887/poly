//! poly-backup-server binary entry point.
//!
//! Starts the Axum backup server with REST API and admin web UI.

use poly_backup_server::{AppState, ServerConfig, create_router};
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info,poly_backup_server=debug")
            }),
        )
        .init();

    let config = ServerConfig::from_env();
    let bind_addr = config.bind_addr.clone();

    tracing::info!("Starting Poly Backup Server on {bind_addr}");
    tracing::info!(
        "Config: max_accounts={}, pow_difficulty={}, token_expiry={}d",
        config.max_accounts,
        config.pow_difficulty,
        config.token_expiry_days
    );

    let state = Arc::new(RwLock::new(AppState { config }));

    let app = create_router(state.clone()).merge(poly_backup_server::web::admin_routes());

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("Backup server listening on {bind_addr}");

    axum::serve(listener, app).await?;
    Ok(())
}
