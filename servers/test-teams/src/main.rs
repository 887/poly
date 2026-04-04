//! Mock Microsoft Teams/Graph API server for Poly testing.
//!
//! Implements the subset of the Microsoft Graph API that poly-teams calls.
//! In-memory state, mock OAuth2 token endpoint.

use axum::routing::get;
use axum::Router;
use poly_test_common::{health_handler, CliArgs, TestServerBase};

mod state;

use state::TeamsState;

fn router(state: TeamsState) -> Router {
    Router::new()
        .route(
            "/health",
            get(|| async { health_handler("teams").await }),
        )
        // TODO(4.6): Teams/Graph API endpoints
        // POST /oauth2/token (mock OAuth)
        // GET /v1.0/me
        // GET /v1.0/me/joinedTeams
        // GET /v1.0/teams/{id}
        // GET /v1.0/teams/{id}/channels
        // GET /v1.0/teams/{id}/channels/{id}/messages
        // POST /v1.0/teams/{id}/channels/{id}/messages
        // GET /v1.0/me/chats
        // GET /v1.0/me/presence
        // GET /v1.0/users/{id}/photo/$value
        // /reset (POST)
        // /seed (POST)
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let state = TeamsState::new();
    if args.seed {
        state.seed();
    }

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-teams listening on {}", base.base_url());

    let app = router(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            let _ = base.shutdown_rx.await;
        })
        .await?;

    Ok(())
}
