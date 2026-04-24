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

use std::sync::Once;

use js_sys::{JSON, Object, Reflect};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{Document, Element, Event, Window};

const OVERLAY_ID: &str = "poly-wasm-crash-overlay";
const CRASH_STATE_KEY: &str = "__polyCrashState";

/// How long (ms) the boot watchdog waits before declaring a hang.
/// Normal boots complete in well under a second; 20 s is generous.
// Boot can take >20s with many restored accounts + favorited servers; 60s is
// a more realistic ceiling and still catches genuine boot hangs.
const BOOT_HANG_TIMEOUT_MS: u32 = 60_000;

/// How long (ms) the main thread may be unresponsive before the interaction
/// watchdog shows the crash overlay. 12 s fires fast enough to feel responsive
/// while still absorbing the Dog/Teams first-render pass. The overlay then
/// offers a visible "Reload" affordance + an auto-reload countdown so the
/// user never has to guess whether the tab is dead.
///
/// If the main thread is truly wedged (tight WASM loop that never yields),
/// the ServiceWorker-based force-reloader (registered separately from
/// `install_wasm_crash_handler`) kicks in at ~25 s and navigates the client
/// back to its current URL, bypassing the main thread entirely.
///
/// NOTE: The previous value of 30 s was too long — users thought the tab was
/// dead and closed it before the overlay ever showed. 12 s is late enough to
/// absorb Dog-account first-render (E5 regression threshold) without feeling
/// unresponsive.
const INTERACTION_HANG_TIMEOUT_MS: u32 = 12_000;

/// How long (ms) the overlay sits before auto-reloading if the user hasn't
/// clicked "Reload" yet. 15 s matches typical attention span after seeing a
/// modal — long enough to read, short enough to recover unattended.
const OVERLAY_AUTO_RELOAD_MS: u32 = 15_000;

/// Install the shared browser/WASM crash handler once for the current page.
pub fn install_wasm_crash_handler() {
    static INSTALLED: Once = Once::new();

    INSTALLED.call_once(|| {
        clear_previous_crash_state();
        install_panic_hook();
        install_window_error_listener();
        install_unhandled_rejection_listener();
        install_boot_hang_watchdog(BOOT_HANG_TIMEOUT_MS);
        install_interaction_hang_watchdog(INTERACTION_HANG_TIMEOUT_MS);
        install_service_worker_force_reloader();
    });
}

/// Register `/poly-service-worker.js` and start posting heartbeats to it.
///
/// The ServiceWorker runs on a separate thread from the page's main thread,
/// so it can observe when main-thread heartbeats stop. When a gap exceeds
/// its own timeout, it calls `client.navigate(client.url)` — a browser-level
/// navigation that bypasses the wedged main thread and force-reloads the tab.
/// This is the only reliable recovery path for truly infinite WASM loops.
fn install_service_worker_force_reloader() {
    // language=JavaScript
    let js = r#"(function() {
    if (!('serviceWorker' in navigator)) { return; }
    if (window.__polyServiceWorkerInstalled) { return; }
    window.__polyServiceWorkerInstalled = true;

    navigator.serviceWorker.register('/poly-service-worker.js', { scope: '/' })
        .then(function(reg) {
            function beat() {
                var sw = navigator.serviceWorker.controller || reg.active;
                if (sw) { try { sw.postMessage({ type: 'poly-heartbeat' }); } catch (e) {} }
            }
            beat();
            setInterval(beat, 500);
        })
        .catch(function(err) {
            console.warn('[poly] ServiceWorker registration failed — no hang auto-reload', err);
        });
})();"#;
    let _ = js_sys::eval(js);
}

