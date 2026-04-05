//! Mock Stoat/Revolt API server for Poly testing.
//!
//! Implements the subset of the Revolt REST API + WebSocket (Bonfire) that
//! poly-stoat calls. In-memory state, no real federation.

use axum::routing::{delete, get, post};
use axum::Router;
use poly_test_common::{health_handler, CliArgs, TestServerBase};

mod routes;
mod state;

use std::sync::Arc;
use state::StoatState;

fn router(state: Arc<StoatState>) -> Router {
    Router::new()
        .route(
            "/health",
            get(|| async { health_handler("stoat").await }),
        )
        // Server config
        .route("/", get(routes::server_config))
        // Auth
        .route("/auth/session/login", post(routes::login))
        .route("/auth/session/logout", delete(routes::logout))
        // Users
        .route("/users/@me", get(routes::get_me))
        .route("/users/dms", get(routes::get_dms))
        .route("/users/{id}", get(routes::get_user))
        .route("/users/{id}/dm", get(routes::get_user_dm))
        // Servers
        .route("/servers/{id}", get(routes::get_server))
        .route("/servers/{id}/members", get(routes::get_server_members))
        // Channels
        .route("/channels/{id}", get(routes::get_channel))
        .route("/channels/{id}/members", get(routes::get_channel_members))
        .route("/channels/{id}/messages", get(routes::get_messages).post(routes::send_message))
        // Sync
        .route("/sync/unreads", get(routes::sync_unreads))
        // TODO(4.4): WS /ws (Bonfire) — WebSocket endpoint for real-time events
        // Lifecycle
        .route("/seed", post(routes::seed))
        .route("/reset", post(routes::reset))
        .route("/reseed", post(routes::reseed))
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let state = Arc::new(StoatState::new());
    if args.seed {
        state.seed();
    }

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-stoat listening on {}", base.base_url());

    let app = router(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            let _ = base.shutdown_rx.await;
        })
        .await?;

    Ok(())
}
