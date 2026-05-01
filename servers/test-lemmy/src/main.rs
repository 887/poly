//! Mock Lemmy API server for Poly testing.
//!
//! Serves a subset of the Lemmy REST API used by poly-lemmy:
//! - `POST /api/v3/user/login` — authenticate and get a JWT
//! - `POST /api/v3/user/logout` — invalidate the session
//! - `GET  /api/v3/community/list` — list subscribed communities
//! - `GET  /api/v3/community` — get a single community
//! - `GET  /api/v3/post/list` — list posts for a community
//! - `GET  /api/v3/private_message/list` — list private messages
//! - `GET  /api/v3/user` — get user profile
//! - `POST /test/auth/token` — test-only bypass: get token without password
//! - `GET  /health` — health check
//!
//! Default port: 8538.

use poly_test_common::{wipe_persisted, AuthState, CliArgs, TestServerBase};
use poly_test_lemmy::{LemmyState, router_with_state};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let auth_path = args.auth_path("lemmy");
    if args.reset {
        wipe_persisted(&auth_path);
    }

    let mut state = LemmyState::new();
    state.auth = AuthState::load(auth_path);
    if args.seed {
        state.seed();
    }
    let state = Arc::new(state);

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-lemmy listening on {}", base.base_url());

    let app = router_with_state(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            drop(base.shutdown_rx.await);
        })
        .await?;

    Ok(())
}
