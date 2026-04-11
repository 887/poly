//! Mock Stoat/Revolt API server for Poly testing.
//!
//! Implements the subset of the Revolt REST API + WebSocket (Bonfire) that
//! poly-stoat calls. In-memory state, no real federation.
//!
//! The router is defined in `lib.rs` so integration tests and the binary
//! share a single source of truth for route registration.

use poly_test_common::{CliArgs, TestServerBase};
use poly_test_stoat::{router, StoatState};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let state = Arc::new(StoatState::new());
    if args.seed {
        state.seed();
    }

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-stoat listening on {}", base.base_url());

    let app = router(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            let _ = base.shutdown_rx.await;
        })
        .await?;

    Ok(())
}
