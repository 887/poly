//! Mock Microsoft Teams/Graph API server for Poly testing.
//!
//! Implements the subset of the Microsoft Graph API that poly-teams calls.
//! In-memory state, mock OAuth2 token endpoint.

use axum::routing::{get, post};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use poly_test_common::{health_handler, CliArgs, TestServerBase};
use std::sync::Arc;

mod state;

use state::TeamsState;

async fn test_auth_token(
    State(state): State<Arc<TeamsState>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let username = body
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("teams_test");
    let token = state.auth.create_token(username);
    Json(serde_json::json!({
        "result": "Success",
        "token": token,
        "user_id": username,
    }))
}

fn router(state: Arc<TeamsState>) -> axum::Router {
    axum::Router::new()
        .route(
            "/health",
            get(|| async { health_handler("teams").await }),
        )
        // Test-only easy-signin
        .route("/test/auth/token", post(test_auth_token))
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
        // /seed (POST) — populate demo data (idempotent)
        // /reset (POST) — wipe to empty state
        // /reseed (POST) — wipe + re-seed in one call
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let state = Arc::new(TeamsState::new());
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
