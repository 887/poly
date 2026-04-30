#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code
)]
//! Mock Forgejo API server for Poly testing — library entry point.
//!
//! Exposes [`router`] so integration tests can spin up the server in-process.

use axum::middleware;
use axum::Router;
use axum::routing::{delete, get, post};
use tower_http::cors::CorsLayer;

mod routes;
mod state;

pub use state::ForgejoState;

use poly_test_common::{handle_inspect_last_headers, header_inspect_middleware};
use std::sync::Arc;

/// Build the Axum router for the mock Forgejo server (with freshly seeded state).
pub fn router() -> Router {
    let state = Arc::new(ForgejoState::new());
    state.seed();
    router_with_state(state)
}

/// Build the router with explicit state (used by `main.rs` for seeded startup).
pub fn router_with_state(state: Arc<ForgejoState>) -> Router {
    let inspect = Arc::clone(&state.inspect);
    Router::new()
        .route("/health", get(routes::health))
        .route("/api/v1/user", get(routes::get_user))
        .route("/api/v1/user/repos", get(routes::list_user_repos))
        .route(
            "/api/v1/user/starred/{owner}/{repo}",
            get(routes::check_starred),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/issues",
            get(routes::list_issues),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/issues/{index}",
            get(routes::get_issue),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/issues/{number}/comments",
            get(routes::list_comments),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}",
            get(routes::get_repo),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/issues/comments/{id}",
            delete(routes::delete_issue_comment),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/contents",
            get(routes::get_contents_root),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/contents/{path}",
            get(routes::get_contents),
        )
        .route("/avatars/{name}", get(routes::serve_avatar))
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
