//! Mock Matrix homeserver for Poly testing.
//!
//! Implements the subset of the Matrix Client-Server API that poly-matrix calls:
//! login, sync, rooms, messages, members, profile, register.
//! In-memory state, no federation, no E2EE.

use axum::routing::get;
use axum::Router;
use poly_test_common::{health_handler, CliArgs, TestServerBase};

mod state;

use state::MatrixState;

fn router(state: MatrixState) -> Router {
    Router::new()
        .route(
            "/health",
            get(|| async { health_handler("matrix").await }),
        )
        // TODO(4.3): Matrix CS API endpoints
        // /_matrix/client/v3/login (POST)
        // /_matrix/client/v3/logout (POST)
        // /_matrix/client/v3/account/whoami (GET)
        // /_matrix/client/v3/register (POST)
        // /_matrix/client/v3/profile/{userId} (GET)
        // /_matrix/client/v3/joined_rooms (GET)
        // /_matrix/client/v3/sync (GET)
        // /_matrix/client/v3/rooms/{roomId}/messages (GET)
        // /_matrix/client/v3/rooms/{roomId}/send/{eventType}/{txnId} (PUT)
        // /_matrix/client/v3/rooms/{roomId}/members (GET)
        // /_matrix/client/v3/rooms/{roomId}/state (GET)
        // /_matrix/client/v1/rooms/{roomId}/hierarchy (GET)
        // /_matrix/client/v3/join/{roomIdOrAlias} (POST)
        // /_matrix/client/v3/publicRooms (GET)
        // /_matrix/client/v3/user/{userId}/account_data/{type} (GET)
        // /seed (POST) — populate demo data (idempotent)
        // /reset (POST) — wipe to empty state
        // /reseed (POST) — wipe + re-seed in one call
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let state = MatrixState::new();
    if args.seed {
        state.seed();
    }

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-matrix listening on {}", base.base_url());

    let app = router(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            let _ = base.shutdown_rx.await;
        })
        .await?;

    Ok(())
}
