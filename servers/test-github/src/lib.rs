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
//! Mock GitHub API server for Poly testing — library entry point.
//!
//! Exposes [`router`] so integration tests can spin up the server in-process.

use axum::Router;
use axum::routing::{delete, get, post};
use std::sync::Arc;

mod routes;
mod state;

pub use state::GitHubState;

/// Backend-specific routes only (no lifecycle, no inspect middleware, no CORS).
///
/// Called by `BackendHarness::routes()`.
pub fn routes_only(_state: Arc<GitHubState>) -> Router<Arc<GitHubState>> {
    Router::new()
        .route("/health", get(routes::health))
        .route("/user", get(routes::get_user))
        .route("/user/repos", get(routes::list_user_repos))
        .route("/repos/{owner}/{repo}", get(routes::get_repo))
        .route("/repos/{owner}/{repo}/issues", get(routes::list_issues))
        .route("/repos/{owner}/{repo}/issues/{number}", get(routes::get_issue))
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
        .route("/user/starred/{owner}/{repo}", get(routes::check_starred))
        .route("/repos/{owner}/{repo}/contents", get(routes::get_contents_root))
        .route("/repos/{owner}/{repo}/contents/{*path}", get(routes::get_contents))
        .route("/graphql", post(routes::graphql))
        .route("/avatars/{filename}", get(routes::serve_avatar))
        .route("/test/auth/token", post(routes::test_auth_token))
        // NOTE: no .with_state() here — build_router() provides it via the outer chain
}

/// Full router with freshly seeded state. Available for integration tests.
pub fn router() -> Router {
    let state = Arc::new(GitHubState::new());
    state.seed();
    router_with_state(state)
}

/// Full router with explicit state.
pub fn router_with_state(state: Arc<GitHubState>) -> Router {
    poly_test_common::build_router::<GitHubState>(state)
}
