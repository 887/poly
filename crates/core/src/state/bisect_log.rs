//! Bisect-log helper — fire-and-forget KV write so the orchestrator can read
//! the ordered trace from SQLite even when CDP is wedged.
//!
//! cfg-gated: only compiled for wasm32 targets; a no-op stub is provided for
//! native builds so call sites compile everywhere.
//!
//! # Why this is a raw `fetch`, not a `poly_host_bridge::Client` call
//!
//! This is the last-resort diagnostic path documented in `CLAUDE.md`. It has to
//! keep working when the WASM main thread is about to wedge in a tight loop,
//! which means the request must be **dispatched to the network thread before JS
//! continues** — `window.fetch(…)` does exactly that, whereas anything that
//! `.await`s (or resolves a promise in a `.then`) never runs once the main
//! thread stops servicing the microtask queue.
//!
//! # Authorising the write without giving that property up
//!
//! `/host/*` requires `Authorization: Bearer <session token>`. The token is
//! learned from `GET /host/session`, which is inherently asynchronous, so this
//! module keeps a process-static copy and never waits on it:
//!
//! * **Token known** — the header goes on the request and the fetch is
//!   dispatched synchronously, exactly as before. This is the steady state for
//!   every call after the first.
//! * **Token not yet known** — the request is dispatched anyway, without the
//!   header. It may come back `401`; nothing observes the response, so that
//!   degrades to "this one line was not recorded" rather than a panic, a block,
//!   or a lost trace. In parallel a one-shot background bootstrap is kicked off
//!   so every *later* call carries the header.
//!
//! The bootstrap deliberately does **not** go through
//! `poly_host_bridge::host_auth`: that client stack is frequently the thing
//! under suspicion when this sink is in use, and the two facts it needs (the
//! route name and the `token` field) are cheap to spell out here. A shell that
//! predates bearer auth simply answers `404`, the token stays `None`, and the
//! writes go out unauthenticated exactly as they always did.

/// Route that mints the shell's session token. Spelled literally rather than
/// imported so this sink has no dependency on the bridge client — see the
/// module docs.
#[cfg(target_arch = "wasm32")]
const SESSION_ROUTE: &str = "/host/session";

/// Process-static copy of the shell session token.
#[cfg(target_arch = "wasm32")]
fn token_slot() -> &'static std::sync::Mutex<Option<String>> {
    static SLOT: std::sync::OnceLock<std::sync::Mutex<Option<String>>> =
        std::sync::OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(None))
}

/// The token, if a previous bootstrap already landed.
///
/// Never blocks and never panics: a poisoned lock reports "unknown", and the
/// caller then sends the write unauthenticated rather than dying inside a
/// diagnostic helper.
#[cfg(target_arch = "wasm32")]
fn cached_token() -> Option<String> {
    token_slot().lock().ok().and_then(|slot| slot.clone())
}

/// Kick off the one-and-only `GET /host/session` bootstrap.
///
/// Returns immediately; the reply is picked up later on the microtask queue.
/// Every failure path (route absent, non-2xx, unparseable body, empty token) is
/// silent by design — this is a diagnostic sink and must never become a second
/// source of breakage.
#[cfg(target_arch = "wasm32")]
fn bootstrap_token(window: &web_sys::Window) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use wasm_bindgen::JsCast as _;

    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    let promise = window.fetch_with_str(SESSION_ROUTE);
    wasm_bindgen_futures::spawn_local(async move {
        let Ok(value) = wasm_bindgen_futures::JsFuture::from(promise).await else {
            return;
        };
        let Ok(resp) = value.dyn_into::<web_sys::Response>() else {
            return;
        };
        if !resp.ok() {
            return;
        }
        let Ok(text_promise) = resp.text() else {
            return;
        };
        let Ok(text) = wasm_bindgen_futures::JsFuture::from(text_promise).await else {
            return;
        };
        let Some(text) = text.as_string() else {
            return;
        };
        let Ok(body) = serde_json::from_str::<serde_json::Value>(&text) else {
            return;
        };
        let token = body
            .get("token")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if token.is_empty() {
            return;
        }
        if let Ok(mut slot) = token_slot().lock() {
            *slot = Some(token.to_string());
        }
    });
}

#[cfg(target_arch = "wasm32")]
pub fn bisect_log(msg: &str) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    let n = C.fetch_add(1, Ordering::SeqCst);
    if let Some(window) = web_sys::window() {
        if let Some(doc) = window.document() {
            doc.set_title(&format!("BISECT#{n}: {msg}"));
        }
        let body = format!(
            r#"{{"key":"bisect:{n:08}","value":{}}}"#,
            serde_json::to_string(msg).unwrap_or_default()
        );
        // Read the cached token BEFORE building the request: everything from
        // here to `fetch_with_request` must stay in one synchronous run of this
        // function, so nothing below may await.
        let token = cached_token();
        if token.is_none() {
            bootstrap_token(&window);
        }
        let init = web_sys::RequestInit::new();
        init.set_method("POST");
        init.set_body(&wasm_bindgen::JsValue::from_str(&body));
        if let Ok(headers) = web_sys::Headers::new() {
            drop(headers.set("content-type", "application/json"));
            if let Some(ref token) = token {
                drop(headers.set("authorization", &format!("Bearer {token}")));
            }

            init.set_headers(&headers);
        }
        if let Ok(req) = web_sys::Request::new_with_str_and_init("/host/kv/set", &init) {
            drop(window.fetch_with_request(&req));

        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn bisect_log(_msg: &str) {}
