//! `TeamsCallingClient` trait + `StubCallingClient` default impl.
//!
//! Phase A of `docs/plans/plan-teams-calling.md`. The trait surface is
//! the contract the rest of the codebase consumes — once the JS-SDK
//! bridge ships (Phase C), a real impl swaps in without forcing
//! call-site churn.
//!
//! ## Interface segregation
//!
//! Calling has three orthogonal concerns:
//!
//! 1. **Lifecycle** — connect, disconnect, mute/deafen.
//! 2. **Identity** — token bootstrap, ACS-AAD identity mapping.
//! 3. **Participants** — query who's on the call.
//!
//! [`TeamsCallingClient`] groups them with default `NotSupported` impls
//! so a partial impl (token-only, lifecycle-only) is free to leave the
//! rest stubbed. SOLID ISP — consumers depend only on the methods they
//! call (`crates/core` voice UI never asks for participant queries when
//! it's only ringing through to disconnect).

use async_trait::async_trait;
use poly_client::VoiceParticipant;

use super::types::{CallId, CallState, CallingError};

/// Capability trait for Teams / ACS calling. Mirrors the surface of
/// [`crate::voice::TeamsVoiceClient`] one-to-one so the voice-stub can
/// delegate today and the real impl can swap in cleanly tomorrow.
///
/// Default impls return [`CallingError::NotSupported`] so partial
/// implementations are valid (ISP).
#[async_trait]
pub trait TeamsCallingClient: Send + Sync {
    /// Connect to a Teams **channel** voice meeting (channels can host
    /// scheduled or ad-hoc meetings).
    async fn connect_voice(&self, _channel_id: &str) -> Result<CallId, CallingError> {
        Err(CallingError::NotSupported(
            "connect_voice not implemented".into(),
        ))
    }

    /// Place a 1:1 or group **direct call** to a Teams chat (DM).
    async fn start_direct_call(&self, _chat_id: &str) -> Result<CallId, CallingError> {
        Err(CallingError::NotSupported(
            "start_direct_call not implemented".into(),
        ))
    }

    /// Tear down the currently active call.
    async fn disconnect_voice(&self) -> Result<(), CallingError> {
        Err(CallingError::NotSupported(
            "disconnect_voice not implemented".into(),
        ))
    }

    /// Set self mute/deafen.
    async fn set_self_mute(
        &self,
        _channel_id: &str,
        _mute: bool,
        _deaf: bool,
    ) -> Result<(), CallingError> {
        Err(CallingError::NotSupported(
            "set_self_mute not implemented".into(),
        ))
    }

    /// Query current call state. Default returns `Disconnected` so UI
    /// can treat an unimplemented backend as "not in a call".
    async fn call_state(&self, _call_id: &CallId) -> Result<CallState, CallingError> {
        Ok(CallState::Disconnected)
    }

    /// List voice participants in a channel meeting.
    async fn get_voice_participants(
        &self,
        _channel_id: &str,
    ) -> Result<Vec<VoiceParticipant>, CallingError> {
        Ok(vec![])
    }

    /// Accept an incoming inbound call. Teams supports inbound — the
    /// real impl wires this to the JS SDK's `IncomingCall.accept(…)`.
    async fn accept_incoming(&self, _call_id: &CallId) -> Result<(), CallingError> {
        Err(CallingError::NotSupported(
            "accept_incoming not implemented".into(),
        ))
    }

    /// Reject an incoming inbound call.
    async fn reject_incoming(&self, _call_id: &CallId) -> Result<(), CallingError> {
        Err(CallingError::NotSupported(
            "reject_incoming not implemented".into(),
        ))
    }

    // ── Phase C trait surface — JS-bridge plug-in points ───────────────────

