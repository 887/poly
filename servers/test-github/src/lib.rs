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

use axum::middleware;
use axum::Router;
use axum::routing::{get, post};
use poly_test_common::{handle_inspect_last_headers, header_inspect_middleware};
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
    use axum::routing::delete;

    let inspect = Arc::clone(&state.inspect);
    Router::new()
        .route("/health", get(routes::health))
        .route("/user", get(routes::get_user))
        .route("/user/repos", get(routes::list_user_repos))
        .route(
            "/repos/{owner}/{repo}",
            get(routes::get_repo),
        )
        .route(
            "/repos/{owner}/{repo}/issues",
            get(routes::list_issues),
        )
        .route(
            "/repos/{owner}/{repo}/issues/{number}",
            get(routes::get_issue),
        )
        .route(
            "/repos/{owner}/{repo}/issues/{number}/comments",
            get(routes::list_comments),
        )
        .route(
            "/repos/{owner}/{repo}/issues/comments/{comment_id}",
            delete(routes::delete_issue_comment),
        )
        .route(
            "/repos/{owner}/{repo}/pulls/comments/{comment_id}",
            delete(routes::delete_pr_comment),
        )
        .route(
            "/user/starred/{owner}/{repo}",
            get(routes::check_starred),
        )
        .route(
            "/repos/{owner}/{repo}/contents",
            get(routes::get_contents_root),
        )
        .route(
            "/repos/{owner}/{repo}/contents/{*path}",
            get(routes::get_contents),
        )
        .route("/graphql", post(routes::graphql))
        .route("/avatars/{filename}", get(routes::serve_avatar))
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
