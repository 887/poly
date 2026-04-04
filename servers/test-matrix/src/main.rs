//! Mock Matrix homeserver for Poly testing.
//!
//! Implements the subset of the Matrix Client-Server API that poly-matrix calls:
//! login, sync, rooms, messages, members, profile, spaces, account data.
//! In-memory state, no federation, no E2EE.

use axum::routing::{get, post, put};
use axum::Router;
use poly_test_common::{health_handler, CliArgs, TestServerBase};

mod routes;
mod state;

use state::MatrixState;

fn router(state: MatrixState) -> Router {
    Router::new()
        .route(
            "/health",
            get(|| async { health_handler("matrix").await }),
        )
        // Auth
        .route("/_matrix/client/v3/login", post(routes::login))
        .route("/_matrix/client/v3/logout", post(routes::logout))
        .route("/_matrix/client/v3/account/whoami", get(routes::whoami))
        // Profile
        .route("/_matrix/client/v3/profile/{userId}", get(routes::get_profile))
        // Rooms
        .route("/_matrix/client/v3/joined_rooms", get(routes::joined_rooms))
        .route("/_matrix/client/v3/rooms/{roomId}/state", get(routes::room_state))
        .route("/_matrix/client/v3/rooms/{roomId}/members", get(routes::room_members))
        .route("/_matrix/client/v3/rooms/{roomId}/messages", get(routes::get_messages))
        .route("/_matrix/client/v3/rooms/{roomId}/send/{eventType}/{txnId}", put(routes::send_message))
        // Sync
        .route("/_matrix/client/v3/sync", get(routes::sync))
        // Spaces
        .route("/_matrix/client/v1/rooms/{roomId}/hierarchy", get(routes::space_hierarchy))
        // Directory
        .route("/_matrix/client/v3/publicRooms", get(routes::public_rooms))
        .route("/_matrix/client/v3/join/{roomIdOrAlias}", post(routes::join_room))
        // Account data
        .route("/_matrix/client/v3/user/{userId}/account_data/{dataType}", get(routes::get_account_data))
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
