//! Mock Microsoft Teams/Graph API server for Poly testing.
//!
//! Implements the subset of the Microsoft Graph API that poly-teams calls.
//! In-memory state, mock OAuth2 token endpoint.

use poly_test_common::{wipe_persisted, AuthState, CliArgs, TestServerBase};
use poly_test_teams::{TeamsState, router};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let auth_path = args.auth_path("teams");
    if args.reset {
        wipe_persisted(&auth_path);
    }

    let mut state = TeamsState::new();
    state.auth = AuthState::load(auth_path);
    if args.seed {
        state.seed();
    }
    let state = Arc::new(state);

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-teams listening on {}", base.base_url());

    let app = router(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            let _ = base.shutdown_rx.await;
        })
        .await?;

    Ok(())
}
