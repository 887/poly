//! Poly Web — Dioxus fullstack entry point.
//!
//! Two compilation targets come out of this one `main.rs`:
//!
//! * **WASM client** — `cfg(target_arch = "wasm32")`. Boots the app via
//!   `dioxus::launch` inside the browser.
//! * **Native server** — `cfg(not(target_arch = "wasm32"))` with the
//!   `server` feature. Starts an axum server that serves the WASM bundle
//!   AND mounts the `/host/*` host-bridge routes on the same port (3000
//!   under `dx serve`). ONE process, ONE port, no separate daemon.
//!
//! Use `dx serve --platform web` for development — dx auto-detects the
//! `fullstack` feature activation via `dioxus/fullstack` and builds both
//! sides.

// WebSandbox — browser popup + postMessage capture (Phase C of plan-host-sandbox-impl.md).
// Only compiled for the WASM target; the server half is handled by the
// `/sandbox/<id>` route in apps/poly-host (C.1).
#[cfg(target_arch = "wasm32")]
mod sandbox;
// Re-export so the type is reachable for future host-cap registry wiring.
#[cfg(target_arch = "wasm32")]
pub use sandbox::WebSandbox;

#[cfg(any(target_arch = "wasm32", feature = "server"))]
use poly_core::ui::App;

#[cfg(target_arch = "wasm32")]
fn sync_mobile_query_flag_before_launch() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(Some(storage)) = window.local_storage() else {
        return;
    };
    let _ = storage.remove_item("poly.forceMobileUi");
}

// ─── WASM client entry ──────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
fn main() {
    tracing::info!("Starting Poly Web (WASM client)");
    poly_core::i18n::init();
    poly_core::theme::init();
    sync_mobile_query_flag_before_launch();
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

    // Open the shared SQLite file used by every native shell. No port
    // negotiation, no sidecar: the server binary talks to SQLite directly.
    let data_dir = poly_host::resolve_data_dir();
    let db_path = data_dir.join("storage.sqlite3");
    // D.3: advertise SandboxBrowser so the WASM settings UI can show the
    // sandbox-status row as "available" on the web shell (WebSandbox / postMessage).
    let caps: Vec<String> = poly_host_sandbox::advertised_host_caps()
        .iter()
        .map(|c| match c {
            poly_host_sandbox::HostCap::SandboxBrowser => "SandboxBrowser",
            poly_host_sandbox::HostCap::SystemTray => "SystemTray",
            poly_host_sandbox::HostCap::OsNotifications => "OsNotifications",
        })
        .map(str::to_string)
        .collect();
    let state = poly_host::HostState::open(&db_path)?.with_caps(caps);
    tracing::info!("poly-web storage: {}", db_path.display());

    // Build the merged router. `serve_dioxus_application` returns a
    // `Router<()>` that already has its state resolved, so we can `.merge`
    // it with the `/host/*` router directly.
    let dioxus_router: Router<()> =
        Router::new().serve_dioxus_application(ServeConfig::new(), App);
    let host_router: Router<()> = poly_host::router(state);
    let merged = host_router.merge(dioxus_router);

    let addr = dioxus_cli_config::fullstack_address_or_localhost();
    tracing::info!("poly-web fullstack listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, merged).await?;
    Ok(())
}

// ─── Fallback (no server feature, native) ───────────────────────────────────
//
// Lets `cargo build -p poly-web` work on native without pulling the server
// runtime (useful for lints / `cargo check` in CI).

#[cfg(all(not(target_arch = "wasm32"), not(feature = "server")))]
fn main() {
    // No tracing subscriber installed in this no-server fallback build, so
    // emit the diagnostic via `writeln!` to stderr directly (avoids the
    // workspace-wide `print_stderr` lint).
    use std::io::Write as _;
    let mut stderr = std::io::stderr().lock();
    drop(writeln!(
        stderr,
        "poly-web binary built without the `server` feature; nothing to run. \
         Build with `--features server` or use `dx serve --platform web`."
    ));
}
