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
    let Ok(search) = window.location().search() else {
        return;
    };
    let Ok(Some(storage)) = window.local_storage() else {
        return;
    };

    for segment in search.trim_start_matches('?').split('&') {
        if segment.is_empty() {
            continue;
        }
        let mut parts = segment.splitn(2, '=');
        let Some(key) = parts.next() else {
            continue;
        };
        if !matches!(key, "mobile" | "polyMobile" | "forceMobile") {
            continue;
        }

        let value = parts.next().unwrap_or_default();
        if matches!(value, "1" | "true" | "yes" | "on") {
            let _ = storage.set_item("poly.forceMobileUi", "1");
        } else if matches!(value, "0" | "false" | "no" | "off") {
            let _ = storage.set_item("poly.forceMobileUi", "0");
        }
    }
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
