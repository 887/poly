//! Library interface for the mock Microsoft Teams/Graph API server.
//!
//! Exposes `router` and `TeamsState` for in-process integration tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code,
    unused_imports
)]

pub mod routes;
pub mod state;

pub use state::TeamsState;

use axum::routing::{delete, get, patch, post};
use axum::Router;
use poly_test_common::health_handler;
use std::sync::Arc;

/// Backend-specific routes only (no lifecycle, no inspect middleware, no CORS).
///
/// Called by `BackendHarness::routes()`.
pub fn routes_only(_state: Arc<TeamsState>) -> Router<Arc<TeamsState>> {
    Router::new()
        .route("/health", get(|| async { health_handler("teams").await }))
        // Test-only easy-signin
        .route("/test/auth/token", post(routes::test_auth_token))
        .route("/test/auth/login", post(routes::login))
        // Current user + user lookup
        .route("/v1.0/me", get(routes::get_me))
        .route("/v1.0/users/{user_id}", get(routes::get_user))
        // Teams
        .route("/v1.0/me/joinedTeams", get(routes::get_joined_teams))
        .route("/v1.0/teams/{team_id}", get(routes::get_team))
        .route("/v1.0/teams/{team_id}/channels", get(routes::get_channels))
        .route(
            "/v1.0/teams/{team_id}/channels/{channel_id}",
            get(routes::get_channel).patch(routes::patch_channel),
        )
        .route(
            "/v1.0/teams/{team_id}/channels/{channel_id}/messages",
            get(routes::get_channel_messages).post(routes::send_channel_message),
        )
        .route(
            "/v1.0/teams/{team_id}/channels/{channel_id}/messages/{message_id}",
            patch(routes::edit_channel_message).delete(routes::delete_channel_message),
        )
        .route(
            "/v1.0/teams/{team_id}/channels/{channel_id}/messages/{message_id}/setReaction",
            post(routes::set_reaction),
        )
        .route(
            "/v1.0/teams/{team_id}/channels/{channel_id}/messages/{message_id}/unsetReaction",
            post(routes::unset_reaction),
        )
        .route(
            "/v1.0/teams/{team_id}/channels/{channel_id}/messages/{message_id}/softDelete",
            post(routes::soft_delete_channel_message),
        )
        .route("/v1.0/me/presence/setPresence", patch(routes::set_presence))
        .route("/test/events/poll", get(routes::long_poll_events))
        // Team members (moderation)
        .route(
            "/v1.0/teams/{team_id}/members",
            get(routes::get_team_members),
        )
        .route(
            "/v1.0/teams/{team_id}/members/{membership_id}",
            delete(routes::delete_team_member),
        )
        // Profile photos (Graph profile-photo path used by poly-teams avatar fetch)
        .route(
            "/v1.0/users/{user_id}/photo/$value",
            get(routes::serve_user_photo),
        )
        // Chats / DMs
        .route("/v1.0/me/chats", get(routes::get_chats))
        .route(
            "/v1.0/chats/{chat_id}/messages",
            get(routes::get_chat_messages).post(routes::send_chat_message),
        )
        // NOTE: no .with_state() here — build_router() provides it via the outer chain
}

/// Full router (backend routes + lifecycle + inspect + CORS).
///
/// Kept for integration tests that call `router(state)` directly.
pub fn router(state: Arc<TeamsState>) -> Router {
    poly_test_common::build_router::<TeamsState>(state)
}
