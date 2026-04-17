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
//!
//! ## Hot-Reload Survival
//!
//! This app is designed to run under `dx serve --hotpatch` so the desktop window
//! stays alive across code changes (no window-jumping on every recompile).
//!
//! The eval and screenshot channels use a **recreatable** pattern: each
//! coroutine creates fresh `mpsc` channels on start, storing the sender in a
//! global `std::sync::Mutex`.  If Dioxus hot-patches the component tree and
//! remounts the root, the coroutines simply recreate their channels — the HTTP
//! server reads the latest sender from the mutex and everything keeps working.

use dioxus::document::eval;
use dioxus::prelude::*;
use poly_core::ui::App;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{mpsc, oneshot};
use poly_ui_macros::context_menu;

// ─── Eval Bridge ──────────────────────────────────────────────────────────────

struct EvalRequest {
    js: String,
    resp: oneshot::Sender<Result<String, String>>,
}

/// Current eval sender — replaced by the coroutine on each (re)start.
///
/// Uses `std::sync::Mutex` (not `tokio::sync::Mutex`) because:
/// - We only hold the lock briefly to clone/replace the sender.
/// - `std::sync::Mutex::new(None)` is const-constructible, so it works in a
///   `static`.
static EVAL_TX: std::sync::Mutex<Option<mpsc::Sender<EvalRequest>>> = std::sync::Mutex::new(None);

// ─── WebKit Screenshot Bridge ─────────────────────────────────────────────────

/// Request to capture the WebKit webview content as PNG.
struct ScreenshotRequest {
    resp: oneshot::Sender<Result<Vec<u8>, String>>,
}

/// Current screenshot sender — replaced by the coroutine on each (re)start.
static SCREENSHOT_TX: std::sync::Mutex<Option<mpsc::Sender<ScreenshotRequest>>> =
    std::sync::Mutex::new(None);

/// Guard: HTTP server binds to :9223 exactly once per process.
static HTTP_SERVER_STARTED: AtomicBool = AtomicBool::new(false);

/// Generation counter — increments on each **full component remount**.
///
/// Starts at 0; becomes 1 on first mount.  Under Dioxus 0.7 `--hotpatch`,
/// `use_coroutine` hook state is preserved across hotpatches, so this counter
/// stays at 1 across most hotpatch cycles (the hook does NOT restart unless
/// the scope is fully dropped).
///
/// For per-rebuild change detection, use `build_id()` instead — that reads
/// the MCP-managed counter in `/tmp/poly-devtools-rebuild-counter` which
/// increments on every `rebuild_app` call regardless of cargo caching.
static GENERATION: AtomicU64 = AtomicU64::new(0);

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

    let tx = {
        let guard = EVAL_TX.lock().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    }
    .ok_or_else(|| {
        "Eval bridge not yet initialised (coroutine may still be starting)".to_string()
    })?;
    let (resp_tx, resp_rx) = oneshot::channel();
    tx.send(EvalRequest {
        js: script,
        resp: resp_tx,
    })
    .await
    .map_err(|e| e.to_string())?;
    resp_rx.await.map_err(|e| e.to_string())?
}

