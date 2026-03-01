//! Poly Android — mobile entry point.
//!
//! Launches Poly using the Dioxus mobile renderer for Android.

use poly_core::ui::App;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,poly_core=debug")),
        )
        .init();

    tracing::info!("Starting Poly Android");

    poly_core::i18n::init();
    poly_core::theme::init();

    dioxus::launch(App);
}