/// Inject a JS heartbeat that detects post-boot main-thread deadlocks.
///
/// A Web Worker sends a `ping` every 500 ms. The main-thread listener
/// records `Date.now()` on each message. A second `setInterval` — also on
/// the main thread — checks the gap between *now* and the last heartbeat;
/// if the gap exceeds `timeout_ms`, the main thread processed no messages
/// in that window (definition of a hang) and we show the crash overlay.
///
/// The worker itself runs on a separate OS thread so it ticks independently
/// of main-thread load. The main-thread interval is what actually notices
/// the gap, which only fires when the main thread resumes — but then it
/// sees that `Date.now() - lastPing > timeout_ms` and shows the overlay
/// retroactively. That's fine for the user-visible case: either the thread
/// recovers (and we warn them the app was just unresponsive) or the thread
/// stays dead forever and the worker logs to console.
fn install_interaction_hang_watchdog(timeout_ms: u32) {
    // language=JavaScript
    let js = format!(
        r#"(function() {{
    if (window.__polyInteractionWatchdogInstalled) {{ return; }}
    window.__polyInteractionWatchdogInstalled = true;

    var TIMEOUT = {timeout};
    var AUTO_RELOAD_MS = {auto_reload};
    window.__polyLastHeartbeat = Date.now();

    // Reset heartbeat on visibility change (resume from suspend, tab
    // switch). Without this, Date.now() jumps across the suspend and
    // the watchdog sees a fake multi-hour "hang". This replaces the
    // previous MAX_REAL_HANG upper-bound filter, which also hid genuine
    // long hangs (>60s) from the user.
    document.addEventListener('visibilitychange', function() {{
        if (document.visibilityState === 'visible') {{
            window.__polyLastHeartbeat = Date.now();
        }}
    }});

    try {{
        var workerSrc = 'setInterval(function(){{postMessage(1)}}, 500);';
        var blob = new Blob([workerSrc], {{ type: 'application/javascript' }});
        var worker = new Worker(URL.createObjectURL(blob));
        worker.onmessage = function() {{
            var now = Date.now();
            var gap = now - window.__polyLastHeartbeat;
            window.__polyLastHeartbeat = now;
            if (gap > TIMEOUT) {{
                showHangOverlay(gap);
            }}
        }};
    }} catch (e) {{
        console.warn('Poly interaction watchdog: worker unavailable', e);
    }}

    setInterval(function() {{
        var gap = Date.now() - window.__polyLastHeartbeat;
        if (gap > TIMEOUT) {{
            showHangOverlay(gap);
        }}
    }}, 1000);

    function showHangOverlay(gapMs) {{
        // Don't double-show if the crash overlay is already visible
        // (e.g. from a Rust panic or another hang report).
        var OVERLAY_ID = 'poly-wasm-crash-overlay';
        if (document.getElementById(OVERLAY_ID)) {{ return; }}
        // Reset the heartbeat so we don't spam the overlay every tick
        // while the user is reading it.
        window.__polyLastHeartbeat = Date.now();

        var overlay = document.createElement('div');
        overlay.id = OVERLAY_ID;
        overlay.style.cssText = 'position:fixed;inset:0;z-index:2147483647;overflow:auto;padding:28px;background:rgba(10,12,16,0.96);color:#fff;font-family:Inter,system-ui,sans-serif;';
        var card = document.createElement('div');
        card.style.cssText = 'max-width:920px;margin:0 auto;background:#1a1f2b;border:1px solid rgba(255,255,255,0.14);border-radius:16px;padding:24px;box-shadow:0 16px 48px rgba(0,0,0,0.45);';
        var h1 = document.createElement('h1');
        h1.style.cssText = 'margin:0 0 8px 0;font-size:28px;line-height:1.2;';
        h1.textContent = 'App not responding';
        var p1 = document.createElement('p');
        p1.style.cssText = 'margin:0 0 12px 0;color:#d8dee9;font-size:15px;line-height:1.5;';
        p1.textContent = 'Poly\u2019s main thread was blocked for ' + Math.round(gapMs/1000) + ' seconds. This usually means an infinite render loop, a deadlocked Dioxus signal, or a missing async yield.';
        var p2 = document.createElement('p');
        p2.style.cssText = 'margin:0 0 18px 0;color:#8fbcff;font-size:14px;font-weight:600;';
        p2.textContent = 'Type: interaction-hang (' + gapMs + 'ms unresponsive)';
        var countdown = document.createElement('p');
        countdown.style.cssText = 'margin:0 0 18px 0;color:#d8dee9;font-size:13px;';
        var remaining = Math.round(AUTO_RELOAD_MS / 1000);
        countdown.textContent = 'Auto-reloading in ' + remaining + 's\u2026';
        var btn = document.createElement('button');
        btn.style.cssText = 'border:0;border-radius:10px;padding:12px 16px;background:#4f8cff;color:white;font-size:14px;font-weight:600;cursor:pointer;';
        btn.textContent = 'Reload now';
        btn.onclick = function() {{ window.location.reload(); }};
        card.appendChild(h1);
        card.appendChild(p1);
        card.appendChild(p2);
        card.appendChild(countdown);
        card.appendChild(btn);
        overlay.appendChild(card);
        document.body && document.body.appendChild(overlay);
        console.error('Poly interaction hang: main thread blocked for ' + gapMs + 'ms');

        var ticker = setInterval(function() {{
            remaining -= 1;
            if (remaining <= 0) {{
                clearInterval(ticker);
                window.location.reload();
            }} else {{
                countdown.textContent = 'Auto-reloading in ' + remaining + 's\u2026';
            }}
        }}, 1000);
    }}
}})();"#,
        timeout = timeout_ms,
        auto_reload = OVERLAY_AUTO_RELOAD_MS,
    );
    let _ = js_sys::eval(&js);
}

