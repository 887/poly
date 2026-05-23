//! Rust panic hook and browser error-event listeners.
//!
//! Installs three independent crash reporters:
//! - The Rust `std::panic::set_hook` — catches Rust panics.
//! - `window.onerror` — catches synchronous JS exceptions.
//! - `window.unhandledrejection` — catches unhandled promise rejections.
//!
//! Each reporter calls [`super::overlay::report_crash`] to display the
//! crash overlay.  No watchdog or hang-detection logic lives here.

use js_sys::JSON;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::Event;

/// Register the Rust panic hook.
///
/// Writes the panic info to `localStorage["poly.lastPanic"]` so it survives
/// the page reload that `wasm-bindgen` performs after the hook returns, then
/// calls `report_crash` to display the overlay.
pub(super) fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()));
        let mut message = panic_info.to_string();
        if message.trim().is_empty() {
            message = crate::i18n::t("wasm-crash-generic-message");
        }

        // Write to localStorage so the info survives the page reload caused
        // by the subsequent `unreachable` opcode clearing the JS context.
        if let Some(window) = web_sys::window()
            && let Ok(Some(storage)) = window.local_storage()
        {
            let entry = format!(
                "PANIC|{}|{}",
                location.as_deref().unwrap_or("unknown"),
                message
            );
            drop(storage.set_item("poly.lastPanic", &entry));
        }

        super::overlay::report_crash("panic", &message, location.as_deref());
    }));
}

/// Listen for synchronous JS errors via `window.onerror`.
pub(super) fn install_window_error_listener() {
    let Some(window) = web_sys::window() else {
        return;
    };

    let closure = Closure::<dyn FnMut(Event)>::wrap(Box::new(|event: Event| {
        let message = event
            .dyn_ref::<web_sys::ErrorEvent>()
            .map(error_event_message)
            .filter(|msg| !msg.trim().is_empty())
            .unwrap_or_else(|| crate::i18n::t("wasm-crash-window-error-fallback"));
        let location = event
            .dyn_ref::<web_sys::ErrorEvent>()
            .and_then(error_event_location);
        super::overlay::report_crash("window-error", &message, location.as_deref());
    }));

    drop(window.add_event_listener_with_callback("error", closure.as_ref().unchecked_ref()));
    closure.forget();
}

/// Listen for unhandled promise rejections via `window.unhandledrejection`.
pub(super) fn install_unhandled_rejection_listener() {
    let Some(window) = web_sys::window() else {
        return;
    };

    let closure = Closure::<dyn FnMut(Event)>::wrap(Box::new(|event: Event| {
        let message = event
            .dyn_ref::<web_sys::PromiseRejectionEvent>()
            .map(|rejection| js_value_to_text(&rejection.reason()))
            .filter(|msg| !msg.trim().is_empty())
            .unwrap_or_else(|| crate::i18n::t("wasm-crash-rejection-fallback"));
        super::overlay::report_crash("unhandled-rejection", &message, None);
    }));

    drop(
        window.add_event_listener_with_callback(
            "unhandledrejection",
            closure.as_ref().unchecked_ref(),
        ),
    );
    closure.forget();
}

fn error_event_message(event: &web_sys::ErrorEvent) -> String {
    let message = event.message();
    if !message.trim().is_empty() {
        return message;
    }
    crate::i18n::t("wasm-crash-window-error-fallback")
}

fn error_event_location(event: &web_sys::ErrorEvent) -> Option<String> {
    let filename = event.filename();
    if filename.trim().is_empty() {
        return None;
    }
    Some(format!("{}:{}:{}", filename, event.lineno(), event.colno()))
}

fn js_value_to_text(value: &wasm_bindgen::JsValue) -> String {
    if let Some(text) = value.as_string() {
        return text;
    }

    JSON::stringify(value)
        .ok()
        .and_then(|text| text.as_string())
        .unwrap_or_else(|| crate::i18n::t("wasm-crash-generic-message"))
}
