//! `WrySandbox` — real Wry/WebKit2GTK sandbox implementation.
//!
//! Phase A of `docs/plans/plan-host-sandbox-impl.md`.
//!
//! ## Architecture
//!
//! Each call to `open_browser_sandbox` spawns a dedicated OS thread that:
//! 1. Runs its own tao `EventLoop` (with `any_thread = true` on Linux/Unix).
//! 2. Opens a new `tao::Window` + `wry::WebView` in incognito mode (fresh
//!    cookie jar — A.3 cookie isolation).
//! 3. Intercepts every navigation event via `with_navigation_handler`. When
//!    the URL matches `capture_url_pattern` (glob), it sends the URL back
//!    through a `std::sync::mpsc` channel, signals exit, and returns `false`
//!    to cancel the navigation.
//! 4. If the window is closed by the user (`CloseRequested`), sends
//!    `Err(UserCancelled)` — A.4 cancel path.
//!
//! The tokio-async `open_browser_sandbox` method bridges the blocking thread
//! result back to the async world via `tokio::sync::oneshot`.
//!
//! ## Cookie isolation (A.3)
//!
//! We use `with_incognito(true)` so the sandbox WebView gets a fresh,
//! non-persistent data store. No cookies are shared with the main app.
//!
//! ## Why a dedicated OS thread per call?
//!
//! On Linux, GTK requires all WebView operations on the thread that called
//! `gtk::init()`. The dioxus desktop already owns the main thread's GTK loop.
//! A second dedicated OS thread (with `any_thread = true`) gets its own GLib
//! main context, allowing parallel WebView windows without conflicting with
//! the main loop. The thread exits as soon as the sandbox window is closed.

use std::sync::mpsc as std_mpsc;

use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use tracing::{debug, warn};

use crate::{HostSandbox, SandboxError, SandboxResult, glob_matches};

/// Real Wry-based sandbox. Implements `HostSandbox`.
///
/// Stateless — each `open_browser_sandbox` call spawns its own OS thread +
/// event loop and tears it down after capture/cancel.
pub struct WrySandbox;

#[async_trait::async_trait]
impl HostSandbox for WrySandbox {
    async fn open_browser_sandbox(
        &self,
        url: String,
        capture_url_pattern: String,
    ) -> Result<SandboxResult, SandboxError> {
        // Bridge: spawn the blocking event loop on an OS thread, deliver the
        // result back via a tokio oneshot.
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<SandboxResult, SandboxError>>();

        std::thread::Builder::new()
            .name("poly-sandbox".into())
            .spawn(move || {
                let result = run_sandbox_event_loop(&url, &capture_url_pattern);
                let _ = tx.send(result);
            })
            .map_err(|e| SandboxError::Internal(format!("thread spawn failed: {e}")))?;

        rx.await
            .map_err(|_| SandboxError::Internal("sandbox thread exited without result".into()))?
    }
}

/// Run a tao event loop on the current OS thread, open a sandbox window,
/// and return when the capture pattern matches or the user cancels.
fn run_sandbox_event_loop(
    url: &str,
    pattern: &str,
) -> Result<SandboxResult, SandboxError> {
    // Channel: the navigation_handler (sync closure) sends the captured URL
    // back to the event-loop body.
    let (nav_tx, nav_rx) = std_mpsc::channel::<Result<String, SandboxError>>();

    // Build the tao event loop. `any_thread = true` is required on Linux so
    // the loop can run on a non-main OS thread alongside the dioxus GTK loop.
    let mut event_loop = {
        let mut builder = EventLoopBuilder::<()>::with_user_event();

        #[cfg(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
        ))]
        {
            use tao::platform::unix::EventLoopBuilderExtUnix as _;
            builder.with_any_thread(true);
        }

        builder.build()
    };

    // Build the window.
    let window = WindowBuilder::new()
        .with_title("Authentication — Poly")
        .with_inner_size(tao::dpi::LogicalSize::new(900.0_f64, 700.0_f64))
        .build(&event_loop)
        .map_err(|e| SandboxError::Internal(format!("window build failed: {e}")))?;

    let pattern_owned = pattern.to_owned();
    let nav_tx_clone = nav_tx.clone();

    // Build the WebView.
    //
    // `with_incognito(true)` → fresh non-persistent cookie jar (A.3).
    // `with_navigation_handler` → intercept every navigation and check
    //   against the capture pattern (A.2).
    let webview_builder = wry::WebViewBuilder::new()
        .with_url(url)
        .with_incognito(true)
        .with_navigation_handler(move |nav_url: String| {
            debug!("sandbox nav: {nav_url}");
            if glob_matches(&pattern_owned, &nav_url) {
                // Pattern matched — send URL and signal we're done.
                debug!("sandbox: captured {nav_url}");
                let _ = nav_tx_clone.send(Ok(nav_url));
                // Return false to block loading the capture URL in the WebView.
                return false;
            }
            true // Allow navigation.
        });

    // On Linux we must use `build_gtk` with `default_vbox()`.
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
    ))]
    let _webview = {
        use tao::platform::unix::WindowExtUnix as _;
        use wry::WebViewBuilderExtUnix as _;

        let vbox = window.default_vbox()
            .ok_or_else(|| SandboxError::Internal("no default vbox on window".into()))?;
        webview_builder
            .build_gtk(vbox)
            .map_err(|e| SandboxError::Internal(format!("webview build failed: {e}")))?
    };

    #[cfg(not(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
    )))]
    let _webview = webview_builder
        .build(&window)
        .map_err(|e| SandboxError::Internal(format!("webview build failed: {e}")))?;

    // Final result; set when the event loop exits.
    let mut outcome: Option<Result<SandboxResult, SandboxError>> = None;

    use tao::platform::run_return::EventLoopExtRunReturn as _;

    event_loop.run_return(|event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        // Check for an incoming capture or error from the navigation handler.
        match nav_rx.try_recv() {
            Ok(r) => {
                outcome = Some(r.map(|captured_url| SandboxResult { captured_url }));
                *control_flow = ControlFlow::Exit;
                return;
            }
            Err(std_mpsc::TryRecvError::Disconnected) => {
                outcome = Some(Err(SandboxError::Internal(
                    "navigation channel disconnected".into(),
                )));
                *control_flow = ControlFlow::Exit;
                return;
            }
            Err(std_mpsc::TryRecvError::Empty) => {}
        }

        #[allow(clippy::wildcard_enum_match_arm)]
        match event {
            tao::event::Event::WindowEvent {
                event: tao::event::WindowEvent::CloseRequested,
                ..
            } => {
                // A.4 cancel path: user closed the window.
                warn!("sandbox window closed by user — UserCancelled");
                outcome = Some(Err(SandboxError::UserCancelled));
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });

    outcome.unwrap_or_else(|| Err(SandboxError::Internal("event loop exited without result".into())))
}
