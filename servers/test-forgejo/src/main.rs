//! Mock Forgejo API server for Poly testing.
//!
//! Serves a subset of the Forgejo REST API v1 used by poly-forgejo:
//! - `GET  /api/v1/user` — authenticated user info
//! - `GET  /api/v1/user/repos` — repos owned by the authenticated user
//! - `GET  /api/v1/repos/{owner}/{repo}/issues` — issues and PRs
//! - `GET  /api/v1/repos/{owner}/{repo}/issues/{number}/comments` — comments
//! - `GET  /api/v1/repos/{owner}/{repo}/contents` — root directory listing
//! - `GET  /api/v1/repos/{owner}/{repo}/contents/{path}` — file or directory
//! - `POST /test/auth/token` — test-only bypass: get token without password
//! - `GET  /health` — health check
//!
//! Default port: 9106.

use poly_test_common::{wipe_persisted, AuthState, CliArgs, TestServerBase};
use poly_test_forgejo::{ForgejoState, router_with_state};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let auth_path = args.auth_path("forgejo");
    if args.reset {
        wipe_persisted(&auth_path);
    }

    let mut state = ForgejoState::new();
    state.auth = AuthState::load(auth_path);
    if args.seed {
        state.seed();
    }
    let state = Arc::new(state);

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-forgejo listening on {}", base.base_url());

    let app = router_with_state(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            drop(base.shutdown_rx.await);
        })
        .await?;

    Ok(())
}
