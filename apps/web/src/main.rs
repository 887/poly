//! Poly Web — client-side WASM entry point.
//!
//! Runs Poly in the browser using Dioxus web (pure WASM, no server component).
//! Use `dx serve --platform web` for development.

use poly_core::ui::App;

fn main() {
    tracing::info!("Starting Poly Web");

    poly_core::i18n::init();
    poly_core::theme::init();

    dioxus::launch(App);
}
