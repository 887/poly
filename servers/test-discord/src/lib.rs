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

use axum::middleware;
use axum::routing::{delete, get, post, put};
use axum::Router;
use poly_test_common::{handle_inspect_last_headers, header_inspect_middleware, health_handler};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

pub fn router(state: Arc<DiscordState>) -> Router {
    let inspect = Arc::clone(&state.inspect);
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
        .route(
            "/api/v10/users/@me/relationships/{user_id}",
            put(routes::put_relationship).delete(routes::delete_relationship),
        )
        .route("/api/v10/users/@me/notes/{user_id}", put(routes::put_user_note))
        .route("/api/v10/users/{user_id}", get(routes::get_user))
        // Guilds
        .route("/api/v10/guilds/{guild_id}", get(routes::get_guild).patch(routes::patch_guild))
        .route(
            "/api/v10/guilds/{guild_id}/channels",
            get(routes::get_guild_channels).patch(routes::reorder_guild_channels),
        )
        // Moderation — guild member + roles
        .route(
            "/api/v10/guilds/{guild_id}/members/@me",
            get(routes::get_guild_member_me),
        )
        .route(
            "/api/v10/guilds/{guild_id}/members/{user_id}",
            delete(routes::kick_member).patch(routes::patch_guild_member),
        )
        .route("/api/v10/guilds/{guild_id}/roles", get(routes::get_guild_roles))
        // Moderation — bans
        .route(
            "/api/v10/guilds/{guild_id}/bans",
            get(routes::get_bans),
        )
        .route(
            "/api/v10/guilds/{guild_id}/bans/{user_id}",
            put(routes::ban_member).delete(routes::unban_member),
        )
        // Moderation — audit log
        .route("/api/v10/guilds/{guild_id}/audit-logs", get(routes::get_audit_log))
        // Channels
        .route(
            "/api/v10/channels/{channel_id}",
            get(routes::get_channel).patch(routes::patch_channel).delete(routes::delete_channel),
        )
        .route(
            "/api/v10/channels/{channel_id}/messages",
            get(routes::get_messages).post(routes::send_message),
        )
        // Moderation — delete message
        .route(
            "/api/v10/channels/{channel_id}/messages/{message_id}",
            delete(routes::delete_message),
        )
        // Group DM recipients + invites
        .route(
            "/api/v10/channels/{channel_id}/recipients/{user_id}",
            put(routes::put_group_dm_recipient),
        )
        .route(
            "/api/v10/channels/{channel_id}/invites",
            post(routes::create_invite),
        )
        // Threads
        .route("/api/v10/guilds/{guild_id}/threads/active", get(routes::get_guild_active_threads))
        .route("/api/v10/channels/{channel_id}/threads/archived/public", get(routes::get_channel_archived_threads))
        // CDN — serve bundled avatar bytes for fixture users
        .route("/avatars/{user_id}/{file}", get(routes::serve_avatar))
        // Gateway WebSocket (Phase 6.5)
        .route("/gateway/ws", get(routes::gateway_ws))
        // Test-only: inject gateway events
        .route("/testhook/emit_thread_event", post(routes::emit_thread_event))
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