/// Heuristic: does the string contain a `;` at depth 0 (outside of string
/// literals, braces, parentheses, and brackets)?
///
/// Previously this only tracked string depth, which caused every IIFE of the
/// form `(function(){ … })()`  to be classified as having a "top-level"
/// semicolon even though all semicolons are inside the `{}`.  That in turn
/// caused `do_eval` to double-wrap the IIFE in an outer function that discarded
/// the inner return value, silently returning `null` for every `js_eval` call.
///
/// The fix: also track `{`, `(`, `[` depth.  A `;` is only "top-level" when
/// depth is 0 (truly outside any block or group).
fn has_top_level_semicolon(s: &str) -> bool {
    let mut depth: i32 = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut prev = '\0';
    for c in s.chars() {
        if in_single {
            if c == '\'' && prev != '\\' {
                in_single = false;
            }
        } else if in_double {
            if c == '"' && prev != '\\' {
                in_double = false;
            }
        } else {
            match c {
                '\'' => in_single = true,
                '"' => in_double = true,
                '{' | '(' | '[' => depth += 1,
                '}' | ')' | ']' => depth -= 1,
                ';' if depth == 0 => return true,
                _ => {}
            }
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

    // NOTE: Channels are NOT pre-initialised here.  Each coroutine creates its
    // own channel pair on start and stores the sender in the global mutex.
    // This makes the bridge survive Dioxus hot-reload / hotpatch remounts.

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

// ─── Coroutine Tasks ──────────────────────────────────────────────────────────

/// Drives JS eval requests using dioxus's built-in eval().
///
/// Creates a fresh channel pair and stores the sender in [`EVAL_TX`].
/// This design survives hot-reload: if the component remounts, the new
/// coroutine creates new channels and the HTTP handlers automatically
/// pick up the latest sender via the global mutex.
async fn run_eval_coroutine() {
    let (tx, mut rx) = mpsc::channel::<EvalRequest>(64);

    // Publish the sender so HTTP handlers can reach us.
    match EVAL_TX.lock() {
        Ok(mut guard) => *guard = Some(tx),
        Err(poisoned) => *poisoned.into_inner() = Some(tx),
    }

    // Increment the generation counter so callers can detect hot-patch cycles.
    let generation_num = GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    tracing::info!(
        "Eval bridge coroutine started — generation {generation_num} (channels recreated)"
    );

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

    // Coroutine ending — clear the sender so callers get a clean error
    // instead of sending into a dead channel.
    match EVAL_TX.lock() {
        Ok(mut guard) => *guard = None,
        Err(poisoned) => *poisoned.into_inner() = None,
    }
    tracing::warn!("Eval bridge coroutine stopped");
}

/// Captures WebKit content via the native snapshot API.
///
/// Must run on the GTK main thread — all dioxus-desktop coroutines do.
/// Creates a fresh channel pair and stores the sender in [`SCREENSHOT_TX`],
/// same recreatable pattern as the eval coroutine.
async fn run_screenshot_coroutine() {
    let (tx, mut rx) = mpsc::channel::<ScreenshotRequest>(4);

    // Publish the sender so HTTP handlers can reach us.
    match SCREENSHOT_TX.lock() {
        Ok(mut guard) => *guard = Some(tx),
        Err(poisoned) => *poisoned.into_inner() = Some(tx),
    }
    tracing::info!("Screenshot bridge coroutine started (channels recreated)");

    // Grab the WebView handle once — GObject clone is cheap.
    let wv = {
        use wry::WebViewExtUnix as _;
        dioxus::desktop::window().webview.webview()
    };
    while let Some(req) = rx.recv().await {
        use std::sync::mpsc as std_mpsc;
        use webkit2gtk::WebViewExt as _;

        let (cb_tx, poll_rx) = std_mpsc::channel::<Result<Vec<u8>, String>>();
        wv.snapshot(
            webkit2gtk::SnapshotRegion::Visible,
            webkit2gtk::SnapshotOptions::empty(),
            webkit2gtk::gio::Cancellable::NONE,
            move |result: Result<cairo::Surface, webkit2gtk::glib::Error>| match result {
                Ok(surface) => {
                    let mut buf: Vec<u8> = Vec::new();
                    match surface.write_to_png(&mut buf) {
                        Ok(_) => {
                            let _ = cb_tx.send(Ok(buf));
                        }
                        Err(e) => {
                            let _ = cb_tx.send(Err(e.to_string()));
                        }
                    }
                }
                Err(e) => {
                    let _ = cb_tx.send(Err(e.to_string()));
                }
            },
        );
        // Poll in 16 ms slices so the GTK main loop can fire the snapshot callback.
        let result = loop {
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
            match poll_rx.try_recv() {
                Ok(r) => break r,
                Err(std_mpsc::TryRecvError::Empty) => continue,
                Err(std_mpsc::TryRecvError::Disconnected) => {
                    break Err("Screenshot channel disconnected".to_string());
                }
            }
        };
        let _ = req.resp.send(result);
    }

    // Coroutine ending — clear the sender.
    match SCREENSHOT_TX.lock() {
        Ok(mut guard) => *guard = None,
        Err(poisoned) => *poisoned.into_inner() = None,
    }
    tracing::warn!("Screenshot bridge coroutine stopped");
}

// ─── Dioxus Wrapper Component ─────────────────────────────────────────────────

/// Root component for the devtools build.
///
/// Spawns the eval and screenshot coroutines, then starts the HTTP server,
/// before delegating rendering entirely to [`App`].
///
/// **Hot-reload safe:** coroutines recreate their channels on each mount.
#[context_menu(inherit)]
/// The HTTP server is guarded by [`HTTP_SERVER_STARTED`] so it only binds once.
#[component]
fn DevtoolsShell() -> Element {
    use_coroutine(move |_: UnboundedReceiver<()>| async move {
        run_eval_coroutine().await;
    });
    use_coroutine(move |_: UnboundedReceiver<()>| async move {
        run_screenshot_coroutine().await;
    });
    use_future(|| async {
        // Only start the HTTP server once per process — if we've already bound
        // :9223, a remount (hot-reload) must not try again.
        if HTTP_SERVER_STARTED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
            && let Err(e) = start_devtools_server().await
        {
            tracing::error!("DevTools HTTP server stopped: {e}");
            HTTP_SERVER_STARTED.store(false, Ordering::SeqCst);
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
        .route("/generation", routing::get(http_generation))
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

/// Path to the rebuild counter file written by `poly-desktop-devtools-mcp`.
///
/// The MCP increments this file's contents on every `rebuild_app` call.
/// Reading it at runtime is the most reliable way to detect "did a rebuild
/// just happen?" — it avoids the `cargo rerun-if-changed` checksum pitfall
/// where a `touch`-only mtime update is ignored by cargo.
const REBUILD_COUNTER_PATH: &str = "/tmp/poly-devtools-rebuild-counter";

/// Returns a monotonically increasing rebuild counter written by the MCP.
///
/// | Value | Meaning |
/// |---|---|
/// | 0 | No `rebuild_app` call has been made yet this session (or counter file was deleted) |
/// | N | `rebuild_app` has been called N times since the counter was last reset |
///
/// Combine with `pid` to distinguish hotpatch rebuilds from full restarts:
/// - `build_id` increased, `pid` same → hotpatch (window survived, code updated)
/// - `pid` changed (generation back to 1) → full process restart
fn build_id() -> u64 {
    std::fs::read_to_string(REBUILD_COUNTER_PATH)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// GET /generation — returns rebuild-detection fields.
///
/// | Field | Meaning |
/// |---|---|
/// | `generation` | Component-mount count (increments when `DevtoolsShell` fully unmounts+remounts; stays at 1 across hotpatches in Dioxus 0.7 because hook state is preserved) |
/// | `build_id`   | MCP rebuild counter — increments on every `rebuild_app` call (reads `/tmp/poly-devtools-rebuild-counter`). 0 = no rebuild called yet this session. |
/// | `pid`        | OS process ID — changes only on full kill+relaunch |
async fn http_generation() -> String {
    let generation_num = GENERATION.load(Ordering::Relaxed);
    let pid = std::process::id();
    serde_json::json!({
        "generation": generation_num,
        "build_id": build_id(),
        "pid": pid
    })
    .to_string()
}

/// POST /eval — body is plain JS; returns JSON {"result":"..."} or {"error":"..."}
///
/// Retries automatically if the dioxus eval bridge returns `EvalError::Finished`,
/// which can happen briefly after a page navigation causes the JS context to reset.
async fn http_eval(body: String) -> (axum::http::StatusCode, String) {
    const MAX_RETRIES: u32 = 5;
    const RETRY_DELAY_MS: u64 = 250;

    let mut last_err = String::new();
    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
            tracing::debug!("Retrying eval (attempt {attempt}/{MAX_RETRIES}) after: {last_err}");
        }
        match do_eval(body.clone()).await {
            Ok(r) => {
                return (
                    axum::http::StatusCode::OK,
                    serde_json::json!({"result": r}).to_string(),
                );
            }
            Err(e) => {
                // Only retry on Finished/Communication errors — these indicate a
                // transient JS context reset after a Dioxus navigation.
                if e.contains("Finished") || e.contains("Communication") {
                    last_err = e;
                    continue;
                }
                // Any other error: fail immediately.
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    serde_json::json!({"error": e}).to_string(),
                );
            }
        }
    }
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        serde_json::json!({"error": format!("Eval failed after {MAX_RETRIES} retries: {last_err}")}).to_string(),
    )
}

/// GET /screenshot — captures the GTK window as a PNG via the native GDK API.
async fn http_screenshot() -> axum::response::Response {
    use axum::response::IntoResponse;

    let tx = {
        let guard = SCREENSHOT_TX.lock().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    };
    let Some(tx) = tx else {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Screenshot bridge not initialised (coroutine may still be starting)",
        )
            .into_response();
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
