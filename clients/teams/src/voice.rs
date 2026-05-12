//! Teams voice stub — Phase I of `docs/plans/plan-voice-video-calls.md`.
//!
//! Every method in this module returns `ClientError::NotSupported` because
//! Teams calling requires the Azure Communication Services (ACS) / Microsoft
//! Graph Calling SDK, which is not yet implemented.
//!
//! Full implementation will ship in a separate plan (`plan-teams-calling.md`,
//! not yet written). This stub exists so:
//!
//! 1. The UI can display Teams voice-related UI elements without crashing.
//! 2. A DM call attempt produces a clear "not yet supported" error rather than
//!    a missing-method panic or a silent no-op.
//! 3. The pseudo-backend `TemporaryCall` fallback in
//!    `crates/core/src/ui/account/common/direct_call.rs` (Phase D.5) handles
//!    Teams calls gracefully via the `NotSupported` return path.
//!
//! # Surface matched to Discord
//!
//! The public methods on [`TeamsVoiceClient`] mirror the Discord voice surface
//! (`clients/discord/src/voice.rs`) so that backend-dispatch code can treat
//! them uniformly. Add a new method here whenever a matching method is added
//! to the Discord voice module.

use poly_client::ClientError;

/// Teams voice stub.
///
/// All methods return `ClientError::NotSupported` until the ACS calling
/// integration ships in a follow-up plan.
pub struct TeamsVoiceClient;

impl TeamsVoiceClient {
    /// Create a new Teams voice stub.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Attempt to connect to a Teams voice channel.
    ///
    /// Always returns `NotSupported` — Teams calling is not yet implemented.
    pub async fn connect_voice(&self, _channel_id: &str) -> Result<(), ClientError> {
        Err(ClientError::NotSupported(
            "Teams calling is not yet implemented".to_string(),
        ))
    }

    /// Attempt to start a Teams DM voice call.
    ///
    /// Always returns `NotSupported` — Teams calling is not yet implemented.
    pub async fn start_direct_call(&self, _dm_id: &str) -> Result<(), ClientError> {
        Err(ClientError::NotSupported(
            "Teams calling is not yet implemented".to_string(),
        ))
    }

    /// Attempt to disconnect from a Teams voice channel or call.
    ///
    /// Always returns `NotSupported` — Teams calling is not yet implemented.
    pub async fn disconnect_voice(&self) -> Result<(), ClientError> {
        Err(ClientError::NotSupported(
            "Teams calling is not yet implemented".to_string(),
        ))
    }

    /// Toggle the local user's mute state.
    ///
    /// Always returns `NotSupported` — Teams calling is not yet implemented.
    pub async fn set_self_mute(
        &self,
        _channel_id: &str,
        _mute: bool,
        _deaf: bool,
    ) -> Result<(), ClientError> {
        Err(ClientError::NotSupported(
            "Teams calling is not yet implemented".to_string(),
        ))
    }

    /// Retrieve the current voice participants in a Teams channel.
    ///
    /// Returns an empty list — Teams calling is not yet implemented.
    /// The trait default in `clients/client/src/lib.rs` already returns
    /// `Ok(vec![])`, so this method is provided here for symmetry with the
    /// Discord voice module.
    pub async fn get_voice_participants(
        &self,
        _channel_id: &str,
    ) -> Result<Vec<poly_client::VoiceParticipant>, ClientError> {
        Ok(vec![])
    }
}

impl Default for TeamsVoiceClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[tokio::test]
    async fn connect_voice_returns_not_supported() {
        let client = TeamsVoiceClient::new();
        let err = client.connect_voice("some-channel").await.unwrap_err();
        assert!(
            matches!(err, ClientError::NotSupported(_)),
            "expected NotSupported, got {err:?}"
        );
    }

    #[tokio::test]
    async fn start_direct_call_returns_not_supported() {
        let client = TeamsVoiceClient::new();
        let err = client.start_direct_call("some-dm").await.unwrap_err();
        assert!(
            matches!(err, ClientError::NotSupported(_)),
            "expected NotSupported, got {err:?}"
        );
    }

    #[tokio::test]
    async fn get_voice_participants_returns_empty() {
        let client = TeamsVoiceClient::new();
        let participants = client
            .get_voice_participants("some-channel")
            .await
            .unwrap();
        assert!(
            participants.is_empty(),
            "expected empty participants list"
        );
    }
}
