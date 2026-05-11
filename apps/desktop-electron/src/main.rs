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

#[cfg(all(not(target_arch = "wasm32"), feature = "server"))]
mod sandbox;

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
    // D.3: propagate caps to the shared /host/caps handler too.
    // The Electron-specific sandbox_router also adds /host/caps (takes
    // precedence in the merge order), but having it in HostState ensures
    // consistency if the merge order ever changes.
    let caps: Vec<String> = sandbox::advertised_host_caps()
        .iter()
        .map(|c| match c {
            poly_host_sandbox::HostCap::SandboxBrowser => "SandboxBrowser",
            poly_host_sandbox::HostCap::SystemTray => "SystemTray",
            poly_host_sandbox::HostCap::OsNotifications => "OsNotifications",
        })
        .map(str::to_string)
        .collect();
    let state = poly_host::HostState::open(&db_path)?.with_caps(caps);
    tracing::info!("poly-desktop-electron storage: {}", db_path.display());

    let dioxus_router: Router<()> =
        Router::new().serve_dioxus_application(ServeConfig::new(), App);
    let host_router: Router<()> = poly_host::router(state);
    // Electron-specific routes: sandbox-browser capability.
    let electron_router: Router<()> = sandbox_router();
    let merged = electron_router.merge(host_router).merge(dioxus_router);

    let addr = dioxus_cli_config::fullstack_address_or_localhost();
    tracing::info!("poly-desktop-electron fullstack listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, merged).await?;
    Ok(())
}

/// Build the Electron-specific axum router for sandbox and caps endpoints.
///
/// Routes:
/// - `GET  /host/caps`             — returns advertised host capabilities.
/// - `POST /host/sandbox/open`     — open a sandboxed browser, resolve on capture.
#[cfg(all(not(target_arch = "wasm32"), feature = "server"))]
fn sandbox_router() -> axum::Router<()> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/host/caps", get(host_caps_handler))
        .route("/host/sandbox/open", post(sandbox_open_handler))
}

/// `GET /host/caps` — returns the JSON list of advertised host capabilities.
///
/// Response: `{ "caps": ["SandboxBrowser"] }`
#[cfg(all(not(target_arch = "wasm32"), feature = "server"))]
async fn host_caps_handler() -> axum::Json<serde_json::Value> {
    let caps: Vec<&str> = sandbox::advertised_host_caps()
        .iter()
        .map(|c| match c {
            poly_host_sandbox::HostCap::SandboxBrowser => "SandboxBrowser",
            poly_host_sandbox::HostCap::SystemTray => "SystemTray",
            poly_host_sandbox::HostCap::OsNotifications => "OsNotifications",
        })
        .collect();
    axum::Json(serde_json::json!({ "caps": caps }))
}

/// Request body for `POST /host/sandbox/open`.
#[cfg(all(not(target_arch = "wasm32"), feature = "server"))]
#[derive(serde::Deserialize)]
struct SandboxOpenRequest {
    url: String,
    capture_url_pattern: String,
}

/// `POST /host/sandbox/open` — open a sandboxed browser window.
///
/// Request: `{ "url": "...", "capture_url_pattern": "..." }`
/// Response (success): `{ "ok": true, "captured_url": "..." }`
/// Response (error):   `{ "ok": false, "error": "..." }`
#[cfg(all(not(target_arch = "wasm32"), feature = "server"))]
async fn sandbox_open_handler(
    axum::Json(req): axum::Json<SandboxOpenRequest>,
) -> axum::Json<serde_json::Value> {
    use poly_host_sandbox::HostSandbox as _;
    let sb = sandbox::ElectronSandbox::new();
    match sb.open_browser_sandbox(req.url, req.capture_url_pattern).await {
        Ok(result) => {
            axum::Json(serde_json::json!({ "ok": true, "captured_url": result.captured_url }))
        }
        Err(poly_host_sandbox::SandboxError::UserCancelled) => {
            axum::Json(serde_json::json!({ "ok": false, "error": "UserCancelled" }))
        }
        Err(e) => {
            axum::Json(serde_json::json!({ "ok": false, "error": e.to_string() }))
        }
    }
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
