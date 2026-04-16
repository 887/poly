//! # poly-desktop-web
//!
//! Thin native Wry shell that loads the Poly WASM app from a `dx serve` dev
//! server.  The shell **never** gets recompiled during development — only the
//! WASM page reloads when `dx serve` finishes rebuilding.
//!
//! ## How it works
//!
//! 1. Opens a native Wry/tao window.
//! 2. Loads `http://127.0.0.1:${POLY_DEV_URL:-3002}/` in the webview.
//! 3. Starts an HTTP eval-bridge on **port 9223** (same as `poly-desktop-devtools`)
//!    so the existing `poly-desktop-devtools-mcp` can drive the app.
//! 4. On each page load, injects a small JS bootstrap that bridges `window.ipc`
//!    back to the eval-bridge IPC handler.
//!
//! ## Eval bridge architecture
//!
//! ```
//! tokio HTTP server → UserEvent::EvalRequest → EventLoopProxy
//!     → tao event loop → webview.evaluate_script()
//!     → JS calls window.ipc.postMessage(JSON)
//!     → Wry IPC handler → oneshot channel → HTTP response
//! ```
//!
//! ## Screenshot
//!
//! Screenshots use WebKit2GTK's native snapshot API, triggered via
//! `UserEvent::ScreenshotRequest` → gtk main thread.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::response::IntoResponse;
use serde_json::Value;
use tokio::sync::oneshot;

// ─── Constants ────────────────────────────────────────────────────────────────

const BRIDGE_PORT: u16 = 9223;
const DEFAULT_DEV_URL: &str = "http://127.0.0.1:3002";
const REBUILD_COUNTER_PATH: &str = "/tmp/poly-devtools-rebuild-counter";

// ─── Shared state types ───────────────────────────────────────────────────────

/// A pending eval request awaiting a JS result.
struct PendingEval {
    tx: oneshot::Sender<Result<String, String>>,
}

/// Shared map from request ID → pending response channel.
type PendingEvals = Arc<Mutex<HashMap<u64, PendingEval>>>;

/// Channel to request a screenshot from the GTK main thread.
struct ScreenshotRequest {
    resp: oneshot::Sender<Result<Vec<u8>, String>>,
}

type ScreenshotTx = Arc<Mutex<Option<tokio::sync::mpsc::Sender<ScreenshotRequest>>>>;

// ─── UserEvent ────────────────────────────────────────────────────────────────

/// Events sent from the tokio HTTP server to the tao/Wry main thread.
#[derive(Debug)]
enum UserEvent {
    /// Run `script` in the webview; deliver result to `id`.
    EvalRequest { id: u64, script: String },
    /// Reload the webview by navigating to the initial URL.
    LoadUrl(String),
    /// Wake the event loop so a pending screenshot request gets processed.
    WakeForScreenshot,
}

// ─── JS injection ─────────────────────────────────────────────────────────────

/// Injected as an initialization script on every page load (before any page JS).
///
/// Defines `window.__poly_eval(id, script)` that:
/// 1. Evaluates `script` as a Promise.
/// 2. Sends the result back via `window.ipc.postMessage(JSON)`.
///
/// Also installs the console log interceptor so `/console` works.
const INIT_SCRIPT: &str = r#"
(function() {
    /* ── Eval bridge ── */
    window.__poly_eval = function(id, script) {
        Promise.resolve().then(function() { return eval(script); }).then(
            function(r) {
                window.ipc.postMessage(JSON.stringify({id: id, result: String(r === null || r === undefined ? 'null' : r)}));
            },
            function(e) {
                window.ipc.postMessage(JSON.stringify({id: id, error: String(e)}));
            }
        );
    };

    /* ── Console log buffer ── */
    window.__polyLogs = [];
    ['log','warn','error','info','debug'].forEach(function(lvl) {
        var orig = console[lvl];
        console[lvl] = function() {
            var args = Array.prototype.slice.call(arguments);
            window.__polyLogs.push({
                level: lvl,
                message: args.map(function(a){
                    return typeof a === 'object' ? JSON.stringify(a) : String(a);
                }).join(' '),
                ts: Date.now()
            });
            if (window.__polyLogs.length > 200) { window.__polyLogs.shift(); }
            orig.apply(console, args);
        };
    });
})();
"#;

// ─── HTTP server helpers ───────────────────────────────────────────────────────

