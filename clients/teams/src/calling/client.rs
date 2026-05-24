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
}
