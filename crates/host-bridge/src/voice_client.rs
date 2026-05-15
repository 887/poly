//! # `voice_client` — DELETED
//!
//! The old `VoiceBridgeClient` (a Discord-shaped typed client for the now-removed
//! `/host/voice/*` routes) has been removed. Its replacement lives in
//! `clients/discord/src/voice_bridge.rs`, which drives the Discord voice protocol
//! directly using the generic host primitives:
//!
//! - [`crate::udp_client::UdpClient`] — raw UDP send/recv
//! - [`crate::codec_opus_client::OpusClient`] — Opus encode/decode
//! - [`crate::aead_client::AeadClient`] — AEAD encrypt/decrypt
//!
//! This file is a tombstone. It exports nothing.
//!
//! `voice_wire::VoiceEvent` is still available for SSE event parsing
//! (the discord plugin emits VoiceEvent variants over its own SSE stream
//! once it owns the encode/decode loop).