    /// Set local mute (audio). Distinct from [`set_self_mute`] which is
    /// the legacy "mute + deafen" combo matching the Discord surface; this
    /// is the granular per-stream API the ACS JS SDK exposes via
    /// `Call.mute()` / `Call.unmute()`.
    ///
    /// Default impl returns [`CallingError::NotImplemented`] — the
    /// `StubCallingClient` (test/UI default) inherits this so the
    /// surface stays callable from UI layers without a panic.
    async fn set_mute(&self, _call_id: &CallId, _muted: bool) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented("set_mute".into()))
    }

    /// Start sending local video on the active call.
    ///
    /// Maps to `Call.startVideo(localVideoStream)` in the ACS JS SDK.
    async fn start_video(&self, _call_id: &CallId) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented("start_video".into()))
    }

    /// Stop sending local video.
    ///
    /// Maps to `Call.stopVideo(localVideoStream)` in the ACS JS SDK.
    async fn stop_video(&self, _call_id: &CallId) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented("stop_video".into()))
    }

    /// Begin screen-share on the active call.
    ///
    /// Maps to `Call.startScreenSharing()` in the ACS JS SDK. The capture
    /// surface (display, window, browser tab) is chosen by the JS side
    /// via `getDisplayMedia` — the Rust API has no input here.
    async fn share_screen(&self, _call_id: &CallId) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented("share_screen".into()))
    }

    /// Stop an in-flight screen-share.
    ///
    /// Maps to `Call.stopScreenSharing()` in the ACS JS SDK.
    async fn stop_screen_share(&self, _call_id: &CallId) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented("stop_screen_share".into()))
    }

    /// Put the local end of the call on hold.
    ///
    /// Maps to `Call.hold()` in the ACS JS SDK. After hold the call
    /// transitions to [`CallState::LocalHold`]; resume with
    /// [`Self::resume_call`].
    async fn hold_call(&self, _call_id: &CallId) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented("hold_call".into()))
    }

    /// Resume a held call.
    ///
    /// Maps to `Call.resume()` in the ACS JS SDK.
    async fn resume_call(&self, _call_id: &CallId) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented("resume_call".into()))
    }
}

/// Default stub — every method returns the trait's default
/// `NotSupported` (or empty) value. Used by [`crate::voice::TeamsVoiceClient`]
/// today.
///
/// Marker struct only — holds no state. The real impl will store the
/// JS-bridge handle + per-call lifecycle state.
#[derive(Debug, Default, Clone, Copy)]
pub struct StubCallingClient;

impl StubCallingClient {
    /// Construct a new stub.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl TeamsCallingClient for StubCallingClient {}

/// WebView-bridged calling client — the Phase C scaffolding for the
/// JS-SDK bridge described in `docs/plans/plan-teams-calling.md`.
///
/// Holds a [`CallingTransport`] handle that will eventually shuttle
/// [`super::ipc::CallingCommand`] frames to a hidden WebView running
/// `@azure/communication-calling`, and demux
/// [`super::ipc::CallingEvent`] frames back.
///
/// Every trait method currently returns
/// [`CallingError::NotImplemented`]. When the JS bridge ships, each
/// method's body becomes a `transport.send(CallingCommand::...).await`
/// + `transport.recv()` pair — no call-site changes required.
///
/// The transport is `Arc<dyn …>` rather than a generic parameter so
/// the type stays object-safe and can be stored in
/// [`crate::TeamsClient`]'s state without leaking generics out to the
/// rest of the codebase (Dependency Inversion).
pub struct WebViewBridgeCallingClient {
    transport: std::sync::Arc<dyn super::ipc::CallingTransport>,
}

impl std::fmt::Debug for WebViewBridgeCallingClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebViewBridgeCallingClient")
            .field("transport", &"<dyn CallingTransport>")
            .finish()
    }
}

impl WebViewBridgeCallingClient {
    /// Construct a new bridge over the given transport.
    #[must_use]
    pub fn new(transport: std::sync::Arc<dyn super::ipc::CallingTransport>) -> Self {
        Self { transport }
    }

    /// Expose the transport handle — useful for tests that need to
    /// flush mock events or assert on sent commands without going
    /// through the trait surface.
    #[must_use]
    pub fn transport(&self) -> &std::sync::Arc<dyn super::ipc::CallingTransport> {
        &self.transport
    }
}

#[async_trait]
impl TeamsCallingClient for WebViewBridgeCallingClient {
    async fn connect_voice(&self, _channel_id: &str) -> Result<CallId, CallingError> {
        Err(CallingError::NotImplemented(
            "WebView bridge: JS-side connectVoice not wired (Phase C)".into(),
        ))
    }

