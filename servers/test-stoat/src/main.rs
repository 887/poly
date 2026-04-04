//! Mock Stoat/Revolt API server for Poly testing.
//!
//! Implements the subset of the Revolt REST API + WebSocket (Bonfire) that
//! poly-stoat calls. In-memory state, no real federation.

use axum::routing::get;
use axum::Router;
use poly_test_common::{health_handler, CliArgs, TestServerBase};

mod state;

use state::StoatState;

fn router(state: StoatState) -> Router {
    Router::new()
        .route(
            "/health",
            get(|| async { health_handler("stoat").await }),
        )
        // TODO(4.4): Stoat/Revolt API endpoints
        // POST /auth/session/login
        // DELETE /auth/session/logout
        // GET /users/@me
        // GET /users/{id}
        // GET /servers/{id}
        // GET /servers/{id}/channels
        // GET /channels/{id}
        // GET /channels/{id}/messages
        // POST /channels/{id}/messages
        // GET /sync/unreads
        // WS /ws (Bonfire)
        // /reset (POST)
        // /seed (POST)
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let state = StoatState::new();
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
