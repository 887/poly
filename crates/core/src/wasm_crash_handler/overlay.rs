//! Crash-overlay DOM rendering.
//!
//! Owns the "App crashed" / "App not responding" overlay that gets injected
//! into the page when a Rust panic, JS error, or hang is detected.
//!
//! All functions in this module operate on the browser DOM via `web_sys`
//! and are responsible only for *rendering* — they do not install any
//! listeners or watchdogs.

use js_sys::{Object, Reflect};
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, Window};

pub(super) const OVERLAY_ID: &str = "poly-wasm-crash-overlay";
pub(super) const CRASH_STATE_KEY: &str = "__polyCrashState";

/// Remove any crash overlay and JS crash-state from a previous page load.
///
/// Called during handler installation so a stale overlay from a previous
/// navigate-away doesn't confuse the new boot.
pub(super) fn clear_previous_crash_state() {
    let Some(window) = web_sys::window() else {
        return;
    };

    drop(Reflect::delete_property(
        &window,
        &wasm_bindgen::JsValue::from_str(CRASH_STATE_KEY),
    ));

    let Some(document) = window.document() else {
        return;
    };

    if let Some(existing) = document.get_element_by_id(OVERLAY_ID)
        && let Some(body) = document.body()
    {
        drop(body.remove_child(&existing));
    }
}

/// Show the crash overlay with a title, message, and location.
///
/// Idempotent: if an overlay is already present it is cleared and reused.
/// The "panic" kind takes precedence over subsequent window-error/rejection
/// reports so the real source location is never clobbered.
pub(super) fn report_crash(kind: &str, message: &str, location: Option<&str>) {
    // Never overwrite a Rust panic with a subsequent window-error/rejection.
    // The panic hook fires first and carries the real source location;
    // window.onerror fires immediately after (triggered by the `unreachable`
    // opcode wasm-bindgen executes after the hook) and would clobber it.
    if kind != "panic"
        && let Some(window) = web_sys::window()
        && let Ok(state) =
            Reflect::get(&window, &wasm_bindgen::JsValue::from_str(CRASH_STATE_KEY))
        && !state.is_undefined()
        && !state.is_null()
        && let Ok(existing_kind) =
            Reflect::get(&state, &wasm_bindgen::JsValue::from_str("kind"))
        && existing_kind.as_string().as_deref() == Some("panic")
    {
        return;
    }

    let title = crate::i18n::t("wasm-crash-title");
    let description = crate::i18n::t("wasm-crash-description");
    let details_label = crate::i18n::t("wasm-crash-details-label");
    let location_label = crate::i18n::t("wasm-crash-location-label");
    let reload_label = crate::i18n::t("wasm-crash-reload-action");
    let path_label = crate::i18n::t("wasm-crash-path-label");
    let kind_text = match kind {
        "panic" => crate::i18n::t("wasm-crash-kind-panic"),
        "window-error" => crate::i18n::t("wasm-crash-kind-window-error"),
        "unhandled-rejection" => crate::i18n::t("wasm-crash-kind-unhandled-rejection"),
        _ => crate::i18n::t("wasm-crash-kind-unknown"),
    };

    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };

    store_crash_state(&window, kind, &kind_text, message, location);

    web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(&format!(
        "Poly WASM crash ({kind}): {message}"
    )));

    let Some(body) = document.body() else {
        return;
    };

    let overlay = ensure_overlay(&document, &body);
    clear_children(&overlay);

    let Some(card) = element(&document, "div") else {
        return;
    };
    drop(card.set_attribute(
        "style",
        "max-width: 920px; margin: 0 auto; background: #1a1f2b; border: 1px solid rgba(255,255,255,0.14); border-radius: 16px; padding: 24px; box-shadow: 0 16px 48px rgba(0,0,0,0.45);",
    ));

    append_text_block(
        &document,
        &card,
        "h1",
        &title,
        "margin: 0 0 8px 0; font-size: 28px; line-height: 1.2;",
    );
    append_text_block(
        &document,
        &card,
        "p",
        &description,
        "margin: 0 0 18px 0; color: #d8dee9; font-size: 15px; line-height: 1.5;",
    );
    append_text_block(
        &document,
        &card,
        "p",
        &format!("{details_label}: {kind_text}"),
        "margin: 0 0 10px 0; font-size: 14px; color: #8fbcff; font-weight: 600;",
    );
    append_text_block(
        &document,
        &card,
        "pre",
        message,
        "white-space: pre-wrap; word-break: break-word; margin: 0 0 16px 0; padding: 14px; background: rgba(0,0,0,0.3); border-radius: 10px; font-size: 13px; line-height: 1.45;",
    );

    let pathname = window.location().pathname().unwrap_or_default();
    append_text_block(
        &document,
        &card,
        "p",
        &format!("{path_label}: {pathname}"),
        "margin: 0 0 10px 0; font-size: 13px; color: #d8dee9;",
    );

    if let Some(location) = location {
        append_text_block(
            &document,
            &card,
            "p",
            &format!("{location_label}: {location}"),
            "margin: 0 0 16px 0; font-size: 13px; color: #d8dee9;",
        );
    }

    if let Some(button) = element(&document, "button") {
        drop(button.set_attribute(
            "style",
            "border: 0; border-radius: 10px; padding: 12px 16px; background: #4f8cff; color: white; font-size: 14px; font-weight: 600; cursor: pointer;",
        ));
        button.set_text_content(Some(&reload_label));
        // Use a plain JS inline onclick so the button works even after WASM
        // has terminated (e.g. after a Rust panic). A wasm-bindgen Closure
        // requires the WASM module to still be alive to dispatch the click,
        // which is never true after the panic hook has run.
        drop(button.set_attribute("onclick", "window.location.reload()"));
        drop(card.append_child(&button));
    }

    drop(overlay.append_child(&card));
}

