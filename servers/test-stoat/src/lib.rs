#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code,
    unused_variables
)]
//! Library interface for the mock Stoat/Revolt API server.
//!
//! Exposes `router` and `StoatState` so integration tests can spin up the
//! server in-process without spawning a subprocess.

pub mod routes;
pub mod state;

use axum::middleware;
use axum::Router;
use axum::routing::{delete, get, post, put};
use poly_test_common::{handle_inspect_last_headers, header_inspect_middleware, health_handler};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

pub use state::StoatState;

/// Build the Stoat mock server router wired to the given state.
pub fn router(state: Arc<StoatState>) -> Router {
    let inspect = Arc::clone(&state.inspect);
    Router::new()
        .route(
            "/health",
            get(|| async { health_handler("stoat").await }),
        )
        // Server config
        .route("/", get(routes::server_config))
        // Auth
        .route("/auth/session/login", post(routes::login))
        .route("/auth/session/logout", post(routes::logout).delete(routes::logout))
        // Users
        .route("/users/@me", get(routes::get_me))
        .route("/users/@me/servers", get(routes::get_my_servers))
        .route("/users/dms", get(routes::get_dms))
        .route("/users/{id}", get(routes::get_user))
        .route("/users/{id}/dm", get(routes::get_user_dm))
        // Servers
        .route("/servers/{id}", get(routes::get_server))
        .route("/servers/{id}/members", get(routes::get_server_members))
        // Moderation — member management
        .route("/servers/{server_id}/members/@me", get(routes::get_my_member))
        .route("/servers/{server_id}/members/{member_id}", delete(routes::kick_member).patch(routes::edit_member))
        // Moderation — bans
        .route("/servers/{server_id}/bans", get(routes::list_bans))
        .route("/servers/{server_id}/bans/{user_id}", put(routes::ban_member).delete(routes::unban_member))
        // Channels
        .route("/channels/{id}", get(routes::get_channel).patch(routes::update_channel))
        .route("/channels/{id}/members", get(routes::get_channel_members))
        .route("/channels/{id}/messages", get(routes::get_messages).post(routes::send_message))
        .route("/channels/{id}/messages/{message_id}", get(routes::get_message).delete(routes::delete_message))
        .route("/channels/{id}/typing", post(routes::channel_start_typing))
        // Bonfire WebSocket
        .route("/bonfire", get(routes::bonfire_ws))
        // Sync
        .route("/sync/unreads", get(routes::sync_unreads))
        // Autumn file serving (avatars, etc.)
        .route("/avatars/{id}", get(routes::serve_avatar))
        // Test-only easy-signin
        .route("/test/auth/token", post(routes::test_auth_token))
        // Lifecycle
        .route("/seed", post(routes::seed))
        .route("/reset", post(routes::reset))
        .route("/reseed", post(routes::reseed))
        // Inspection endpoints (Phase E)
        .route(
            "/test/inspect/last-headers",
            get(handle_inspect_last_headers).with_state(Arc::clone(&inspect)),
        )
        .with_state(state)
        .layer(middleware::from_fn_with_state(
            Arc::clone(&inspect),
            header_inspect_middleware,
        ))
        .layer(CorsLayer::very_permissive())
}
