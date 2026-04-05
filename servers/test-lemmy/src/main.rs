//! Mock Lemmy API v3 server for Poly testing.
//!
//! Implements the subset of the Lemmy REST API that `poly-lemmy` calls.
//! All state is hardcoded — no real persistence.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::routing::{get, post};
use axum::Router;
use poly_test_common::{health_handler, CliArgs, TestServerBase};
use tower_http::cors::CorsLayer;

mod routes;

pub const DEFAULT_PORT: u16 = 8536;

fn router() -> Router {
    Router::new()
        .route("/health", get(|| async { health_handler("lemmy").await }))
        // Auth
        .route("/api/v3/user/login", post(routes::login))
        // Site info
        .route("/api/v3/site", get(routes::get_site))
        // Communities
        .route("/api/v3/community/list", get(routes::list_communities))
        .route("/api/v3/community", get(routes::get_community))
        // Posts
        .route("/api/v3/post/list", get(routes::list_posts))
        // Comments
        .route("/api/v3/comment/list", get(routes::list_comments))
        .route("/api/v3/comment", post(routes::create_comment))
        // Private messages
        .route(
            "/api/v3/private_message/list",
            get(routes::list_private_messages),
        )
        // Test-only easy-signin
        .route("/test/auth/token", post(routes::test_auth_token))
        .layer(CorsLayer::very_permissive())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let port = if args.port == 0 { DEFAULT_PORT } else { args.port };
    let base = TestServerBase::bind(port).await?;
    tracing::info!("poly-test-lemmy listening on {}", base.base_url());

    axum::serve(base.listener, router())
        .with_graceful_shutdown(async {
            let _ = base.shutdown_rx.await;
        })
        .await?;

    Ok(())
}
