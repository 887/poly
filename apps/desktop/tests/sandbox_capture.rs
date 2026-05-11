//! Integration test for `WrySandbox` — Phase A.6.
//!
//! Spins up a tiny axum HTTP server that:
//!   GET /start → 302 redirect to `/captured?token=abc`
//!   GET /captured → 200 (terminal page)
//!
//! Drives `WrySandbox::open_browser_sandbox` with capture pattern
//! `*//*/captured*`, asserts the resolved URL contains `token=abc`.
//!
//! The test is marked `#[cfg(not(target_arch = "wasm32"))]` because the Wry
//! sandbox is native-only.  It also requires a display (X11/Wayland) on Linux.
//! On CI without a display, set `POLY_SANDBOX_SKIP_DISPLAY_TEST=1` to skip.
//!
//! Note: This test opens a real GTK/Wry window. It must be run in a thread
//! that can safely call GTK. Because cargo test spawns tests on separate
//! threads (each test gets its own thread), we use `with_any_thread = true`
//! inside WrySandbox which allows the GTK event loop on non-main threads.
//!
//! Run with: `cargo test -p poly-desktop --test sandbox_capture`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::SocketAddr;

use axum::{Router, routing};
use poly_host_sandbox::{HostSandbox, WrySandbox};
use tokio::net::TcpListener;

/// Spawn an axum server with:
/// - GET /start → 302 to /captured?token=abc
/// - GET /captured → 200 "captured"
///
/// Returns the bound address.
async fn spawn_mock_server() -> SocketAddr {
    let app = Router::new()
        .route(
            "/start",
            routing::get(|| async {
                axum::response::Redirect::to("/captured?token=abc")
            }),
        )
        .route(
            "/captured",
            routing::get(|| async { "captured" }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    addr
}

/// Skip helper: returns true when not explicitly opted into display tests.
///
/// Display tests require a functional GTK-compatible display (X11 or Wayland).
/// They are opt-in because:
/// - Wayland compositors may accept socket connections but fail at the protocol
///   level (GDK Error 71 / XDG_WM_BASE), causing fatal GTK aborts.
/// - CI environments often lack a display server entirely.
///
/// To run the display-requiring test:
///   POLY_SANDBOX_RUN_DISPLAY_TEST=1 cargo test -p poly-desktop --test sandbox_capture
///
/// On Linux with a broken Wayland compositor, also set:
///   GDK_BACKEND=x11 POLY_SANDBOX_RUN_DISPLAY_TEST=1 cargo test ...
fn should_skip() -> bool {
    // Only run when the user explicitly opts in.
    std::env::var("POLY_SANDBOX_RUN_DISPLAY_TEST").is_err()
}

#[tokio::test]
async fn sandbox_captures_redirect_url() {
    if should_skip() {
        eprintln!("sandbox_capture: skipping — set POLY_SANDBOX_RUN_DISPLAY_TEST=1 to enable");
        eprintln!("  On Linux with broken Wayland: GDK_BACKEND=x11 POLY_SANDBOX_RUN_DISPLAY_TEST=1 cargo test ...");
        return;
    }

    let addr = spawn_mock_server().await;
    let start_url = format!("http://{addr}/start");
    // Pattern: match any URL containing "/captured" followed by a query
    let pattern = format!("*{addr}/captured*");

    let sandbox = WrySandbox;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        sandbox.open_browser_sandbox(start_url, pattern),
    )
    .await
    .expect("sandbox timed out after 15s")
    .expect("sandbox returned error");

    assert!(
        result.captured_url.contains("token=abc"),
        "expected token=abc in captured URL, got: {}",
        result.captured_url
    );
}

#[test]
fn advertised_host_caps_includes_sandbox_browser() {
    use poly_client::HostCap;
    let caps = poly_host_sandbox::advertised_host_caps();
    assert!(
        caps.contains(&HostCap::SandboxBrowser),
        "expected SandboxBrowser in advertised caps, got: {caps:?}"
    );
}
