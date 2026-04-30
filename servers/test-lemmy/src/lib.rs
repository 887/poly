#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code
)]
//! Mock Lemmy API server for Poly testing — library entry point.
//!
//! Exposes [`router`] so integration tests can spin up the server in-process.

use axum::middleware;
use axum::Router;
use axum::routing::{get, post};
use tower_http::cors::CorsLayer;

mod routes;
mod state;

pub use state::LemmyState;

use poly_test_common::{handle_inspect_last_headers, header_inspect_middleware};
use std::sync::Arc;

/// Build the Axum router for the mock Lemmy server.
///
/// The router is shared between the standalone binary (`main.rs`) and
/// integration tests, which call this function directly to start an
/// in-process server.
pub fn router() -> Router {
    let state = Arc::new(LemmyState::new());
    state.seed();
    router_with_state(state)
}

/// Build the router with explicit state (used by `main.rs` for seeded startup).
pub fn router_with_state(state: Arc<LemmyState>) -> Router {
    let inspect = Arc::clone(&state.inspect);
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
