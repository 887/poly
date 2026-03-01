//! Poly Desktop Electron — WASM entry point.
//!
//! Compiles to WebAssembly using `dx build --platform web` from this directory.
//! The resulting `dist/` folder is loaded by `electron/main.js` inside an
//! Electron BrowserWindow, giving the full Poly UI in Chromium.
//!
//! This file is intentionally identical to `apps/web/src/main.rs` — the
//! electron wrapper adds packaging and native OS integration on top of the
//! same WASM build.

use poly_core::ui::App;

fn main() {
    tracing::info!("Starting Poly Desktop (Electron)");

    poly_core::i18n::init();
    poly_core::theme::init();

    dioxus::launch(App);
}
