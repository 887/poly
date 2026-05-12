//! Voice state slice — participants, connections, and media settings.
//!
//! Extracted from `ChatData` so that voice writes (participant list ticks,
//! mic/mute toggles) only re-render voice-watching components rather than
//! the entire chat list. Provided alongside `BatchedSignal<ChatData>` at
//! the `App` level (see `crates/core/src/ui.rs`).
//!
//! # Hang-class notes
//! - Use `.batch(|v| …)` for writes (hang class #1: no raw `Signal::write()`).
//! - Use `.peek()` for one-shot snapshots that must not subscribe the parent.
//! - Use `.set_if_changed()` / `batch_if_changed()` for effect-self-write
//!   patterns (hang class #8).

use poly_client::{VoiceConnection, VoiceParticipant};
use std::collections::HashMap;

pub use crate::state::chat_data::VoiceMediaSettings;

/// Runtime voice state: participants, active/held connections, media settings.
///
/// Held as `BatchedSignal<VoiceState>` in context so voice writes don't
/// re-render chat list or other non-voice subscribers.
#[derive(Debug, Clone, Default)]
pub struct VoiceState {
    /// Participants in each voice channel, keyed by channel ID.
    pub voice_channel_participants: HashMap<String, Vec<VoiceParticipant>>,
    /// The local user's current voice connection (`None` if not in a call).
    pub voice_connection: Option<VoiceConnection>,
    /// Voice calls currently on hold.
    ///
    /// Poly only renders one active call at a time, but temporary direct calls
    /// can suspend the previously active call (similar to Teams/Discord) so
    /// the user can swap back later.
    pub held_voice_connections: Vec<VoiceConnection>,
    /// Voice and audio device settings (noise cancel, mic/speaker selection).
    pub voice_media_settings: VoiceMediaSettings,
    /// C.4 — per-channel speaking indicator map.
    ///
    /// Outer key: channel_id. Inner key: user_id. Value: `true` when the
    /// participant is currently speaking (op 5 bitmask non-zero).
    ///
    /// Updated via `BatchedSignal::set_if_changed` to avoid hang class #8
    /// (self-firing effect when the speaking map update re-notifies the
    /// subscriber that triggered the update).
    pub voice_speaking_map: HashMap<String, HashMap<String, bool>>,
}
