#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code
)]
//! Mock GitHub API server for Poly testing — library entry point.
//!
//! Exposes [`router`] so integration tests can spin up the server in-process.

use axum::Router;
use axum::routing::{get, post};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

mod routes;
mod state;

pub use state::GitHubState;

/// Build the Axum router for the mock GitHub server (with freshly seeded state).
pub fn router() -> Router {
    let state = Arc::new(GitHubState::new());
    state.seed();
    router_with_state(state)
}

/// Build the router with explicit state (used by `main.rs` for seeded startup).
pub fn router_with_state(state: Arc<GitHubState>) -> Router {
    Router::new()
        .route("/health", get(routes::health))
        .route("/user", get(routes::get_user))
        .route("/user/repos", get(routes::list_user_repos))
        .route(
            "/repos/{owner}/{repo}/issues",
            get(routes::list_issues),
        )
        .route(
            "/repos/{owner}/{repo}/issues/{number}/comments",
            get(routes::list_comments),
        )
        .route(
            "/repos/{owner}/{repo}/contents",
            get(routes::get_contents_root),
        )
        .route(
            "/repos/{owner}/{repo}/contents/{*path}",
            get(routes::get_contents),
        )
        .route("/test/auth/token", post(routes::test_auth_token))
        .with_state(state)
        .layer(CorsLayer::very_permissive())
}
