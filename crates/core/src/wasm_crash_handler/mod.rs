//! Browser/WASM crash reporting helpers.
//!
//! Installs a panic hook plus global browser error listeners so route crashes
//! become visible in the DOM instead of silently wedging the page.
//!
//! Also installs a boot-hang watchdog: a JS `setTimeout` that fires after
//! `BOOT_HANG_TIMEOUT_MS` milliseconds. If the startup overlay has not
//! dismissed by then (detected via the `data-poly-startup-phase` DOM attribute
//! written by the App component), the watchdog shows the crash overlay with a
//! "not responding" message. This catches infinite Dioxus render loops that
//! never progress past the loading screen without triggering a Rust panic.
//!
//! # Sub-modules (single responsibility)
//!
//! | Module | Responsibility |
//! |--------|---------------|
//! | `overlay` | DOM crash-overlay rendering and JS crash-state storage |
//! | `panic_hook` | Rust panic hook + `window.onerror` / `unhandledrejection` listeners |
//! | `watchdog` | Boot-hang and interaction-hang watchdogs; ServiceWorker force-reloader |

use std::sync::Once;

mod overlay;
mod panic_hook;
mod watchdog;

/// Install the shared browser/WASM crash handler once for the current page.
pub fn install_wasm_crash_handler() {
    static INSTALLED: Once = Once::new();

    INSTALLED.call_once(|| {
        overlay::clear_previous_crash_state();
        panic_hook::install_panic_hook();
        panic_hook::install_window_error_listener();
        panic_hook::install_unhandled_rejection_listener();
        watchdog::install_boot_hang_watchdog(watchdog::BOOT_HANG_TIMEOUT_MS);
        watchdog::install_interaction_hang_watchdog(watchdog::INTERACTION_HANG_TIMEOUT_MS);
        watchdog::install_service_worker_force_reloader();
    });
}
