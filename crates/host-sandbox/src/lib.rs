// FUTURE: docs/plans/plan-host-sandbox-impl.md
//! Host-side sandbox host-cap stub.
//!
//! v1: every method returns `Err(SandboxError::NotImplemented)`. The real
//! sandbox plumbing — opening a sub-browser to handle Discord captcha
//! challenges and similar flows — lives in `plan-host-sandbox-impl.md`
//! (future).
//!
//! What we ship today:
//! - The `HostSandbox` trait — defines the host-side contract.
//! - `StubSandbox` — a no-op impl returning `NotImplemented`.
//! - `advertised_host_caps()` — returns the empty list in v1, so any
//!   plugin's `requires-host-cap` declaration causes the mechanism to
//!   render as DISABLED in the UI (Phase F).

use poly_client::HostCap;

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("sandbox not implemented in v1")]
    NotImplemented,
    #[error("invalid url: {0}")]
    InvalidUrl(String),
    #[error("user cancelled")]
    UserCancelled,
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
    /// `capture_url_pattern` (a glob or regex — see impl). Returns the
    /// captured URL fragment so the caller can extract OAuth tokens etc.
    async fn open_browser_sandbox(
        &self,
        url: String,
        capture_url_pattern: String,
    ) -> Result<SandboxResult, SandboxError>;
}

/// V1 stub. Every call returns `Err(NotImplemented)` — the real impl
/// lives in `plan-host-sandbox-impl.md`.
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

/// Returns the list of host-cap variants this build advertises as
/// SUPPORTED. The UI reads this to render mechanism toggles as DISABLED
/// when their `requires-host-cap` isn't in this list.
///
/// - `web` feature enabled (apps/web): advertises `SandboxBrowser` because
///   the browser popup + `/sandbox/<id>` redirect shim is live (Phase C of
///   plan-host-sandbox-impl.md).
/// - No feature: returns empty (stub — Wry/Electron phases not yet complete).
#[must_use]
pub fn advertised_host_caps() -> &'static [HostCap] {
    #[cfg(feature = "web")]
    {
        &[HostCap::SandboxBrowser]
    }
    #[cfg(not(feature = "web"))]
    {
        &[]
    }
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
    #[cfg(not(feature = "web"))]
    fn stub_advertises_empty_host_caps() {
        assert!(advertised_host_caps().is_empty());
    }

    #[test]
    #[cfg(feature = "web")]
    fn web_advertises_sandbox_browser_cap() {
        use poly_client::HostCap;
        assert!(advertised_host_caps().contains(&HostCap::SandboxBrowser));
    }
}
