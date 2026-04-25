//! Bisect-log helper — fire-and-forget KV write so the orchestrator can read
//! the ordered trace from SQLite even when CDP is wedged.
//!
//! cfg-gated: only compiled for wasm32 targets; a no-op stub is provided for
//! native builds so call sites compile everywhere.

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
        let init = web_sys::RequestInit::new();
        init.set_method("POST");
        init.set_body(&wasm_bindgen::JsValue::from_str(&body));
        if let Ok(headers) = web_sys::Headers::new() {
            let _ = headers.set("content-type", "application/json");
            init.set_headers(&headers);
        }
        if let Ok(req) = web_sys::Request::new_with_str_and_init("/host/kv/set", &init) {
            let _ = window.fetch_with_request(&req);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn bisect_log(_msg: &str) {}
