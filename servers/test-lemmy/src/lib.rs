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
    dead_code
)]
//! Mock Lemmy API server for Poly testing — library entry point.
//!
//! Exposes [`router`] so integration tests can spin up the server in-process.

use axum::Router;
use axum::routing::{get, post};

mod routes;
mod state;

pub use state::LemmyState;

use std::sync::Arc;

/// Backend-specific routes only (no lifecycle, no inspect middleware, no CORS).
///
/// Called by `BackendHarness::routes()`.
pub fn routes_only(_state: Arc<LemmyState>) -> Router<Arc<LemmyState>> {
    Router::new()
        .route("/health", get(routes::health))
        // Auth
        .route("/api/v3/user/login", post(routes::login))
        .route("/api/v3/user/logout", post(routes::logout))
        .route("/api/v3/user/register", post(routes::register))
        // Comments
        .route("/api/v3/comment", post(routes::create_comment))
        .route("/api/v3/comment/list", get(routes::list_comments))
        // Communities (servers)
        .route("/api/v3/community/list", get(routes::list_communities))
        .route("/api/v3/community", get(routes::get_community).put(routes::edit_community))
        // Community search (Discover Communities)
        .route("/api/v3/search", get(routes::search_communities))
        // Posts (messages in a community channel)
        .route("/api/v3/post/list", get(routes::list_posts))
        .route("/api/v3/post", get(routes::get_post))
        // Private messages
        .route("/api/v3/private_message/list", get(routes::list_private_messages))
        // User
        .route("/api/v3/user", get(routes::get_user))
        // Site info
        .route("/api/v3/site", get(routes::get_site))
        // Moderation
        .route("/api/v3/community/ban_user", post(routes::community_ban_user))
        .route("/api/v3/post/remove", post(routes::post_remove))
        .route("/api/v3/comment/remove", post(routes::comment_remove))
        .route("/api/v3/modlog", get(routes::get_modlog))
        // pict-rs image serving — Lemmy's convention for avatar/image URLs
        .route("/pictrs/image/{filename}", get(routes::serve_pictrs_image))
        // Test-only bypass: get a token without a password
        .route("/test/auth/token", post(routes::test_auth_token))
        // NOTE: no .with_state() here — build_router() provides it via the outer chain
}

/// Full router (backend routes + lifecycle + inspect + CORS).
///
/// Available for integration tests; seeded state variant.
pub fn router() -> Router {
    let state = Arc::new(LemmyState::new());
    state.seed();
    router_with_state(state)
}

/// Full router with explicit state.
pub fn router_with_state(state: Arc<LemmyState>) -> Router {
    poly_test_common::build_router::<LemmyState>(state)
}
