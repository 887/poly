//! `VoiceTransportBackend` capability sub-trait (Phase C.1 — ISP split).
//!
//! Carved out of [`IsBackend`] in Phase C.1 of
//! `docs/plans/plan-solid-audit-core-state.md`.  Groups the voice/DM-call
//! transport capabilities that only voice-capable backends (`poly-discord`,
//! `poly-stoat`) need to override.  Read-only news feeds (`poly-hackernews`,
//! `poly-forgejo`, `poly-github`, `poly-reddit`, …) do not opt in.
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(vt) = backend.as_voice_transport() {
//!     vt.join_voice_channel_transport(&server_id, &channel_id).await?;
//! }
//! ```
//!
//! The legacy [`IsBackend`] methods (`get_voice_participants`,
//! `join_voice_channel_transport`, `start_dm_call_transport`,
//! `set_voice_mute`) remain as default-delegating shims so existing call
//! sites in `crates/core/` continue to compile unchanged — the default
//! impl consults `as_voice_transport()` and delegates if `Some`, else
//! returns the documented "no-op / `NotSupported`" fallback.
//!
//! [`IsBackend`]: crate::IsBackend
//! [`IsBackend::as_voice_transport`]: crate::IsBackend::as_voice_transport

use async_trait::async_trait;

use crate::{ClientResult, VoiceParticipant};

/// Capability sub-trait for backend voice / DM-call transport signaling.
///
/// No default impls: presence of `impl VoiceTransportBackend` is the opt-in
/// signal.  Backends that don't carry voice capabilities leave
/// [`IsBackend::as_voice_transport`] returning `None` (the default).
///
/// # Liskov contract
///
/// Each method MUST obey the same contract the method had when it lived
/// directly on [`IsBackend`]:
///
/// * `get_voice_participants` — may return empty list for backends with no
///   active call; must not panic.
/// * `join_voice_channel_transport` / `set_voice_mute` — fire-and-forget
///   transport signal; may fail with [`ClientError::Network`] but must not
///   panic.  Pseudo-backend fallback is `Ok(())`.
/// * `start_dm_call_transport` — may fail with [`ClientError::NotSupported`];
///   caller falls back to pseudo-backend.
///
/// [`IsBackend`]: crate::IsBackend
/// [`IsBackend::as_voice_transport`]: crate::IsBackend::as_voice_transport
/// [`ClientError::Network`]: crate::ClientError::Network
/// [`ClientError::NotSupported`]: crate::ClientError::NotSupported
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait VoiceTransportBackend: Send + Sync {
    /// Get the current voice participants in a voice or video channel.
    async fn get_voice_participants(
        &self,
        channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>>;

    /// Signal the backend that the local user is joining a voice channel.
    ///
    /// Default `Ok(())` — pseudo-backend fallback.  Backends with gateway
    /// signaling (Discord op 4) override.
    async fn join_voice_channel_transport(
        &self,
        _server_id: &str,
        _channel_id: &str,
    ) -> ClientResult<()> {
        Ok(())
    }

    /// Initiate a DM call via backend transport (real signaling).
    ///
    /// Default `Err(NotSupported)` — caller falls back to pseudo-backend.
    async fn start_dm_call_transport(&self, _dm_channel_id: &str) -> ClientResult<()> {
        Err(crate::ClientError::NotSupported(
            "start_dm_call_transport".into(),
        ))
    }

    /// Toggle the local user's mute / deafen state on the backend.
    ///
    /// Default `Ok(())` — pseudo-backend fallback.  Backends with gateway
    /// signaling (Discord op 4) override.
    async fn set_voice_mute(
        &self,
        _server_id: &str,
        _channel_id: &str,
        _self_mute: bool,
        _self_deaf: bool,
    ) -> ClientResult<()> {
        Ok(())
    }
}
