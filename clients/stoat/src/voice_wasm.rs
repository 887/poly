//! Stoat (Vortex) voice transport — WASM target.
//!
//! Sibling to `clients/stoat/src/voice.rs` (native). This file is `#[cfg(target_arch = "wasm32")]`
//! at the module declaration in `lib.rs`, mirroring discord's `voice_bridge.rs`
//! pattern (Phase X of plan-voice-media-plane-e2e.md).
//!
//! # Status
//!
//! STUB — Phase B of `docs/plans/plan-stoat-voice-wasm.md` will fill in:
//!
//! - `gloo_net::websocket::futures::WebSocket` Vortex signaling loop
//! - `/host/codec/opus/*` encoder/decoder sessions (no `audiopus` FFI)
//! - `MediaStreamTrackProcessor` mic capture (lift from `clients/discord/src/voice_bridge/audio_capture.rs`)
//! - `AudioContext` + `AudioBufferSourceNode` speaker playback
//! - Per-user `OpusDecoder` keyed off the 8-byte ASCII user-id prefix on each
//!   binary WS frame
//!
//! Until Phase B lands, this module exists only so that Phase A.3's
//! architectural decision (sibling file) is materialized in the tree and
//! parallel B-phase sonnet agents can edit `voice_wasm.rs` independently
//! without colliding on `voice.rs`'s outer `#[cfg(feature = "voice")]` arm.

/// Placeholder symbol so that downstream `pub use voice_wasm::…` references
/// don't need to be deleted-and-re-added when B.1 lands.
///
/// Phase B.1 will replace this with `pub async fn connect_voice_wasm(…)`.
pub fn placeholder() {}
