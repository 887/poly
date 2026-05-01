//! Mock Discord API server for Poly testing.
//!
//! Implements the subset of the Discord REST API that poly-discord calls.
//! In-memory state, mock token auth.

use poly_test_common::{wipe_persisted, AuthState, CliArgs, TestServerBase};
use poly_test_discord::{DiscordState, router};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let auth_path = args.auth_path("discord");
    if args.reset {
        wipe_persisted(&auth_path);
    }

    let mut state = DiscordState::new();
    state.auth = AuthState::load(auth_path);
    if args.seed {
        state.seed();
    }
    let state = Arc::new(state);

    let base = TestServerBase::bind(args.port).await?;
    // Set the gateway URL based on the actual bound address so the
    // `/api/v10/gateway` endpoint hands clients a working ws:// URL —
    // the default placeholder hardcodes a fixed port and omits the path.
    *state.gateway_url.write().await = format!("ws://{}/gateway/ws", base.addr);
    tracing::info!("poly-test-discord listening on {}", base.base_url());

    let app = router(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            drop(base.shutdown_rx.await);
        })
        .await?;

    Ok(())
}
