//! Mock Discord API server for Poly testing.
//!
//! Implements the subset of the Discord REST API that poly-discord calls.
//! In-memory state, mock token auth.

use poly_test_common::{CliArgs, TestServerBase};
use poly_test_discord::{DiscordState, router};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let state = Arc::new(DiscordState::new());
    if args.seed {
        state.seed();
    }

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-discord listening on {}", base.base_url());

    let app = router(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            let _ = base.shutdown_rx.await;
        })
        .await?;

    Ok(())
}
