// FUTURE: docs/plans/plan-host-sandbox-impl.md — Phase A (Wry) + Phase C (Web) shipped.
//! Host-side sandbox host-cap.
//!
//! `StubSandbox` returns `Err(SandboxError::NotImplemented)` for every call.
//!
//! Phase A (feature `wry-sandbox`): real `WrySandbox` that opens an isolated
//! Wry/WebKit2GTK window, intercepts navigation events, and resolves with the
//! captured URL when the pattern matches. Cookie isolation via incognito mode.
//!
//! Phase C (feature `web`): the apps/web fullstack server hosts a
//! `/sandbox/<id>` redirect shim and `WebSandbox` (in `apps/web/src/sandbox.rs`)
//! drives the popup-window flow via `window.open` + `postMessage`.
//!
//! What we ship:
//! - The `HostSandbox` trait — host-side contract.
//! - `StubSandbox` — no-op, returns `NotImplemented` (default build).
//! - `WrySandbox` — real Wry impl, gated on `wry-sandbox` (Phase A).
//! - `advertised_host_caps()` — returns `[SandboxBrowser]` when either
//!   `wry-sandbox` or `web` feature is active, `[]` otherwise.

pub use poly_client::HostCap;

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("sandbox not implemented in v1")]
    NotImplemented,
    #[error("invalid url: {0}")]
    InvalidUrl(String),
    #[error("user cancelled")]
    UserCancelled,
    #[error("sandbox internal error: {0}")]
    Internal(String),
    /// Shell-specific IPC / DevTools Protocol error.
    #[error("sandbox IPC error: {0}")]
    CdpError(String),
}

#[derive(Debug, Clone)]
pub struct SandboxResult {
    /// The captured URL (or fragment of it) that matched `capture_url_pattern`.
    pub captured_url: String,
}

// On WASM (`wasm32-unknown-unknown`), JS closures are not `Send`, so the
// async_trait macro must use `?Send`. On native, keep `Send + Sync` so the
// sandbox can be shared across threads in the axum runtime.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait HostSandbox {
    /// Open a sub-browser at `url` and resolve when navigation matches
    /// `capture_url_pattern` (a glob — `*` matches any sequence of chars).
    /// Returns the captured URL so the caller can extract OAuth tokens etc.
    async fn open_browser_sandbox(
        &self,
        url: String,
        capture_url_pattern: String,
    ) -> Result<SandboxResult, SandboxError>;
}

/// V1 stub. Every call returns `Err(NotImplemented)`.
pub struct StubSandbox;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl HostSandbox for StubSandbox {
    async fn open_browser_sandbox(
        &self,
        _url: String,
        _capture_url_pattern: String,
    ) -> Result<SandboxResult, SandboxError> {
        Err(SandboxError::NotImplemented)
    }
}

// ── Wry implementation (Phase A) ────────────────────────────────────────────

#[cfg(feature = "wry-sandbox")]
pub mod wry_sandbox;

#[cfg(feature = "wry-sandbox")]
pub use wry_sandbox::WrySandbox;

// ── Host-cap advertisement ──────────────────────────────────────────────────

/// Returns the list of host-cap variants this build advertises as SUPPORTED.
///
/// - `wry-sandbox` (apps/desktop, Phase A): advertises `SandboxBrowser`.
/// - `web` (apps/web, Phase C): advertises `SandboxBrowser`.
/// - Default: empty (stub).
///
/// The UI reads this list to render mechanism toggles as DISABLED when their
/// `requires-host-cap` isn't present here.
#[must_use]
pub fn advertised_host_caps() -> &'static [HostCap] {
    #[cfg(any(feature = "wry-sandbox", feature = "web"))]
    {
        &[HostCap::SandboxBrowser]
    }
    #[cfg(not(any(feature = "wry-sandbox", feature = "web")))]
    {
        &[]
    }
}

/// Match a URL against a glob pattern.
/// `*` in the pattern matches any sequence of characters.
/// This is intentionally simple: no `?` or `[...]` support.
#[must_use]
pub fn glob_matches(pattern: &str, url: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == url;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut rest = url;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if let Some(tail) = rest.strip_prefix(part) {
                rest = tail;
            } else {
                return false;
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else if let Some(pos) = rest.find(part) {
            rest = &rest[pos + part.len()..];
        } else {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[tokio::test]
    async fn stub_returns_not_implemented() {
        let s = StubSandbox;
        let r = s
            .open_browser_sandbox(
                "https://example.com".into(),
                "*example.com*".into(),
            )
            .await;
        assert!(matches!(r, Err(SandboxError::NotImplemented)));
    }

    #[test]
    #[cfg(not(any(feature = "wry-sandbox", feature = "web")))]
    fn stub_advertises_empty_host_caps() {
        assert!(advertised_host_caps().is_empty());
    }

    #[test]
    #[cfg(any(feature = "wry-sandbox", feature = "web"))]
    fn feature_advertises_sandbox_browser_cap() {
        assert!(advertised_host_caps().contains(&HostCap::SandboxBrowser));
    }

    #[test]
    fn glob_matches_works() {
        assert!(glob_matches("*example.com*", "http://example.com/foo"));
        // Original test used the pattern "*//captured*" which requires the
        // literal substring "//captured" (no intervening text) — that's not in
        // the URL ("//127.0.0.1/captured" has the host between // and /captured).
        // The pattern needs a second `*` to match intervening host text.
        assert!(glob_matches("*//*captured*", "http://127.0.0.1/captured?token=abc"));
        assert!(!glob_matches("*//*captured*", "http://example.com/other"));
        assert!(glob_matches("exact", "exact"));
        assert!(!glob_matches("exact", "notexact"));
        assert!(glob_matches("http*token=abc", "http://localhost/captured?token=abc"));
    }
}