    async fn start_direct_call(&self, _chat_id: &str) -> Result<CallId, CallingError> {
        Err(CallingError::NotImplemented(
            "WebView bridge: JS-side startCall not wired (Phase C)".into(),
        ))
    }

    async fn disconnect_voice(&self) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented(
            "WebView bridge: JS-side hangUp not wired (Phase C)".into(),
        ))
    }

    async fn set_self_mute(
        &self,
        _channel_id: &str,
        _mute: bool,
        _deaf: bool,
    ) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented(
            "WebView bridge: JS-side mute not wired (Phase C)".into(),
        ))
    }

    async fn set_mute(&self, _call_id: &CallId, _muted: bool) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented(
            "WebView bridge: JS-side mute not wired (Phase C)".into(),
        ))
    }

    async fn start_video(&self, _call_id: &CallId) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented(
            "WebView bridge: JS-side startVideo not wired (Phase C)".into(),
        ))
    }

    async fn stop_video(&self, _call_id: &CallId) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented(
            "WebView bridge: JS-side stopVideo not wired (Phase C)".into(),
        ))
    }

    async fn share_screen(&self, _call_id: &CallId) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented(
            "WebView bridge: JS-side startScreenSharing not wired (Phase C)".into(),
        ))
    }

    async fn stop_screen_share(&self, _call_id: &CallId) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented(
            "WebView bridge: JS-side stopScreenSharing not wired (Phase C)".into(),
        ))
    }

    async fn hold_call(&self, _call_id: &CallId) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented(
            "WebView bridge: JS-side hold not wired (Phase C)".into(),
        ))
    }

    async fn resume_call(&self, _call_id: &CallId) -> Result<(), CallingError> {
        Err(CallingError::NotImplemented(
            "WebView bridge: JS-side resume not wired (Phase C)".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[tokio::test]
    async fn stub_connect_voice_returns_not_supported() {
        let c = StubCallingClient::new();
        let err = c.connect_voice("ch").await.unwrap_err();
        assert!(matches!(err, CallingError::NotSupported(_)));
    }

    #[tokio::test]
    async fn stub_start_direct_call_returns_not_supported() {
        let c = StubCallingClient::new();
        let err = c.start_direct_call("dm").await.unwrap_err();
        assert!(matches!(err, CallingError::NotSupported(_)));
    }

    #[tokio::test]
    async fn stub_disconnect_returns_not_supported() {
        let c = StubCallingClient::new();
        let err = c.disconnect_voice().await.unwrap_err();
        assert!(matches!(err, CallingError::NotSupported(_)));
    }

    #[tokio::test]
    async fn stub_call_state_returns_disconnected() {
        let c = StubCallingClient::new();
        let s = c.call_state(&CallId::from("x")).await.unwrap();
        assert_eq!(s, CallState::Disconnected);
    }

    #[tokio::test]
    async fn stub_get_voice_participants_returns_empty() {
        let c = StubCallingClient::new();
        let p = c.get_voice_participants("ch").await.unwrap();
        assert!(p.is_empty());
    }

    #[tokio::test]
    async fn stub_accept_incoming_returns_not_supported() {
        let c = StubCallingClient::new();
        let err = c.accept_incoming(&CallId::from("x")).await.unwrap_err();
        assert!(matches!(err, CallingError::NotSupported(_)));
    }

    #[tokio::test]
    async fn stub_reject_incoming_returns_not_supported() {
        let c = StubCallingClient::new();
        let err = c.reject_incoming(&CallId::from("x")).await.unwrap_err();
        assert!(matches!(err, CallingError::NotSupported(_)));
    }

    #[tokio::test]
    async fn stub_set_self_mute_returns_not_supported() {
        let c = StubCallingClient::new();
        let err = c.set_self_mute("ch", true, false).await.unwrap_err();
        assert!(matches!(err, CallingError::NotSupported(_)));
    }

    // ── Phase C trait surface defaults ─────────────────────────────────

    #[tokio::test]
    async fn stub_set_mute_returns_not_implemented() {
        let c = StubCallingClient::new();
        let err = c.set_mute(&CallId::from("x"), true).await.unwrap_err();
        assert!(matches!(err, CallingError::NotImplemented(_)));
    }

    #[tokio::test]
    async fn stub_start_video_returns_not_implemented() {
        let c = StubCallingClient::new();
        let err = c.start_video(&CallId::from("x")).await.unwrap_err();
        assert!(matches!(err, CallingError::NotImplemented(_)));
    }

    #[tokio::test]
    async fn stub_stop_video_returns_not_implemented() {
        let c = StubCallingClient::new();
        let err = c.stop_video(&CallId::from("x")).await.unwrap_err();
        assert!(matches!(err, CallingError::NotImplemented(_)));
    }

    #[tokio::test]
    async fn stub_share_screen_returns_not_implemented() {
        let c = StubCallingClient::new();
        let err = c.share_screen(&CallId::from("x")).await.unwrap_err();
        assert!(matches!(err, CallingError::NotImplemented(_)));
    }

    #[tokio::test]
    async fn stub_stop_screen_share_returns_not_implemented() {
        let c = StubCallingClient::new();
        let err = c.stop_screen_share(&CallId::from("x")).await.unwrap_err();
        assert!(matches!(err, CallingError::NotImplemented(_)));
    }

    #[tokio::test]
    async fn stub_hold_call_returns_not_implemented() {
        let c = StubCallingClient::new();
        let err = c.hold_call(&CallId::from("x")).await.unwrap_err();
        assert!(matches!(err, CallingError::NotImplemented(_)));
    }

    #[tokio::test]
    async fn stub_resume_call_returns_not_implemented() {
        let c = StubCallingClient::new();
        let err = c.resume_call(&CallId::from("x")).await.unwrap_err();
        assert!(matches!(err, CallingError::NotImplemented(_)));
    }

    // ── WebViewBridgeCallingClient ─────────────────────────────────────

    #[tokio::test]
    async fn webview_bridge_connect_voice_not_implemented() {
        let t = std::sync::Arc::new(super::super::ipc::MockCallingTransport::new());
        let c = WebViewBridgeCallingClient::new(t);
        let err = c.connect_voice("ch").await.unwrap_err();
        assert!(matches!(err, CallingError::NotImplemented(_)));
    }

    #[tokio::test]
    async fn webview_bridge_set_mute_not_implemented() {
        let t = std::sync::Arc::new(super::super::ipc::MockCallingTransport::new());
        let c = WebViewBridgeCallingClient::new(t);
        let err = c.set_mute(&CallId::from("x"), true).await.unwrap_err();
        assert!(matches!(err, CallingError::NotImplemented(_)));
    }

    #[tokio::test]
    async fn webview_bridge_hold_resume_not_implemented() {
        let t = std::sync::Arc::new(super::super::ipc::MockCallingTransport::new());
        let c = WebViewBridgeCallingClient::new(t);
        assert!(matches!(
            c.hold_call(&CallId::from("x")).await.unwrap_err(),
            CallingError::NotImplemented(_)
        ));
        assert!(matches!(
            c.resume_call(&CallId::from("x")).await.unwrap_err(),
            CallingError::NotImplemented(_)
        ));
    }

    #[tokio::test]
    async fn webview_bridge_video_screen_not_implemented() {
        let t = std::sync::Arc::new(super::super::ipc::MockCallingTransport::new());
        let c = WebViewBridgeCallingClient::new(t);
        for err in [
            c.start_video(&CallId::from("x")).await.unwrap_err(),
            c.stop_video(&CallId::from("x")).await.unwrap_err(),
            c.share_screen(&CallId::from("x")).await.unwrap_err(),
            c.stop_screen_share(&CallId::from("x")).await.unwrap_err(),
        ] {
            assert!(matches!(err, CallingError::NotImplemented(_)));
        }
    }

    #[test]
    fn webview_bridge_transport_accessor() {
        let t = std::sync::Arc::new(super::super::ipc::MockCallingTransport::new());
        let c = WebViewBridgeCallingClient::new(t);
        // Just assert the accessor compiles and the Arc strong count is sane.
        assert!(std::sync::Arc::strong_count(c.transport()) >= 1);
    }
}
