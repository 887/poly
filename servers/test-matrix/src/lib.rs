#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code
)]
//! Library interface for the mock Matrix homeserver.
//!
//! Exposes `router` and `MatrixState` so integration tests can spin up the
//! server in-process without spawning a subprocess.

pub mod routes;
pub mod state;

use axum::Router;
use axum::routing::{get, post, put};
use poly_test_common::health_handler;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

pub use state::MatrixState;

/// Build the Matrix mock server router wired to the given state.
pub fn router(state: Arc<MatrixState>) -> Router {
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
        // Test helpers
        .route("/test/auth/token", post(routes::test_auth_token))
        .with_state(state)
        .layer(CorsLayer::very_permissive())
}