/// Inject a JS `setTimeout` that shows the crash overlay if the startup
/// overlay has not dismissed after `timeout_ms` milliseconds.
///
/// The check reads `data-poly-startup-phase` from `<html>`, which the App
/// component sets to `"revealed"` once the startup overlay hides.  If it is
/// still `"booting"` (or absent) when the timer fires, the app is stuck —
/// typically due to an infinite Dioxus render loop or a data-loading deadlock
/// that prevented the overlay from hiding.
fn install_boot_hang_watchdog(timeout_ms: u32) {
    // language=JavaScript
    let js = format!(
        r#"(function() {{
    var t = {timeout};
    window.__polyBootWatchdog = setTimeout(function() {{
        var phase = document.documentElement.getAttribute('data-poly-startup-phase');
        if (phase === 'revealed') {{ return; }}
        var OVERLAY_ID = 'poly-wasm-crash-overlay';
        if (document.getElementById(OVERLAY_ID)) {{ return; }}
        var overlay = document.createElement('div');
        overlay.id = OVERLAY_ID;
        overlay.style.cssText = 'position:fixed;inset:0;z-index:2147483647;overflow:auto;padding:28px;background:rgba(10,12,16,0.96);color:#fff;font-family:Inter,system-ui,sans-serif;';
        var card = document.createElement('div');
        card.style.cssText = 'max-width:920px;margin:0 auto;background:#1a1f2b;border:1px solid rgba(255,255,255,0.14);border-radius:16px;padding:24px;box-shadow:0 16px 48px rgba(0,0,0,0.45);';
        var h1 = document.createElement('h1');
        h1.style.cssText = 'margin:0 0 8px 0;font-size:28px;line-height:1.2;';
        h1.textContent = 'App not responding';
        var p1 = document.createElement('p');
        p1.style.cssText = 'margin:0 0 12px 0;color:#d8dee9;font-size:15px;line-height:1.5;';
        p1.textContent = 'Poly is stuck on the loading screen (\u201cbooting\u201d phase never completed). This usually means a render loop or missing data prevented the app from starting.';
        var p2 = document.createElement('p');
        p2.style.cssText = 'margin:0 0 18px 0;color:#8fbcff;font-size:14px;font-weight:600;';
        p2.textContent = 'Type: boot-hang (startup overlay not dismissed after ' + (t/1000) + 's)';
        var btn = document.createElement('button');
        btn.style.cssText = 'border:0;border-radius:10px;padding:12px 16px;background:#4f8cff;color:white;font-size:14px;font-weight:600;cursor:pointer;';
        btn.textContent = 'Reload';
        btn.onclick = function() {{ window.location.reload(); }};
        card.appendChild(h1);
        card.appendChild(p1);
        card.appendChild(p2);
        card.appendChild(btn);
        overlay.appendChild(card);
        document.body && document.body.appendChild(overlay);
        console.error('Poly boot hang: startup overlay still visible after ' + t + 'ms (phase=' + phase + ')');
    }}, t);
}})();"#,
        timeout = timeout_ms,
    );
    let _ = js_sys::eval(&js);
}

fn install_panic_hook() {
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
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let entry = format!(
                    "PANIC|{}|{}",
                    location.as_deref().unwrap_or("unknown"),
                    message
                );
                let _ = storage.set_item("poly.lastPanic", &entry);
            }
        }

        report_crash("panic", &message, location.as_deref());
    }));
}

fn install_window_error_listener() {
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
        report_crash("window-error", &message, location.as_deref());
    }));

    let _ = window.add_event_listener_with_callback("error", closure.as_ref().unchecked_ref());
    closure.forget();
}

