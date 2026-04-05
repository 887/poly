//! Library interface for the mock Microsoft Teams/Graph API server.
//!
//! Exposes `router` and `TeamsState` for in-process integration tests.

#![allow(dead_code)]

pub mod routes;
pub mod state;

pub use state::TeamsState;

use axum::routing::{get, post};
use axum::Router;
use poly_test_common::health_handler;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

pub fn router(state: Arc<TeamsState>) -> Router {
    Router::new()
        .route("/health", get(|| async { health_handler("teams").await }))
        // Test-only easy-signin
        .route("/test/auth/token", post(routes::test_auth_token))
        // Current user
        .route("/v1.0/me", get(routes::get_me))
        // Teams
        .route("/v1.0/me/joinedTeams", get(routes::get_joined_teams))
        .route("/v1.0/teams/{team_id}", get(routes::get_team))
        .route("/v1.0/teams/{team_id}/channels", get(routes::get_channels))
        .route(
            "/v1.0/teams/{team_id}/channels/{channel_id}/messages",
            get(routes::get_channel_messages).post(routes::send_channel_message),
        )
        // Chats / DMs
        .route("/v1.0/me/chats", get(routes::get_chats))
        // Lifecycle
        .route("/seed", post(routes::seed))
        .route("/reset", post(routes::reset))
        .route("/reseed", post(routes::reseed))
        .with_state(state)
        .layer(CorsLayer::very_permissive())
}
