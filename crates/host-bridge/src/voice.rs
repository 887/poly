//! # `/host/voice/*` — DELETED
//!
//! The old Discord-coupled voice bridge routes (`/host/voice/connect`,
//! `/host/voice/send_audio`, etc.) have been removed. Their protocol logic
//! (Discord WS handshake, op-codes, RTP framing, AEAD) lives in
//! `clients/discord/src/voice_bridge.rs`.
//!
//! The generic host primitives that replaced them are:
//!
//! - `/host/udp/*`         — `crates/host-bridge/src/udp.rs`
//! - `/host/codec/opus/*`  — `crates/host-bridge/src/codec_opus.rs`
//! - `/host/aead/*`        — `crates/host-bridge/src/aead.rs`
//!
//! This file is kept as a tombstone so references in `jj log` and plan-doc
//! change IDs stay searchable. It exports nothing.
