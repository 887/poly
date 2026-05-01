//! Mock Stoat/Revolt API server for Poly testing.
//!
//! Implements the subset of the Revolt REST API + WebSocket (Bonfire) that
//! poly-stoat calls. In-memory state, no real federation.
//!
//! The router is defined in `lib.rs` so integration tests and the binary
//! share a single source of truth for route registration.

use poly_test_common::{wipe_persisted, AuthState, CliArgs, TestServerBase};
use poly_test_stoat::{router, StoatState};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let auth_path = args.auth_path("stoat");
    if args.reset {
        wipe_persisted(&auth_path);
    }

    let mut state = StoatState::new();
    state.auth = AuthState::load(auth_path);
    if args.seed {
        state.seed();
    }
    let state = Arc::new(state);

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-stoat listening on {}", base.base_url());

    let app = router(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            drop(base.shutdown_rx.await);
        })
        .await?;

    Ok(())
}
