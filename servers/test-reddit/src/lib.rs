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
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub use state::RedditState;

/// Build the axum router. Caller supplies the (already-seeded) state.
#[must_use]
pub fn router(state: Arc<RedditState>) -> Router {
    Router::new()
        // Anonymous read.
        .route("/", get(routes::frontpage))
        .route("/login", get(routes::login_page))
        .route("/login/", get(routes::login_page))
        .route("/r/{sub}/{sort}/", get(routes::list_subreddit))
        .route("/comments/{id}/", get(routes::get_post))
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
        // Writes.
        .route("/api/subscribe", post(routes::subscribe))
        .route("/api/compose", post(routes::compose))
        .route("/api/comment", post(routes::comment))
        .route("/api/vote", post(routes::vote))
        // Avatars.
        .route("/avatars/{animal}", get(routes::avatar))
        // Test-only convenience.
        .route("/test/reset", post(routes::test_reset))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Default seeded router (cat + dog + r/rust + r/programming subscribed).
#[must_use]
pub fn router_default() -> Router {
    router(Arc::new(RedditState::seeded()))
}
