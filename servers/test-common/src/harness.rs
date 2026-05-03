//! `BackendHarness` trait + `run::<H>()` entry-point helper.
//!
//! Every mock test server implements this trait on its `XxxState` type,
//! then its `main.rs` becomes a one-liner:
//!
//! ```ignore
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     poly_test_common::run::<MyState>().await
//! }
//! ```
//!
//! The harness owns:
//! - CLI argument parsing (`--port`, `--seed`, `--reset`, `--verbose`)
//! - Auth state load/wipe
//! - Demo data seed on startup
//! - `/seed`, `/reset`, `/reseed` lifecycle HTTP endpoints
//! - `/test/inspect/last-headers` + header-inspect middleware
//! - `CorsLayer::very_permissive()`
//! - Graceful shutdown

use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Json;
use tower_http::cors::CorsLayer;

use crate::{
    AuthState, CliArgs, HeaderInspectBuffer, TestServerBase,
    handle_inspect_last_headers, header_inspect_middleware, wipe_persisted,
};

/// Shared contract for all poly mock test server states.
///
/// Implement on `XxxState`, call `poly_test_common::run::<XxxState>().await`
/// from `main`.
pub trait BackendHarness: Sized + Send + Sync + 'static {
    /// Short lowercase name used for logging and the auth-file path.
    const BACKEND: &'static str;

    /// Construct state from loaded auth tokens.
    /// Backends that don't use persisted auth may ignore the parameter.
    fn new(auth: AuthState) -> Self;

    /// Populate demo data. Must be idempotent (skip if already seeded).
    fn seed(&self) {}

    /// Wipe all in-memory data to the empty state.
    fn reset(&self) {}

    /// Wipe + re-seed. Default: `reset` then `seed`.
    fn reseed(&self) {
        self.reset();
        self.seed();
    }

    /// Return the per-backend-specific axum routes wired to `state`.
    ///
    /// Return type is `Router<Arc<Self>>` — leave the state slot open; do NOT
    /// call `.with_state()` inside this function. The harness calls
    /// `.with_state(state)` once after merging lifecycle + inspect routes.
    ///
    /// Do NOT include `/seed`, `/reset`, `/reseed`,
    /// `/test/inspect/last-headers`, the inspect middleware, or `CorsLayer`
    /// — the harness mounts all of those.
    fn routes(state: Arc<Self>) -> Router<Arc<Self>>;

    /// Return a reference to the shared header-inspect ring buffer.
    fn inspect_buf(&self) -> Arc<HeaderInspectBuffer>;

    /// Called after the TCP listener is bound, before serving begins.
    ///
    /// Override when the state needs to know the actual bound address (e.g.
    /// `test-discord` stores the gateway WebSocket URL).
    ///
    /// Default: no-op.
    fn post_bind(
        _state: &Arc<Self>,
        _addr: SocketAddr,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        Box::pin(async {})
    }
}

// ---- lifecycle axum handlers ------------------------------------------------

async fn lifecycle_seed<H: BackendHarness>(
    State(state): State<Arc<H>>,
) -> impl IntoResponse {
    state.seed();
    Json(serde_json::json!({ "status": "seeded" }))
}

async fn lifecycle_reset<H: BackendHarness>(
    State(state): State<Arc<H>>,
) -> impl IntoResponse {
    state.reset();
    Json(serde_json::json!({ "status": "reset" }))
}

async fn lifecycle_reseed<H: BackendHarness>(
    State(state): State<Arc<H>>,
) -> impl IntoResponse {
    state.reseed();
    Json(serde_json::json!({ "status": "reseeded" }))
}

// ---- router builder ---------------------------------------------------------

/// Build the full axum `Router`: backend routes + lifecycle + inspect + CORS.
///
/// Available to integration tests that want the full router without a
/// CLI/bind round-trip.
pub fn build_router<H: BackendHarness>(state: Arc<H>) -> Router {
    let inspect = state.inspect_buf();

    // Backend-specific routes — state slot still open (Router<Arc<H>>).
    let backend_routes: Router<Arc<H>> = H::routes(Arc::clone(&state));

    // Lifecycle routes also carry the state slot open.
    let lifecycle: Router<Arc<H>> = Router::new()
        .route("/seed", post(lifecycle_seed::<H>))
        .route("/reset", post(lifecycle_reset::<H>))
        .route("/reseed", post(lifecycle_reseed::<H>));

    // Inspect route: capture the inspect buffer in a closure so the handler
    // needs no separate state slot, allowing merge into Router<Arc<H>>.
    let inspect_clone = Arc::clone(&inspect);
    let inspect_route: Router<Arc<H>> = Router::new().route(
        "/test/inspect/last-headers",
        get(move || {
            let buf = Arc::clone(&inspect_clone);
            async move { handle_inspect_last_headers(State(buf)).await }
        }),
    );

    // Merge and resolve the state slot once.
    backend_routes
        .merge(lifecycle)
        .merge(inspect_route)
        .with_state(state)
        .layer(middleware::from_fn_with_state(
            Arc::clone(&inspect),
            header_inspect_middleware,
        ))
        .layer(CorsLayer::very_permissive())
}

// ---- run::<H>() entry point -------------------------------------------------

/// Full test-server lifecycle: CLI → auth → state → bind → serve → shutdown.
///
/// ```ignore
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     poly_test_common::run::<MyState>().await
/// }
/// ```
pub async fn run<H: BackendHarness>() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let auth_path = args.auth_path(H::BACKEND);
    if args.reset {
        wipe_persisted(&auth_path);
    }

    let auth = AuthState::load(auth_path);
    let state = Arc::new(H::new(auth));
    if args.seed {
        state.seed();
    }

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!(
        "poly-test-{} listening on {}",
        H::BACKEND,
        base.base_url()
    );

    H::post_bind(&state, base.addr).await;

    let app = build_router::<H>(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            drop(base.shutdown_rx.await);
        })
        .await?;

    Ok(())
}
