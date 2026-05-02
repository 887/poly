//! Mock Reddit test server entry point. Default port: 9108.

#![allow(clippy::expect_used)]

use poly_test_common::{CliArgs, TestServerBase};
use poly_test_reddit::{RedditState, router};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let state = Arc::new(RedditState::seeded());

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-reddit listening on {}", base.base_url());

    let app = router(state);
    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            drop(base.shutdown_rx.await);
        })
        .await?;

    Ok(())
}
