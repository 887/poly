//! Poly Desktop — DevTools build.
//!
//! Identical to the normal desktop app but with an embedded HTTP eval-bridge
//! server on port 9223 that `poly-desktop-devtools-mcp` uses to:
//!   - Evaluate JavaScript in the webview (`POST /eval`)
//!   - Capture a screenshot of the rendered page (`GET /screenshot`)
//!   - Inspect the DOM (`GET /dom`)
//!   - Read console output (`GET /console`)
//!
//! WebKit2GTK's inspector protocol is NOT Chrome CDP, so we bridge through
//! dioxus's built-in `eval()` instead.

use dioxus::document::eval;
use dioxus::prelude::*;
use poly_core::ui::App;
use std::sync::OnceLock;
use tokio::sync::{Mutex, mpsc, oneshot};

// ─── Eval Bridge ──────────────────────────────────────────────────────────────

struct EvalRequest {
    js: String,
    resp: oneshot::Sender<Result<String, String>>,
}

/// Sender half — held globally so HTTP handlers can reach the dioxus eval loop.
static EVAL_TX: OnceLock<mpsc::Sender<EvalRequest>> = OnceLock::new();
/// Receiver half — transferred into the dioxus coroutine exactly once.
static EVAL_RX: OnceLock<Mutex<Option<mpsc::Receiver<EvalRequest>>>> = OnceLock::new();

// ─── WebKit Screenshot Bridge ─────────────────────────────────────────────────

/// Request to capture the WebKit webview content as PNG.
struct ScreenshotRequest {
    resp: oneshot::Sender<Result<Vec<u8>, String>>,
}

static SCREENSHOT_TX: OnceLock<mpsc::Sender<ScreenshotRequest>> = OnceLock::new();
static SCREENSHOT_RX: OnceLock<Mutex<Option<mpsc::Receiver<ScreenshotRequest>>>> = OnceLock::new();

/// Evaluate JS in the webview from any tokio context.
///
/// Dioxus wraps the script in `async function(dioxus) { SCRIPT }` and the
/// **return value** of that function is the result — there is no REPL mode.
///
/// This helper:
/// - Passes scripts already starting with `return ` unchanged.
/// - Wraps bare single expressions with `return (expr)`.
/// - Wraps multi-statement scripts (containing `;`) in an IIFE so that the last
///   `return` inside the IIFE propagates outward:
///   `return (function(){ SCRIPT })()` — callers must include their own `return`
///   for the desired value, or end a single-expression IIFE naturally.
///
/// Shorthand: for multi-statement scripts, either:
///   a) Start with `return ` + a self-contained expression, OR
///   b) Write `return (function(){ stmt1; stmt2; return value; })()`
async fn do_eval(js: impl Into<String>) -> Result<String, String> {
    let expr = js.into();
    let trimmed = expr.trim();

    let script = if trimmed.starts_with("return ") {
        // Caller manages their own return — pass through unchanged.
        expr
    } else {
        // Strip trailing semicolons/whitespace for wrapping.
        let stripped = trimmed.trim_end_matches(|c: char| c == ';' || c.is_whitespace());

        // If the expression looks like a single expression (no bare top-level `;`
        // outside of strings/parens), wrap with `return (expr)`.
        // Otherwise wrap in an IIFE. We use a heuristic: look for `;` outside
        // of string literals in the stripped expression.
        if has_top_level_semicolon(stripped) {
            // Multi-statement: wrap in IIFE. Append `return null;` as a guaranteed
            // non-undefined fallback — any explicit `return` earlier will short-circuit.
            // This prevents EvalError::Communication caused by `undefined` returns.
            format!("return (function(){{\n{stripped}\n; return null; }})()")
        } else {
            // Single expression: use nullish coalescing so `undefined` becomes `null`.
            format!("return (({stripped}) ?? null)")
        }
    };

    let tx = EVAL_TX
        .get()
        .ok_or_else(|| "Eval bridge not yet initialised".to_string())?;
    let (resp_tx, resp_rx) = oneshot::channel();
    tx.send(EvalRequest {
        js: script,
        resp: resp_tx,
    })
    .await
    .map_err(|e| e.to_string())?;
    resp_rx.await.map_err(|e| e.to_string())?
}

