//! poly-backup-server binary entry point.
//!
//! Starts the Axum backup sync server with:
//! - REST API for encrypted settings sync (`/api/...`)
//! - Swagger UI at `/swagger-ui`
//! - Admin HTML UI at `/` (protected by PoW + username/password)

use poly_backup_server::{AdminState, AppState, Config, create_app};
use std::sync::Arc;
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Logging ──────────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info,poly_backup_server=debug")
            }),
        )
        .init();

    // ── Config ───────────────────────────────────────────────────────────────
    let config = Config::from_env();

    if config.passphrase == "changeme" {
        tracing::warn!(
            "POLY_PASSPHRASE is set to the default 'changeme' — \
             set a strong passphrase before exposing this server!"
        );
    }
    if config.admin_password == "changeme" {
        tracing::warn!(
            "POLY_ADMIN_PASSWORD is set to the default 'changeme' — \
             change it before exposing the admin UI!"
        );
    }

    let bind_addr = config.bind;

    tracing::info!("Poly Backup Server starting on {bind_addr}");
    tracing::info!(
        "Config: max_accounts={}, pow_difficulty={}, token_expiry={}d, admin={}",
        if config.max_accounts == 0 {
            "unlimited".to_owned()
        } else {
            config.max_accounts.to_string()
        },
        config.pow_difficulty,
        config.token_expiry_days,
        config.admin_user,
    );
    tracing::info!("Admin UI:    http://{bind_addr}/");
    tracing::info!("Swagger UI:  http://{bind_addr}/swagger-ui");
    tracing::info!("Health:      http://{bind_addr}/api/health");

    // ── Database ─────────────────────────────────────────────────────────────
    // Ensure the data directory exists.
    if let Err(e) = tokio::fs::create_dir_all(&config.data_dir).await {
        tracing::error!(
            "Failed to create data dir {}: {}",
            config.data_dir.display(),
            e
        );
        return Err(e.into());
    }

    let db = poly_backup_server::db::init(&config).await?;

    // ── State ─────────────────────────────────────────────────────────────────
    let state = AppState {
        db,
        config: Arc::new(config),
        admin: AdminState::new(),
    };

    // ── Spawn stale-challenge cleanup every 5 minutes ─────────────────────────
    {
        let admin = Arc::clone(&state.admin);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
            loop {
                interval.tick().await;
                let before = admin.challenges.len();
                admin
                    .challenges
                    .retain(|_, (_, exp)| exp.elapsed().as_secs() == 0);
                admin.sessions.retain(|_, exp| exp.elapsed().as_secs() == 0);
                let after = admin.challenges.len();
                if before != after {
                    tracing::debug!("Pruned {} stale admin challenges", before - after);
                }
            }
        });
    }

    // ── HTTP server ──────────────────────────────────────────────────────────
    let app = create_app(state);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Backup server shut down");
    Ok(())
}

/// Wait for Ctrl-C or SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = signal::ctrl_c().await {
            tracing::error!("Failed to install Ctrl+C handler: {e}");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                tracing::error!("Failed to install SIGTERM handler: {e}");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => tracing::info!("Ctrl-C received — shutting down"),
        () = terminate => tracing::info!("SIGTERM received — shutting down"),
    }
}
