//! Mock Matrix homeserver for Poly testing.
//!
//! Implements the subset of the Matrix Client-Server API that poly-matrix
//! calls. In-memory state, no federation. The router is defined in `lib.rs`
//! so integration tests and the binary share a single source of truth for
//! route registration and CORS configuration.

use poly_test_common::{wipe_persisted, AuthState, CliArgs, TestServerBase};
use poly_test_matrix::{router, MatrixState};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let auth_path = args.auth_path("matrix");
    if args.reset {
        wipe_persisted(&auth_path);
    }

    let mut state = MatrixState::new();
    state.auth = AuthState::load(auth_path);
    if args.seed {
        state.seed();
    }
    let state = Arc::new(state);

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-matrix listening on {}", base.base_url());

    let app = router(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            drop(base.shutdown_rx.await);
        })
        .await?;

    Ok(())
}