fn install_unhandled_rejection_listener() {
    let Some(window) = web_sys::window() else {
        return;
    };

    let closure = Closure::<dyn FnMut(Event)>::wrap(Box::new(|event: Event| {
        let message = event
            .dyn_ref::<web_sys::PromiseRejectionEvent>()
            .map(|rejection| js_value_to_text(&rejection.reason()))
            .filter(|msg| !msg.trim().is_empty())
            .unwrap_or_else(|| crate::i18n::t("wasm-crash-rejection-fallback"));
        report_crash("unhandled-rejection", &message, None);
    }));

    let _ = window
        .add_event_listener_with_callback("unhandledrejection", closure.as_ref().unchecked_ref());
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

fn clear_previous_crash_state() {
    let Some(window) = web_sys::window() else {
        return;
    };

    let _ = Reflect::delete_property(&window, &wasm_bindgen::JsValue::from_str(CRASH_STATE_KEY));

    let Some(document) = window.document() else {
        return;
    };

    if let Some(existing) = document.get_element_by_id(OVERLAY_ID)
        && let Some(body) = document.body()
    {
        let _ = body.remove_child(&existing);
    }
}

fn report_crash(kind: &str, message: &str, location: Option<&str>) {
    // Never overwrite a Rust panic with a subsequent window-error/rejection.
    // The panic hook fires first and carries the real source location;
    // window.onerror fires immediately after (triggered by the `unreachable`
    // opcode wasm-bindgen executes after the hook) and would clobber it.
    if kind != "panic" {
        if let Some(window) = web_sys::window() {
            if let Ok(state) =
                Reflect::get(&window, &wasm_bindgen::JsValue::from_str(CRASH_STATE_KEY))
            {
                if !state.is_undefined() && !state.is_null() {
                    if let Ok(existing_kind) =
                        Reflect::get(&state, &wasm_bindgen::JsValue::from_str("kind"))
                    {
                        if existing_kind.as_string().as_deref() == Some("panic") {
                            return;
                        }
                    }
                }
            }
        }
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
    let _ = card.set_attribute(
        "style",
        "max-width: 920px; margin: 0 auto; background: #1a1f2b; border: 1px solid rgba(255,255,255,0.14); border-radius: 16px; padding: 24px; box-shadow: 0 16px 48px rgba(0,0,0,0.45);",
    );

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
        let _ = button.set_attribute(
            "style",
            "border: 0; border-radius: 10px; padding: 12px 16px; background: #4f8cff; color: white; font-size: 14px; font-weight: 600; cursor: pointer;",
        );
        button.set_text_content(Some(&reload_label));
        // Use a plain JS inline onclick so the button works even after WASM
        // has terminated (e.g. after a Rust panic). A wasm-bindgen Closure
        // requires the WASM module to still be alive to dispatch the click,
        // which is never true after the panic hook has run.
        let _ = button.set_attribute("onclick", "window.location.reload()");
        let _ = card.append_child(&button);
    }

    let _ = overlay.append_child(&card);
}

fn store_crash_state(
    window: &Window,
    kind: &str,
    kind_text: &str,
    message: &str,
    location: Option<&str>,
) {
    let state = Object::new();
    let _ = Reflect::set(
        &state,
        &wasm_bindgen::JsValue::from_str("kind"),
        &wasm_bindgen::JsValue::from_str(kind),
    );
    let _ = Reflect::set(
        &state,
        &wasm_bindgen::JsValue::from_str("kindLabel"),
        &wasm_bindgen::JsValue::from_str(kind_text),
    );
    let _ = Reflect::set(
        &state,
        &wasm_bindgen::JsValue::from_str("message"),
        &wasm_bindgen::JsValue::from_str(message),
    );
    let _ = Reflect::set(
        &state,
        &wasm_bindgen::JsValue::from_str("locale"),
        &wasm_bindgen::JsValue::from_str(&crate::i18n::current_locale()),
    );
    let _ = Reflect::set(
        &state,
        &wasm_bindgen::JsValue::from_str("path"),
        &wasm_bindgen::JsValue::from_str(&window.location().pathname().unwrap_or_default()),
    );
    if let Some(location) = location {
        let _ = Reflect::set(
            &state,
            &wasm_bindgen::JsValue::from_str("location"),
            &wasm_bindgen::JsValue::from_str(location),
        );
    }
    let _ = Reflect::set(
        window,
        &wasm_bindgen::JsValue::from_str(CRASH_STATE_KEY),
        &state,
    );
}

fn ensure_overlay(document: &Document, body: &web_sys::HtmlElement) -> Element {
    if let Some(existing) = document.get_element_by_id(OVERLAY_ID) {
        return existing;
    }

    let Some(overlay) = element(document, "div") else {
        return body.clone().unchecked_into::<Element>();
    };
    overlay.set_id(OVERLAY_ID);
    let _ = overlay.set_attribute(
        "style",
        "position: fixed; inset: 0; z-index: 2147483647; overflow: auto; padding: 28px; background: rgba(10, 12, 16, 0.96); color: #fff; font-family: Inter, system-ui, sans-serif;",
    );
    let _ = body.append_child(&overlay);
    overlay
}

fn clear_children(node: &Element) {
    while let Some(child) = node.first_child() {
        let _ = node.remove_child(&child);
    }
}

fn append_text_block(document: &Document, parent: &Element, tag: &str, text: &str, style: &str) {
    let Some(child) = element(document, tag) else {
        return;
    };
    let _ = child.set_attribute("style", style);
    child.set_text_content(Some(text));
    let _ = parent.append_child(&child);
}

fn element(document: &Document, tag: &str) -> Option<Element> {
    document.create_element(tag).ok()
}
