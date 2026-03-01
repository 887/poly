//! Poly Desktop (Wry) — main entry point.
//!
//! Launches the Poly messenger using Dioxus desktop with the system
//! webview (Wry) renderer.

use poly_core::ui::App;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,poly_core=debug".parse().unwrap()),
        )
        .init();

    tracing::info!("Starting Poly Desktop (Wry)");

    // Initialize poly-core subsystems
    // Note: Dioxus manages the async runtime
    poly_core::i18n::init();
    poly_core::theme::init();

    // Launch Dioxus desktop app
    dioxus::launch(App);
}
