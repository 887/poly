//! Poly Desktop (Blitz) — experimental WGPU native renderer.
//!
//! Uses the Dioxus Blitz renderer (WGPU-based) instead of webview.
//! This is experimental and may not support all CSS features.
//!
//! DECISION(D1): Blitz is one of 3 desktop renderers.

use poly_core::ui::App;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,poly_core=debug")),
        )
        .init();

    tracing::info!("Starting Poly Desktop (Blitz — experimental)");

    poly_core::i18n::init();
    poly_core::theme::init();

    // TODO(phase-2.1.8): Use Blitz renderer when available
    // For now, falls back to standard Dioxus desktop
    dioxus::launch(App);
}
