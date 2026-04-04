//! Test wrapper around the real poly-server with /reset and /seed support.
//!
//! Unlike the other 4 mock servers (which are in-memory fakes), this wraps
//! the real `poly-server` as a library — improvements flow both ways.

use poly_test_common::{CliArgs, TestServerBase};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    args.init_tracing();

    let base = TestServerBase::bind(args.port).await?;
    tracing::info!("poly-test-poly listening on {}", base.base_url());

    // TODO(4.7): Wire up real poly-server router with:
    // - Temp SurrealKV database (auto-cleaned)
    // - /reset route: drop all data, re-seed
    // - /seed route: create Cockatoo + Parrot accounts, servers, channels, messages
    // - Serve avatar images for Cockatoo + Parrot
    //
    // For now, just run an empty server to validate the crate builds.
    let app = axum::Router::new().route(
        "/health",
        axum::routing::get(|| async {
            axum::Json(serde_json::json!({ "status": "ok", "backend": "poly" }))
        }),
    );

    axum::serve(base.listener, app)
        .with_graceful_shutdown(async {
            let _ = base.shutdown_rx.await;
        })
        .await?;

    Ok(())
}
