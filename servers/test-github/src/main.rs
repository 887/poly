//! Mock GitHub API server for Poly testing.
//!
//! Serves a subset of the GitHub REST API v3 used by poly-github:
//! - `GET  /user` — authenticated user info
//! - `GET  /user/repos` — repos owned by the authenticated user
//! - `GET  /repos/{owner}/{repo}/issues` — issues and PRs
//! - `GET  /repos/{owner}/{repo}/issues/{number}/comments` — comments
//! - `GET  /repos/{owner}/{repo}/contents` — root directory listing
//! - `GET  /repos/{owner}/{repo}/contents/{path}` — file or directory
//! - `POST /test/auth/token` — test-only bypass: get token without password
//! - `GET  /health` — health check
//!
//! Default port: 9107.

use poly_test_common::{CliArgs, TestServerBase};
use poly_test_github::{GitHubState, router_with_state};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let state = Arc::new(GitHubState::new());
    if args.seed {
        state.seed();
    }

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-github listening on {}", base.base_url());

    let app = router_with_state(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            let _ = base.shutdown_rx.await;
        })
        .await?;

    Ok(())
}
