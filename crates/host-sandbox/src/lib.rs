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

#[async_trait::async_trait]
pub trait HostSandbox: Send + Sync {
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

#[async_trait::async_trait]
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
/// SUPPORTED. v1: empty (no real sandbox impl). The UI Phase F reads
/// this to render mechanism toggles as DISABLED when their
/// `requires-host-cap` isn't in this list.
pub fn advertised_host_caps() -> &'static [HostCap] {
    &[]
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
    fn v1_advertises_empty_host_caps() {
        assert!(advertised_host_caps().is_empty());
    }
}
