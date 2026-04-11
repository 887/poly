//! Poly Desktop — Dioxus Wry (native) + fullstack server entry point.
//!
//! Three compilation targets come out of this one `main.rs`:
//!
//! * **Native Wry desktop** — default features (`desktop-native`).
//!   Launches a full Dioxus desktop window via the Wry renderer.
//!   Production build.
//! * **WASM client** — `cfg(target_arch = "wasm32")`. Built by
//!   `dx serve --platform web` (port 3002) and hosted inside the
//!   `apps/desktop-web` Wry dev shell.
//! * **Native fullstack server** — `cfg(not(target_arch = "wasm32"))`
//!   with `server` feature. Runs under `dx serve --platform web
//!   --fullstack @server --features server` to serve the WASM bundle
//!   AND mount `/host/*` on port 3002 in dev mode.

// ─── WASM client entry ──────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
fn main() {
    tracing::info!("Starting Poly Desktop (WASM client)");
    poly_core::i18n::init();
    poly_core::theme::init();
    poly_core::install_wasm_crash_handler();
    dioxus::launch(poly_core::ui::App);
}

// ─── Native fullstack server entry (dev shell) ──────────────────────────────

#[cfg(all(not(target_arch = "wasm32"), feature = "server"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use axum::Router;
    use dioxus::prelude::{DioxusRouterExt, ServeConfig};

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,poly_core=debug")),
        )
        .init();

    let data_dir = poly_host::resolve_data_dir();
    let db_path = data_dir.join("storage.sqlite3");
    let state = poly_host::HostState::open(&db_path)?;
    tracing::info!("poly-desktop storage: {}", db_path.display());

    let dioxus_router: Router<()> =
        Router::new().serve_dioxus_application(ServeConfig::new(), poly_core::ui::App);
    let host_router: Router<()> = poly_host::router(state);
    let merged = host_router.merge(dioxus_router);

    let addr = dioxus_cli_config::fullstack_address_or_localhost();
    tracing::info!("poly-desktop fullstack listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, merged).await?;
    Ok(())
}

// ─── Native Wry desktop entry (production) ──────────────────────────────────

#[cfg(all(not(target_arch = "wasm32"), not(feature = "server")))]
fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,poly_core=debug")),
        )
        .init();

    tracing::info!("Starting Poly Desktop (Wry)");

    poly_core::i18n::init();
    poly_core::theme::init();

    dioxus::launch(poly_core::ui::App);
}
