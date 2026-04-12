//! Mock Forgejo API server for Poly testing — library entry point.
//!
//! Exposes [`router`] so integration tests can spin up the server in-process.

use axum::Router;
use axum::routing::{get, post};
use tower_http::cors::CorsLayer;

mod routes;
mod state;

pub use state::ForgejoState;

use std::sync::Arc;

/// Build the Axum router for the mock Forgejo server (with freshly seeded state).
pub fn router() -> Router {
    let state = Arc::new(ForgejoState::new());
    state.seed();
    router_with_state(state)
}

/// Build the router with explicit state (used by `main.rs` for seeded startup).
pub fn router_with_state(state: Arc<ForgejoState>) -> Router {
    Router::new()
        .route("/health", get(routes::health))
        .route("/api/v1/user", get(routes::get_user))
        .route("/api/v1/user/repos", get(routes::list_user_repos))
        .route(
            "/api/v1/repos/{owner}/{repo}/issues",
            get(routes::list_issues),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/issues/{number}/comments",
            get(routes::list_comments),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/contents",
            get(routes::get_contents_root),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/contents/{path}",
            get(routes::get_contents),
        )
        .route("/test/auth/token", post(routes::test_auth_token))
        .with_state(state)
        .layer(CorsLayer::very_permissive())
}
