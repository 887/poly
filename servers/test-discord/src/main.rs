//! Mock Discord API server for Poly testing.
//!
//! Implements the subset of the Discord REST API + Gateway WebSocket that
//! poly-discord calls. In-memory state, no real bot gateway.

use axum::routing::{get, post};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use poly_test_common::{health_handler, CliArgs, TestServerBase};
use std::sync::Arc;

mod state;

use state::DiscordState;

async fn test_auth_token(
    State(state): State<Arc<DiscordState>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let username = body
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("discord_test");
    let token = state.auth.create_token(username);
    Json(serde_json::json!({
        "result": "Success",
        "token": token,
        "user_id": username,
    }))
}

fn router(state: Arc<DiscordState>) -> axum::Router {
    axum::Router::new()
        .route(
            "/health",
            get(|| async { health_handler("discord").await }),
        )
        // Test-only easy-signin
        .route("/test/auth/token", post(test_auth_token))
        // TODO(4.5): Discord API endpoints
        // GET /api/v10/users/@me
        // GET /api/v10/users/@me/guilds
        // GET /api/v10/guilds/{id}
        // GET /api/v10/guilds/{id}/channels
        // GET /api/v10/channels/{id}
        // GET /api/v10/channels/{id}/messages
        // POST /api/v10/channels/{id}/messages
        // GET /api/v10/users/@me/channels (DMs)
        // POST /api/v10/users/@me/channels (open DM)
        // WS /gateway (Gateway v10)
        // /seed (POST) — populate demo data (idempotent)
        // /reset (POST) — wipe to empty state
        // /reseed (POST) — wipe + re-seed in one call
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let state = Arc::new(DiscordState::new());
    if args.seed {
        state.seed();
    }

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-discord listening on {}", base.base_url());

    let app = router(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            let _ = base.shutdown_rx.await;
        })
        .await?;

    Ok(())
}
