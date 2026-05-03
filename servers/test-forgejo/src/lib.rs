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

use axum::Router;
use axum::routing::{delete, get, post};
use std::sync::Arc;

mod routes;
mod state;

pub use state::ForgejoState;

/// Backend-specific routes only (no lifecycle, no inspect middleware, no CORS).
///
/// Called by `BackendHarness::routes()`.
pub fn routes_only(_state: Arc<ForgejoState>) -> Router<Arc<ForgejoState>> {
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
        .route("/api/v1/repos/{owner}/{repo}", get(routes::get_repo))
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
        // NOTE: no .with_state() here — build_router() provides it via the outer chain
}

/// Full router with freshly seeded state. Available for integration tests.
pub fn router() -> Router {
    let state = Arc::new(ForgejoState::new());
    state.seed();
    router_with_state(state)
}

/// Full router with explicit state.
pub fn router_with_state(state: Arc<ForgejoState>) -> Router {
    poly_test_common::build_router::<ForgejoState>(state)
}