fn store_crash_state(
    window: &Window,
    kind: &str,
    kind_text: &str,
    message: &str,
    location: Option<&str>,
) {
    let state = Object::new();
    drop(Reflect::set(
        &state,
        &wasm_bindgen::JsValue::from_str("kind"),
        &wasm_bindgen::JsValue::from_str(kind),
    ));
    drop(Reflect::set(
        &state,
        &wasm_bindgen::JsValue::from_str("kindLabel"),
        &wasm_bindgen::JsValue::from_str(kind_text),
    ));
    drop(Reflect::set(
        &state,
        &wasm_bindgen::JsValue::from_str("message"),
        &wasm_bindgen::JsValue::from_str(message),
    ));
    drop(Reflect::set(
        &state,
        &wasm_bindgen::JsValue::from_str("locale"),
        &wasm_bindgen::JsValue::from_str(&crate::i18n::current_locale()),
    ));
    drop(Reflect::set(
        &state,
        &wasm_bindgen::JsValue::from_str("path"),
        &wasm_bindgen::JsValue::from_str(&window.location().pathname().unwrap_or_default()),
    ));
    if let Some(location) = location {
        drop(Reflect::set(
            &state,
            &wasm_bindgen::JsValue::from_str("location"),
            &wasm_bindgen::JsValue::from_str(location),
        ));
    }
    drop(Reflect::set(
        window,
        &wasm_bindgen::JsValue::from_str(CRASH_STATE_KEY),
        &state,
    ));
}

fn ensure_overlay(document: &Document, body: &web_sys::HtmlElement) -> Element {
    if let Some(existing) = document.get_element_by_id(OVERLAY_ID) {
        return existing;
    }

    let Some(overlay) = element(document, "div") else {
        return body.clone().unchecked_into::<Element>();
    };
    overlay.set_id(OVERLAY_ID);
    drop(overlay.set_attribute(
        "style",
        "position: fixed; inset: 0; z-index: 2147483647; overflow: auto; padding: 28px; background: rgba(10, 12, 16, 0.96); color: #fff; font-family: Inter, system-ui, sans-serif;",
    ));
    drop(body.append_child(&overlay));

    overlay
}

fn clear_children(node: &Element) {
    while let Some(child) = node.first_child() {
        drop(node.remove_child(&child));
    }
}

fn append_text_block(document: &Document, parent: &Element, tag: &str, text: &str, style: &str) {
    let Some(child) = element(document, tag) else {
        return;
    };
    drop(child.set_attribute("style", style));
    child.set_text_content(Some(text));
    drop(parent.append_child(&child));
}

fn element(document: &Document, tag: &str) -> Option<Element> {
    document.create_element(tag).ok()
}
