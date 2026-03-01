// Poly Server — self-hosted chat backend (reference implementation)
//
// TODO(phase-2.2): Full implementation per phase-2.2-plan.md
//
// DECISION(DX-S01): Pragmatic auth — argon2 password hashing + custom JWT rather than
// SurrealDB RECORD ACCESS, because embedded SurrealKV does not expose multi-session
// isolation cleanly from Rust. Permissions are enforced in Rust handlers.

use std::sync::Arc;

use axum::{Router, middleware};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;
use tracing_subscriber::EnvFilter;

use poly_server::{AppState, Config, api, auth, db, ws};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,poly_server=debug".to_owned()),
        ))
        .init();

    let config = Arc::new(Config::from_env());
    info!("Starting Poly Server on {}", config.bind_addr);

    let db = Arc::new(db::init(&config).await?);
    let ws = Arc::new(ws::WsState::new());

    let state = AppState {
        db,
        config: config.clone(),
        ws,
    };

    // Apply auth middleware to the API and protected auth routes.
    let protected = api::router()
        .merge(auth::routes::protected_router())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ));

    let app = Router::new()
        .merge(auth::routes::public_router())
        .merge(protected)
        .merge(ws::router())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    info!("Listening on http://{}", config.bind_addr);
    axum::serve(listener, app).await?;
    Ok(())
}
