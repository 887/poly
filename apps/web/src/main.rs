//! Poly Web — client-side WASM entry point.
//!
//! Runs Poly in the browser using Dioxus web (pure WASM, no server component).
//! Use `dx serve --platform web` for development.

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

#[cfg(not(target_arch = "wasm32"))]
fn sync_mobile_query_flag_before_launch() {}

fn main() {
    tracing::info!("Starting Poly Web");

    // i18n::init() also registers native plugin FTL (e.g. demo translations).
    poly_core::i18n::init();
    poly_core::theme::init();
    sync_mobile_query_flag_before_launch();
    poly_core::install_wasm_crash_handler();

    dioxus::launch(App);
}
