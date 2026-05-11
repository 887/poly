//! Mock old.reddit.com test server library.
//!
//! See `routes.rs` for the surface; `state.rs` for the in-memory mock
//! state. Wire entry point:
//!
//! ```ignore
//! use poly_test_reddit::{router, RedditState};
//! use std::sync::Arc;
//! let state = Arc::new(RedditState::seeded());
//! let app = router(state);
//! // axum::serve(listener, app).await
//! ```

pub mod routes;
pub mod state;

use axum::{Router, routing::{get, post}};
use std::sync::Arc;

pub use state::RedditState;

/// Backend-specific routes only (no lifecycle, no inspect middleware,
/// no CORS). Called by `BackendHarness::routes()`.
#[must_use]
pub fn routes_only(_state: Arc<RedditState>) -> Router<Arc<RedditState>> {
    Router::new()
        // Anonymous read.
        .route("/", get(routes::frontpage))
        .route("/login", get(routes::login_page))
        .route("/login/", get(routes::login_page))
        .route("/r/{sub}/{sort}/", get(routes::list_subreddit))
        .route("/comments/{id}/", get(routes::get_post))
        .route("/comments/{id}/.json", get(routes::get_post_json))
        .route("/r/{sub}/comments/{id}/{slug}/", get(routes::get_post_with_slug))
        .route("/user/{name}/", get(routes::get_user))
        // Auth + JSON.
        .route("/api/me.json", get(routes::api_me))
        .route("/api/login/{user}", post(routes::login))
        // Authed reads.
        .route("/message/inbox/", get(routes::inbox))
        .route("/subreddits/mine/", get(routes::subreddits_mine_html))
        .route("/subreddits/mine/.json", get(routes::subreddits_mine_json))
        .route(
            "/subreddits/mine/subscriber/.json",
            get(routes::subreddits_mine_json),
        )
        // Community search + popular default (Discover Communities)
        .route("/subreddits/search.json", get(routes::subreddits_search))
        .route("/subreddits/popular.json", get(routes::subreddits_popular))
        // Per-subreddit deterministic letter SVG (community icons).
        .route("/sub-icons/{sub_with_ext}", get(routes::sub_icon))
        // Writes.
        .route("/api/subscribe", post(routes::subscribe))
        .route("/api/compose", post(routes::compose))
        .route("/api/comment", post(routes::comment))
        .route("/api/submit", post(routes::submit))
        .route("/api/vote", post(routes::vote))
        // Avatars.
        .route("/avatars/{animal}", get(routes::avatar))
        // Test-only convenience.
        .route("/test/reset", post(routes::test_reset))
}

/// Full router (backend routes + lifecycle + inspect + CORS) — kept for
/// integration tests that call `router(state)` directly without going
/// through `poly_test_common::run::<RedditState>()`.
#[must_use]
pub fn router(state: Arc<RedditState>) -> Router {
    poly_test_common::build_router::<RedditState>(state)
}

/// Default seeded router (cat + dog + r/rust + r/programming subscribed).
#[must_use]
pub fn router_default() -> Router {
    router(Arc::new(RedditState::seeded()))
}
