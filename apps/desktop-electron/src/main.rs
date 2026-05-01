//! Poly Desktop Electron — Dioxus fullstack entry point.
//!
//! Two compilation targets come out of this one `main.rs`:
//!
//! * **WASM client** (`cfg(target_arch = "wasm32")`): booted inside the
//!   Electron BrowserWindow.
//! * **Native server** (`cfg(not(target_arch = "wasm32"))` with `server`
//!   feature): runs under `dx serve --platform web --fullstack` and
//!   serves the WASM bundle AND mounts the `/host/*` routes on the same
//!   port (3001 for this shell). One process, one port — Electron's
//!   renderer points at `http://127.0.0.1:3001/` and storage goes
//!   through the same SQLite file used by every other native shell.

#[cfg(any(target_arch = "wasm32", feature = "server"))]
use poly_core::ui::App;

// ─── WASM client entry ──────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
fn main() {
    tracing::info!("Starting Poly Desktop (Electron, WASM client)");

    poly_core::i18n::init();
    poly_core::theme::init();
    poly_core::install_wasm_crash_handler();

    dioxus::launch(App);
}

// ─── Native fullstack server entry ──────────────────────────────────────────

#[cfg(all(not(target_arch = "wasm32"), feature = "server"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use axum::Router;
    use dioxus::prelude::{DioxusRouterExt, ServeConfig};

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let data_dir = poly_host::resolve_data_dir();
    let db_path = data_dir.join("storage.sqlite3");
    let state = poly_host::HostState::open(&db_path)?;
    tracing::info!("poly-desktop-electron storage: {}", db_path.display());

    let dioxus_router: Router<()> =
        Router::new().serve_dioxus_application(ServeConfig::new(), App);
    let host_router: Router<()> = poly_host::router(state);
    let merged = host_router.merge(dioxus_router);

    let addr = dioxus_cli_config::fullstack_address_or_localhost();
    tracing::info!("poly-desktop-electron fullstack listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, merged).await?;
    Ok(())
}

// ─── Fallback (no server feature, native) ───────────────────────────────────

#[cfg(all(not(target_arch = "wasm32"), not(feature = "server")))]
fn main() {
    // lint-allow-unused: fallback main() has no logger initialised yet
    #[allow(clippy::print_stderr)]
    {
        eprintln!(
            "poly-desktop-electron binary built without the `server` feature; nothing to run. \
             Build with `--features server` or use `dx serve --platform web --fullstack`."
        );
    }
}
