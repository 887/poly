#![allow(
    clippy::significant_drop_tightening,
    clippy::option_if_let_else,
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::needless_pass_by_value,
    clippy::map_unwrap_or,
    clippy::manual_let_else,
    clippy::single_match_else,
    clippy::use_self,
    clippy::match_same_arms,
    clippy::redundant_clone,
    clippy::struct_excessive_bools,
    clippy::wildcard_enum_match_arm,
    clippy::implicit_hasher,
    clippy::match_like_matches_macro,
    clippy::or_fun_call,
    clippy::explicit_iter_loop,
    clippy::assigning_clones,
    clippy::significant_drop_in_scrutinee,
    clippy::uninlined_format_args,
    clippy::collapsible_if,
    clippy::suspicious_operation_groupings,
    clippy::redundant_closure_for_method_calls,
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

use axum::Router;
use axum::routing::{delete, get, post, put};
use poly_test_common::health_handler;
use std::sync::Arc;

pub use state::StoatState;

/// Backend-specific routes only (no lifecycle, no inspect middleware, no CORS).
///
/// Called by `BackendHarness::routes()`.
pub fn routes_only(state: Arc<StoatState>) -> Router<Arc<StoatState>> {
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
        .route("/channels/create", post(routes::create_channel))
        .route("/channels/{id}", get(routes::get_channel).patch(routes::update_channel).delete(routes::delete_channel))
        .route("/channels/{id}/members", get(routes::get_channel_members))
        .route("/channels/{id}/messages", get(routes::get_messages).post(routes::send_message))
        .route("/channels/{id}/messages/{message_id}", get(routes::get_message).delete(routes::delete_message))
        .route("/channels/{id}/typing", post(routes::channel_start_typing))
        // Phase F — voice (Vortex mock)
        .route("/channels/{id}/join_call", post(routes::join_call))
        .route("/channels/{id}/voice_state", axum::routing::patch(routes::patch_voice_state))
        .route("/vortex/ws", get(routes::vortex_ws))
        // Bonfire WebSocket
        .route("/bonfire", get(routes::bonfire_ws))
        // Sync
        .route("/sync/unreads", get(routes::sync_unreads))
        // Autumn file serving (avatars, etc.)
        .route("/avatars/{id}", get(routes::serve_avatar))
        // Test-only easy-signin
        .route("/test/auth/token", post(routes::test_auth_token))
        // NOTE: no .with_state() here — build_router() provides it via the outer chain
}

/// Full router (backend routes + lifecycle + inspect + CORS).
///
/// Kept for integration tests that call `router(state)` directly.
pub fn router(state: Arc<StoatState>) -> Router {
    poly_test_common::build_router::<StoatState>(state)
}
