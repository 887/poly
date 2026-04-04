//! Test wrapper around the real poly-server with /reset, /seed, /reseed support.
//!
//! Uses SQLite in-memory for fast compilation and zero disk overhead.
//! The real poly-server defaults to SurrealDB in production.

use std::sync::Arc;

use axum::middleware;
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use poly_server::{api, auth, db, ws, AppState, Config};
use poly_test_common::{CliArgs, TestServerBase};

fn build_config(uploads_dir: &str) -> Config {
    Config {
        bind_addr: "unused".into(), // We bind via TestServerBase
        db_path: ":memory:".to_string(),
        server_name: "Poly Test Server".into(),
        invite_only: false,
        jwt_secret: "test-secret-for-poly-test-server-do-not-use-in-prod".into(),
        jwt_expiry_secs: 60 * 60 * 24, // 1 day for tests
        uploads_dir: uploads_dir.to_string(),
    }
}

async fn build_state(uploads_dir: &str) -> anyhow::Result<AppState> {
    let config = Arc::new(build_config(uploads_dir));
    let db = Arc::new(db::init(&config).await?);
    let ws = Arc::new(ws::WsState::new());
    Ok(AppState { db, config, ws })
}

fn poly_router(state: AppState) -> Router {
    // Real poly-server router with auth middleware
    let protected = api::router()
        .merge(auth::routes::protected_router())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ));

    Router::new()
        .merge(auth::routes::public_router())
        .merge(protected)
        .merge(ws::router())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    // Create temp directory for uploads only (DB is in-memory).
    let tmp_uploads = tempfile::tempdir()?;
    let uploads_dir = tmp_uploads.path().to_string_lossy().to_string();

    tracing::info!("Using in-memory SQLite database");
    tracing::info!("Using temp uploads at {uploads_dir}");

    let state = build_state(&uploads_dir).await?;

    // TODO(4.7): Seed demo data (Cockatoo + Parrot) if --seed flag is set
    if args.seed {
        tracing::info!("TODO: seed Cockatoo + Parrot demo data");
    }

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-poly listening on {}", base.base_url());

    // Build app: real poly-server routes + lifecycle routes
    let lifecycle = Router::new()
        .route("/health", get(|| async {
            Json(serde_json::json!({ "status": "ok", "backend": "poly" }))
        }))
        .route("/seed", post(|| async {
            // TODO(4.7): Seed Cockatoo + Parrot demo data
            Json(serde_json::json!({ "status": "seeded" }))
        }))
        .route("/reset", post(|| async {
            // TODO(4.7): Wipe database tables
            Json(serde_json::json!({ "status": "reset" }))
        }))
        .route("/reseed", post(|| async {
            // TODO(4.7): Wipe + re-seed
            Json(serde_json::json!({ "status": "reseeded" }))
        }));

    let app = poly_router(state).merge(lifecycle);

    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            let _ = base.shutdown_rx.await;
        })
        .await?;

    // temp dir cleaned up when dropped
    drop(tmp_uploads);

    Ok(())
}