/// Heuristic: does the string contain a `;` outside of string literals?
/// Handles single and double quoted strings (no template literals for simplicity).
fn has_top_level_semicolon(s: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut prev = '\0';
    for c in s.chars() {
        match c {
            '\'' if !in_double && prev != '\\' => in_single = !in_single,
            '"' if !in_single && prev != '\\' => in_double = !in_double,
            ';' if !in_single && !in_double => return true,
            _ => {}
        }
        prev = c;
    }
    false
}

// ─── Injected JS ──────────────────────────────────────────────────────────────

/// Injected into <head> on startup.
/// • Intercepts console.* to buffer the last 200 messages in window.__polyLogs
/// Intercepts console.* to buffer messages in window.__polyLogs.
const DEVTOOLS_HEAD: &str = r#"<script>
/* poly-devtools initialisation */
(function () {
  window.__polyLogs = [];
  ['log','warn','error','info','debug'].forEach(function(lvl) {
    var orig = console[lvl];
    console[lvl] = function() {
      var args = Array.prototype.slice.call(arguments);
      window.__polyLogs.push({
        level: lvl,
        message: args.map(function(a){return typeof a==='object'?JSON.stringify(a):String(a);}).join(' '),
        ts: Date.now()
      });
      if (window.__polyLogs.length > 200) window.__polyLogs.shift();
      orig.apply(console, args);
    };
  });
}());
</script>"#;

// ─── Entry Point ──────────────────────────────────────────────────────────────

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,poly_core=debug")),
        )
        .init();

    tracing::info!("Starting Poly Desktop — DevTools build");
    tracing::info!("DevTools HTTP server will be available at http://127.0.0.1:9223");

    // Initialise the eval bridge channels before dioxus starts.
    let (tx, rx) = mpsc::channel::<EvalRequest>(64);
    if EVAL_TX.set(tx).is_err() {
        tracing::error!("EVAL_TX already initialized — duplicate init?");
    }
    if EVAL_RX.set(Mutex::new(Some(rx))).is_err() {
        tracing::error!("EVAL_RX already initialized — duplicate init?");
    }

    // Initialise the screenshot bridge channels.
    let (ss_tx, ss_rx) = mpsc::channel::<ScreenshotRequest>(4);
    if SCREENSHOT_TX.set(ss_tx).is_err() {
        tracing::error!("SCREENSHOT_TX already initialized — duplicate init?");
    }
    if SCREENSHOT_RX.set(Mutex::new(Some(ss_rx))).is_err() {
        tracing::error!("SCREENSHOT_RX already initialized — duplicate init?");
    }

    poly_core::i18n::init();
    poly_core::theme::init();

    dioxus::LaunchBuilder::new()
        .with_cfg(
            dioxus::desktop::Config::default()
                .with_custom_head(DEVTOOLS_HEAD.to_string())
                .with_window(
                    dioxus::desktop::WindowBuilder::new()
                        .with_title("Poly — DevTools Build")
                        .with_inner_size(dioxus::desktop::LogicalSize::new(1440.0_f64, 900.0_f64)),
                ),
        )
        .launch(DevtoolsShell);
}

// ─── Dioxus Wrapper Component ─────────────────────────────────────────────────

