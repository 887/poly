#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code
)]
//! Library interface for the mock Discord REST API v10 server.
//!
//! Exposes `router` and `DiscordState` so integration tests can start the
//! server in-process without a subprocess.

pub mod routes;
pub mod state;

pub use state::DiscordState;

use axum::routing::{get, post};
use axum::Router;
use poly_test_common::health_handler;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

pub fn router(state: Arc<DiscordState>) -> Router {
    Router::new()
        .route("/health", get(|| async { health_handler("discord").await }))
        // Auth — Spacebar-compatible password login + Gateway discovery
        .route("/api/v10/auth/login", post(routes::login))
        .route("/api/v10/gateway", get(routes::get_gateway))
        // Test-only easy-signin
        .route("/test/auth/token", post(routes::test_auth_token))
        // Users
        .route("/api/v10/users/@me", get(routes::get_me))
        .route("/api/v10/users/@me/guilds", get(routes::get_my_guilds))
        .route("/api/v10/users/@me/channels", get(routes::get_dms).post(routes::open_dm))
        .route("/api/v10/users/{user_id}", get(routes::get_user))
        // Guilds
        .route("/api/v10/guilds/{guild_id}", get(routes::get_guild))
        .route("/api/v10/guilds/{guild_id}/channels", get(routes::get_guild_channels))
        // Channels
        .route("/api/v10/channels/{channel_id}", get(routes::get_channel))
        .route("/api/v10/channels/{channel_id}/messages", get(routes::get_messages).post(routes::send_message))
        // Threads
        .route("/api/v10/guilds/{guild_id}/threads/active", get(routes::get_guild_active_threads))
        .route("/api/v10/channels/{channel_id}/threads/archived/public", get(routes::get_channel_archived_threads))
        // Lifecycle
        .route("/seed", post(routes::seed))
        .route("/reset", post(routes::reset))
        .route("/reseed", post(routes::reseed))
        .with_state(state)
        .layer(CorsLayer::very_permissive())
}