fn read_rebuild_counter() -> u64 {
    std::fs::read_to_string(REBUILD_COUNTER_PATH)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

async fn do_eval(
    proxy: tao::event_loop::EventLoopProxy<UserEvent>,
    pending: PendingEvals,
    next_id: Arc<AtomicU64>,
    js: String,
) -> Result<String, String> {
    let id = next_id.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();

    {
        let mut map = pending.lock().map_err(|e| e.to_string())?;
        map.insert(id, PendingEval { tx });
    }

    let script = format!(
        "window.__poly_eval({}, {})",
        id,
        serde_json::to_string(&js).map_err(|e| e.to_string())?
    );

    proxy
        .send_event(UserEvent::EvalRequest { id, script })
        .map_err(|e| format!("Event loop closed: {e}"))?;

    match tokio::time::timeout(std::time::Duration::from_secs(15), rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("Eval channel closed before response".to_string()),
        Err(_) => {
            let mut map = pending.lock().map_err(|e| e.to_string())?;
            map.remove(&id);
            Err("Eval timeout after 15s".to_string())
        }
    }
}

// ─── Axum HTTP server ─────────────────────────────────────────────────────────

async fn start_http_server(
    proxy: tao::event_loop::EventLoopProxy<UserEvent>,
    pending: PendingEvals,
    next_id: Arc<AtomicU64>,
    screenshot_tx: ScreenshotTx,
    dev_url: String,
    generation: Arc<AtomicU64>,
) -> anyhow::Result<()> {
    use axum::{Router, routing};
    use tower_http::cors::{Any, CorsLayer};

    let proxy_clone = proxy.clone();
    let pending_clone = pending.clone();
    let next_id_clone = next_id.clone();
    let screenshot_tx_clone = screenshot_tx.clone();
    let dev_url_clone = dev_url.clone();
    let generation_clone = generation.clone();

    let app = Router::new()
        .route("/status", routing::get(|| async { "ok" }))
        .route(
            "/generation",
            routing::get({
                let gen_arc = generation_clone.clone();
                move || {
                    let gen_arc = gen_arc.clone();
                    async move {
                        let pid = std::process::id();
                        let gen_val = gen_arc.load(Ordering::Relaxed);
                        serde_json::json!({
                            "generation": gen_val,
                            "build_id": read_rebuild_counter(),
                            "pid": pid
                        })
                        .to_string()
                    }
                }
            }),
        )
        .route(
            "/eval",
            routing::post({
                let proxy = proxy_clone.clone();
                let pending = pending_clone.clone();
                let next_id = next_id_clone.clone();
                move |body: String| {
                    let proxy = proxy.clone();
                    let pending = pending.clone();
                    let next_id = next_id.clone();
                    async move {
                        const MAX_RETRIES: u32 = 5;
                        const RETRY_DELAY_MS: u64 = 250;
                        let mut last_err = String::new();
                        for attempt in 0..=MAX_RETRIES {
                            if attempt > 0 {
                                tokio::time::sleep(std::time::Duration::from_millis(
                                    RETRY_DELAY_MS,
                                ))
                                .await;
                            }
                            match do_eval(
                                proxy.clone(),
                                pending.clone(),
                                next_id.clone(),
                                body.clone(),
                            )
                            .await
                            {
                                Ok(r) => {
                                    return (
                                        axum::http::StatusCode::OK,
                                        serde_json::json!({"result": r}).to_string(),
                                    );
                                }
                                Err(e) => {
                                    last_err = e;
                                    // Retry on transient errors
                                    if last_err.contains("timeout")
                                        || last_err.contains("channel closed")
                                        || last_err.contains("closed")
                                    {
                                        continue;
                                    }
                                    break;
                                }
                            }
                        }
                        (
                            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                            serde_json::json!({"error": last_err}).to_string(),
                        )
                    }
                }
            }),
        )
        .route(
            "/screenshot",
            routing::get({
                let stx = screenshot_tx_clone.clone();
                let proxy_ss = proxy_clone.clone();
                move || {
                    let stx = stx.clone();
                    let proxy_ss = proxy_ss.clone();
                    async move { http_screenshot(stx, proxy_ss).await }
                }
            }),
        )
        .route(
            "/reload",
            routing::post({
                let proxy = proxy_clone.clone();
                let url = dev_url_clone.clone();
                move || {
                    let proxy = proxy.clone();
                    let url = url.clone();
                    async move {
                        match proxy.send_event(UserEvent::LoadUrl(url)) {
                            Ok(()) => (axum::http::StatusCode::OK, "reloading".to_string()),
                            Err(e) => (
                                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                                format!("Event loop closed: {e}"),
                            ),
                        }
                    }
                }
            }),
        )
        .route(
            "/dom",
            routing::get({
                let proxy = proxy_clone.clone();
                let pending = pending_clone.clone();
                let next_id = next_id_clone.clone();
                move || {
                    let proxy = proxy.clone();
                    let pending = pending.clone();
                    let next_id = next_id.clone();
                    async move {
                        match do_eval(
                            proxy,
                            pending,
                            next_id,
                            "document.documentElement.outerHTML".to_string(),
                        )
                        .await
                        {
                            Ok(html) => (axum::http::StatusCode::OK, html),
                            Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e),
                        }
                    }
                }
            }),
        )
        // NOTE: `/host` no longer lives on the 9223 MCP eval port. The
        // full `/host/*` route set is served on port 9333 by the
        // `poly-host` router we spawn separately in `main()`. Moving it
        // aligns the Wry shell with `poly_host_bridge::BRIDGE_PORT` so
        // WASM-side callers hit the same URL on every platform.
        .route(
            "/console",
            routing::get({
                let proxy = proxy_clone;
                let pending = pending_clone;
                let next_id = next_id_clone;
                move || {
                    let proxy = proxy.clone();
                    let pending = pending.clone();
                    let next_id = next_id.clone();
                    async move {
                        match do_eval(
                            proxy,
                            pending,
                            next_id,
                            "JSON.stringify(window.__polyLogs || [])".to_string(),
                        )
                        .await
                        {
                            Ok(logs) => (axum::http::StatusCode::OK, logs),
                            Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e),
                        }
                    }
                }
            }),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{BRIDGE_PORT}")).await?;
    tracing::info!("Eval bridge HTTP server listening on http://127.0.0.1:{BRIDGE_PORT}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn http_screenshot(
    screenshot_tx: ScreenshotTx,
    proxy: tao::event_loop::EventLoopProxy<UserEvent>,
) -> axum::response::Response {
    let tx = {
        let guard = screenshot_tx.lock().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    };
    let Some(tx) = tx else {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Screenshot bridge not yet initialised",
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

    // Wake the tao event loop so it processes the screenshot request.
    // Without this, ControlFlow::Wait keeps the loop blocked on idle windows.
    let _ = proxy.send_event(UserEvent::WakeForScreenshot);

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

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let dev_url = std::env::var("POLY_DEV_URL").unwrap_or_else(|_| DEFAULT_DEV_URL.to_string());
    tracing::info!("poly-desktop-web starting — loading WASM from {dev_url}");
    tracing::info!("Eval bridge will be available at http://127.0.0.1:{BRIDGE_PORT}");

    // ── Shared state ──────────────────────────────────────────────────────────
    let pending: PendingEvals = Arc::new(Mutex::new(HashMap::new()));
    let next_id = Arc::new(AtomicU64::new(1));
    let screenshot_tx: ScreenshotTx = Arc::new(Mutex::new(None));
    let generation = Arc::new(AtomicU64::new(0));
    let http_started = AtomicBool::new(false);

    // ── tao event loop ────────────────────────────────────────────────────────
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tao::window::WindowBuilder;
    #[cfg(target_os = "linux")]
    use tao::platform::unix::WindowExtUnix as _;

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // ── Window ────────────────────────────────────────────────────────────────
    let window = match WindowBuilder::new()
        .with_title("Poly Desktop (Web Dev)")
        .with_inner_size(tao::dpi::LogicalSize::new(1440.0_f64, 900.0_f64))
        .with_min_inner_size(tao::dpi::LogicalSize::new(800.0_f64, 600.0_f64))
        .build(&event_loop)
    {
        Ok(w) => w,
        Err(e) => {
            eprintln!("fatal: Failed to create window: {e}");
            std::process::exit(1);
        }
    };

    // ── WebView ───────────────────────────────────────────────────────────────
    let pending_ipc = pending.clone();

    let builder = wry::WebViewBuilder::new()
        .with_url(&dev_url)
        .with_initialization_script(INIT_SCRIPT)
        .with_ipc_handler(move |req: wry::http::Request<String>| {
            let body = req.into_body();
            // Parse JSON: { id: u64, result: "..." } or { id: u64, error: "..." }
            let Ok(v) = serde_json::from_str::<Value>(&body) else {
                tracing::warn!("IPC: invalid JSON from JS: {body}");
                return;
            };
            let Some(id) = v.get("id").and_then(|v| v.as_u64()) else {
                tracing::warn!("IPC: missing id in message");
                return;
            };
            let result = if let Some(r) = v.get("result").and_then(|v| v.as_str()) {
                Ok(r.to_string())
            } else if let Some(e) = v.get("error").and_then(|v| v.as_str()) {
                Err(e.to_string())
            } else {
                Ok(v.to_string())
            };

            let mut map = pending_ipc.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(pending_eval) = map.remove(&id) {
                let _ = pending_eval.tx.send(result);
            }
        });

    // On Linux we need to use build_gtk for Wayland/X11 compatibility.
    // Must use default_vbox() — not gtk_window() — so the webview gets proper
    // size allocation from tao's internal GTK container.
    #[cfg(target_os = "linux")]
    let webview = {
        use wry::WebViewBuilderExtUnix as _;
        let vbox = match window.default_vbox() {
            Some(v) => v,
            None => {
                eprintln!("fatal: tao window should have a default vbox");
                std::process::exit(1);
            }
        };
        match builder.build_gtk(vbox) {
            Ok(wv) => wv,
            Err(e) => {
                eprintln!("fatal: Failed to create webview: {e}");
                std::process::exit(1);
            }
        }
    };

    #[cfg(not(target_os = "linux"))]
    let webview = match builder.build(&window) {
        Ok(wv) => wv,
        Err(e) => {
            eprintln!("fatal: Failed to create webview: {e}");
            std::process::exit(1);
        }
    };

    // ── Screenshot channel ────────────────────────────────────────────────────
    // Set up the screenshot mpsc channel now that we have the webview handle.
    // The tokio task will pull from this channel in the event loop.
    let (ss_tx, mut ss_rx) = tokio::sync::mpsc::channel::<ScreenshotRequest>(4);
    {
        let mut guard = screenshot_tx.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(ss_tx);
    }

    // Grab webkit2gtk handle once (cheap GObject clone).
    #[cfg(target_os = "linux")]
    let wk_webview = {
        use wry::WebViewExtUnix as _;
        webview.webview()
    };

    // ── Start tokio runtime + HTTP server ─────────────────────────────────────
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("fatal: failed to create tokio runtime: {e}");
            std::process::exit(1);
        }
    };

    if !http_started.swap(true, Ordering::SeqCst) {
        let proxy2 = proxy.clone();
        let pending2 = pending.clone();
        let next_id2 = next_id.clone();
        let screenshot_tx2 = screenshot_tx.clone();
        let dev_url2 = dev_url.clone();
        let gen2 = generation.clone();
        rt.spawn(async move {
            if let Err(e) = start_http_server(
                proxy2,
                pending2,
                next_id2,
                screenshot_tx2,
                dev_url2,
                gen2,
            )
            .await
            {
                tracing::error!("HTTP server error: {e}");
            }
        });
        // Host bridge used to live on a separate loopback port (9333) in
        // this shell. It now rides inside `dx serve --platform web
        // --fullstack`: the apps/desktop native server binary mounts
        // `/host/*` on the same 3002 port it already serves WASM from.
    }

    // Increment generation on first start
    generation.fetch_add(1, Ordering::Relaxed);

    // ── Screenshot polling task (runs on GTK main thread via tao event loop) ──
    // We handle screenshot requests inside the event loop below using try_recv.

    // ── Event loop ────────────────────────────────────────────────────────────
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        // Poll for pending screenshot requests (non-blocking).
        // This runs in the GTK main thread so it's safe to call webkit2gtk snapshot.
        #[cfg(target_os = "linux")]
        while let Ok(req) = ss_rx.try_recv() {
            use std::sync::mpsc as std_mpsc;
            use webkit2gtk::WebViewExt as _;

            let (cb_tx, poll_rx) = std_mpsc::channel::<Result<Vec<u8>, String>>();
            let wk = wk_webview.clone();
            wk.snapshot(
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
            // Spawn a tokio task to wait for the snapshot result and deliver it.
            let poll_rx = std::sync::Mutex::new(poll_rx);
            rt.spawn(async move {
                let result = loop {
                    tokio::time::sleep(std::time::Duration::from_millis(16)).await;
                    let guard = poll_rx.lock().unwrap_or_else(|e| e.into_inner());
                    match guard.try_recv() {
                        Ok(r) => break r,
                        Err(std_mpsc::TryRecvError::Empty) => continue,
                        Err(std_mpsc::TryRecvError::Disconnected) => {
                            break Err("Screenshot channel disconnected".to_string());
                        }
                    }
                };
                let _ = req.resp.send(result);
            });
        }

        match event {
            tao::event::Event::UserEvent(UserEvent::EvalRequest { id: _id, script }) => {
                if let Err(e) = webview.evaluate_script(&script) {
                    tracing::warn!("evaluate_script error: {e}");
                }
            }
            tao::event::Event::UserEvent(UserEvent::LoadUrl(url)) => {
                tracing::info!("Reloading webview → {url}");
                if let Err(e) = webview.load_url(&url) {
                    tracing::warn!("load_url error: {e}");
                }
            }
            tao::event::Event::UserEvent(UserEvent::WakeForScreenshot) => {
                // No-op: the event loop woke up and will process ss_rx.try_recv()
                // at the top of the next iteration (already handled above).
            }
            tao::event::Event::WindowEvent {
                event: tao::event::WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}
