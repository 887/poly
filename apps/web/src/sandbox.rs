//! WebSandbox — browser-popup sandbox implementation for `apps/web`.
//!
//! # How it works
//!
//! 1. `open_browser_sandbox(url, capture_pattern)` generates a random
//!    `sandbox_id`, opens the OAuth URL in a popup window, and installs a
//!    `window.addEventListener('message', …)` listener that waits for the
//!    redirect shim at `/sandbox/<id>` to post back the captured URL.
//!
//! 2. The redirect shim (`GET /sandbox/<id>`) is served by the fullstack
//!    axum server (see `apps/poly-host/src/lib.rs`). It is the page that the
//!    OAuth provider redirects to — it runs `window.opener.postMessage(…)`
//!    and closes itself.
//!
//! 3. A `setInterval` polls `popup.closed` every 500 ms. If the user closes
//!    the popup before the capture fires, the future resolves with
//!    `SandboxError::UserCancelled`.
//!
//! # Constraint (C.4)
//!
//! The OAuth provider MUST be configured to redirect to
//! `<host-origin>/sandbox/<id>` (the same origin that served the WASM app).
//! This is required because `postMessage` uses `location.origin` as the
//! target, preventing cross-origin message leaks. Backends that hardcode
//! their own callback URL without the shim path will NOT work on the web
//! shell — they must be configured to use the shim.
//!
//! See `docs/plans/plan-host-sandbox-impl.md` Phase C for the full design.

/// On the web shell, `WebSandbox` implements [`poly_host_sandbox::HostSandbox`]
/// by opening a browser popup and awaiting a `postMessage` from the redirect
/// shim at `/sandbox/<id>` (served by `apps/poly-host`). Exported so shell
/// bootstrap code can name the type explicitly when wiring the host-cap
/// registry (C.5).
#[cfg(target_arch = "wasm32")]
pub use wasm::WebSandbox;

#[cfg(target_arch = "wasm32")]
mod wasm {
    use js_sys::Promise;
    use poly_host_sandbox::{HostSandbox, SandboxError, SandboxResult};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;

    /// Web-shell sandbox implementation. Compiles only for `wasm32-unknown-unknown`.
    pub struct WebSandbox;

    #[async_trait::async_trait(?Send)]
    impl HostSandbox for WebSandbox {
        /// Open a browser popup at `url` and resolve when the redirect shim
        /// at `/sandbox/<id>` posts the captured URL back.
        ///
        /// `capture_url_pattern` is currently unused on the web variant —
        /// the shim always captures the full redirect URL (it only fires when
        /// the OAuth provider redirects to `<origin>/sandbox/<id>`). Future
        /// versions may filter by pattern in the WASM listener.
        async fn open_browser_sandbox(
            &self,
            url: String,
            _capture_url_pattern: String,
        ) -> Result<SandboxResult, SandboxError> {
            // Generate a random sandbox id (hex-encoded 8 bytes).
            // We use Math.random() via js_sys since getrandom may not be wired
            // on all WASM builds; this is a nonce, not a secret.
            let sandbox_id = sandbox_id_js();

            // Build the JS promise that drives the whole flow:
            //   - opens a popup,
            //   - installs a postMessage listener filtered by (type, id, origin),
            //   - installs a setInterval cancel-detector.
            // The promise is written inline as a JS string evaluated with
            // js_sys::eval so we don't have to add a forest of web-sys feature
            // flags just for this single feature. The JS code is safe — it only
            // reads `window.location.origin`, calls `window.open`, and
            // `window.addEventListener`; it does not eval user-supplied strings.
            let js_code = format!(
                r#"
new Promise(function(resolve, reject) {{
  var sandboxId = {id_json};
  var origin = window.location.origin;
  var shimUrl = origin + '/sandbox/' + sandboxId;

  // We open the OAuth URL directly. The provider must be configured to
  // redirect the user back to shimUrl (= origin + /sandbox/<id>).
  // Note: we pass `url` here, not `shimUrl` — the caller supplies the
  // full OAuth entry URL with the redirect_uri embedded.
  var popup = window.open({url_json}, '_blank', 'popup,width=600,height=800,noopener=0');

  if (!popup) {{
    reject(new Error('PopupBlocked'));
    return;
  }}

  var done = false;

  function cleanup() {{
    window.removeEventListener('message', onMessage);
    clearInterval(cancelTimer);
  }}

  function onMessage(event) {{
    // Only accept same-origin messages tagged for our sandbox id.
    if (event.origin !== origin) return;
    if (!event.data || event.data.type !== 'sandbox-captured') return;
    if (event.data.id !== sandboxId) return;
    if (done) return;
    done = true;
    cleanup();
    try {{ popup.close(); }} catch(_) {{}}
    resolve(event.data.url);
  }}

  // Cancel path: if the user closes the popup before the capture fires.
  var cancelTimer = setInterval(function() {{
    if (!popup || popup.closed) {{
      if (done) {{ clearInterval(cancelTimer); return; }}
      done = true;
      cleanup();
      reject(new Error('UserCancelled'));
    }}
  }}, 500);

  window.addEventListener('message', onMessage);
}})
"#,
                id_json = js_sys::JSON::stringify(&JsValue::from_str(&sandbox_id))
                    .map(|s| s.as_string().unwrap_or_else(|| format!("\"{sandbox_id}\"")))
                    .unwrap_or_else(|_| format!("\"{sandbox_id}\"")),
                url_json = js_sys::JSON::stringify(&JsValue::from_str(&url))
                    .map(|s| s.as_string().unwrap_or_else(|| format!("\"{url}\"")))
                    .unwrap_or_else(|_| format!("\"{url}\"")),
            );

            let promise: Promise = js_sys::eval(&js_code)
                .map_err(|e| {
                    SandboxError::InvalidUrl(format!(
                        "eval failed: {}",
                        e.as_string().unwrap_or_else(|| "js error".to_string())
                    ))
                })?
                .dyn_into::<Promise>()
                .map_err(|_| SandboxError::InvalidUrl("eval did not return a Promise".into()))?;

            let result = JsFuture::from(promise).await.map_err(|e| {
                let msg = e.as_string().unwrap_or_else(|| "unknown".to_string());
                if msg.contains("UserCancelled") {
                    SandboxError::UserCancelled
                } else if msg.contains("PopupBlocked") {
                    SandboxError::InvalidUrl("popup was blocked by the browser".into())
                } else {
                    SandboxError::InvalidUrl(msg)
                }
            })?;

            let captured_url = result
                .as_string()
                .ok_or_else(|| SandboxError::InvalidUrl("captured value was not a string".into()))?;

            Ok(SandboxResult { captured_url })
        }
    }

    /// Generate a random sandbox id using `Math.random()` (nonce, not a secret).
    fn sandbox_id_js() -> String {
        // 64-bit random → 16 hex chars. Sufficient for disambiguation.
        let lo = (js_sys::Math::random() * (u32::MAX as f64)) as u32;
        let hi = (js_sys::Math::random() * (u32::MAX as f64)) as u32;
        format!("{lo:08x}{hi:08x}")
    }
}
