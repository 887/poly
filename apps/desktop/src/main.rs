//! Poly Desktop (Wry) — main entry point.
//!
//! Launches the Poly messenger using Dioxus desktop with the system
//! webview (Wry) renderer.
//!
//! When compiled for WASM (via `dx serve --platform web` in web-shell mode),
//! the tracing_subscriber and tokio deps are unavailable — we skip them.

use poly_core::ui::App;

fn main() {
    // Initialize logging (native only — tracing-subscriber isn't available on wasm32)
    #[cfg(not(target_arch = "wasm32"))]
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,poly_core=debug")),
        )
        .init();

    tracing::info!("Starting Poly Desktop (Wry)");

    // Initialize poly-core subsystems
    poly_core::i18n::init();
    poly_core::theme::init();

    #[cfg(target_arch = "wasm32")]
    poly_core::install_wasm_crash_handler();

    // Launch Dioxus app
    dioxus::launch(App);
}