#[component]
fn DevtoolsShell() -> Element {
    // Coroutine: drives JS eval requests using dioxus's built-in eval().
    use_coroutine(move |_: UnboundedReceiver<()>| async move {
        let Some(eval_rx_mutex) = EVAL_RX.get() else {
            tracing::error!("EVAL_RX not initialized");
            return;
        };
        let Some(mut rx) = eval_rx_mutex.lock().await.take() else {
            tracing::error!("EVAL_RX receiver already consumed");
            return;
        };

        while let Some(req) = rx.recv().await {
            let result: Result<serde_json::Value, _> = eval(&req.js).await;
            let out = match result {
                Ok(v) => Ok(match v {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                }),
                Err(e) => Err(e.to_string()),
            };
            let _ = req.resp.send(out);
        }
    });

    // Coroutine: captures WebKit content via webkit2gtk's native snapshot API.
    // Runs on the GTK main thread (all dioxus-desktop coroutines do), which is
    // required for calling GLib/GDK/WebKit functions.
    use_coroutine(move |_: UnboundedReceiver<()>| async move {
        let Some(screenshot_rx_mutex) = SCREENSHOT_RX.get() else {
            tracing::error!("SCREENSHOT_RX not initialized");
            return;
        };
        let Some(mut rx) = screenshot_rx_mutex.lock().await.take() else {
            tracing::error!("SCREENSHOT_RX receiver already consumed");
            return;
        };

        // Grab the webkit2gtk WebView once — it's a GObject clone (cheap).
        let wv = {
            use wry::WebViewExtUnix as _;
            dioxus::desktop::window().webview.webview()
        };

        while let Some(req) = rx.recv().await {
            use std::sync::mpsc;
            use webkit2gtk::WebViewExt as _;

            let (tx, poll_rx) = mpsc::channel::<Result<Vec<u8>, String>>();

            wv.snapshot(
                webkit2gtk::SnapshotRegion::FullDocument,
                webkit2gtk::SnapshotOptions::empty(),
                webkit2gtk::gio::Cancellable::NONE,
                move |result: Result<cairo::Surface, webkit2gtk::glib::Error>| match result {
                    Ok(surface) => {
                        let mut buf: Vec<u8> = Vec::new();
                        match surface.write_to_png(&mut buf) {
                            Ok(_) => {
                                let _ = tx.send(Ok(buf));
                            }
                            Err(e) => {
                                let _ = tx.send(Err(e.to_string()));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string()));
                    }
                },
            );

            // Yield control to the GTK main loop in small slices so the
            // snapshot callback can fire, then collect the result.
            let result = loop {
                tokio::time::sleep(std::time::Duration::from_millis(16)).await;
                match poll_rx.try_recv() {
                    Ok(r) => break r,
                    Err(mpsc::TryRecvError::Empty) => continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        break Err("Screenshot channel disconnected".to_string());
                    }
                }
            };

            let _ = req.resp.send(result);
        }
    });

    // Future: start the axum HTTP server (background, non-blocking).
    use_future(|| async {
        if let Err(e) = start_devtools_server().await {
            tracing::error!("DevTools HTTP server stopped: {e}");
        }
    });

    rsx! {
        App {}

    }
}

// ─── HTTP Server ──────────────────────────────────────────────────────────────

async fn start_devtools_server() -> anyhow::Result<()> {
    use axum::{Router, routing};

    let app = Router::new()
        .route("/status", routing::get(http_status))
        .route("/eval", routing::post(http_eval))
        .route("/screenshot", routing::get(http_screenshot))
        .route("/dom", routing::get(http_dom))
        .route("/console", routing::get(http_console));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:9223").await?;
    tracing::info!("DevTools HTTP server listening on http://127.0.0.1:9223");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn http_status() -> &'static str {
    "ok"
}

/// POST /eval — body is plain JS; returns JSON {"result":"..."} or {"error":"..."}
async fn http_eval(body: String) -> (axum::http::StatusCode, String) {
    match do_eval(body).await {
        Ok(r) => (
            axum::http::StatusCode::OK,
            serde_json::json!({"result": r}).to_string(),
        ),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({"error": e}).to_string(),
        ),
    }
}

/// GET /screenshot — captures the GTK window as a PNG via the native GDK API.
async fn http_screenshot() -> axum::response::Response {
    use axum::response::IntoResponse;

    let tx = match SCREENSHOT_TX.get() {
        Some(tx) => tx,
        None => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Screenshot bridge not initialised",
            )
                .into_response();
        }
    };

    let (resp_tx, resp_rx) = oneshot::channel();
    if tx.send(ScreenshotRequest { resp: resp_tx }).await.is_err() {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Screenshot channel closed",
        )
            .into_response();
    }

    match resp_rx.await {
        Ok(Ok(png)) => ([(axum::http::header::CONTENT_TYPE, "image/png")], png).into_response(),
        Ok(Err(e)) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Screenshot request dropped",
        )
            .into_response(),
    }
}

/// GET /dom — returns document.documentElement.outerHTML
async fn http_dom() -> (axum::http::StatusCode, String) {
    match do_eval("document.documentElement.outerHTML").await {
        Ok(html) => (axum::http::StatusCode::OK, html),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

/// GET /console — returns JSON array of buffered log messages
async fn http_console() -> (axum::http::StatusCode, String) {
    match do_eval("JSON.stringify(window.__polyLogs || [])").await {
        Ok(logs) => (axum::http::StatusCode::OK, logs),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}
