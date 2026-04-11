//! # poly-host — host-bridge daemon for apps/web
//!
//! Single-user loopback HTTP daemon that serves `/host/*` on port 9333 so
//! `apps/web` (running in a real browser) has a native side to talk to.
//!
//! ## Run it
//!
//! ```sh
//! cargo run -p poly-host        # loopback :9333
//! ```
//!
//! Then in another terminal:
//!
//! ```sh
//! cd apps/web && dx serve --platform web --port 3000
//! ```
//!
//! The full route set, storage path, and design rationale live in
//! `docs/plans/phase-2.21-host-bridge-unification-plan.md` and in the
//! library crate ([`poly_host`]).

use std::net::SocketAddr;

use anyhow::{Context, Result};
use poly_host::{HostState, resolve_data_dir, serve};
use poly_host_bridge::BRIDGE_PORT;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,poly_host=debug")),
        )
        .init();

    let data_dir = resolve_data_dir();
    let db_path = data_dir.join("storage.sqlite3");
    let state = HostState::open(&db_path)
        .with_context(|| format!("init host state at {}", db_path.display()))?;

    let addr = SocketAddr::from(([127, 0, 0, 1], BRIDGE_PORT));
    serve(addr, state).await
}
