//! Per-trait `impl ... for DiscordClient` blocks — SOLID B.1 split.
//!
//! Originally one 3 582-line `lib.rs`; each trait implementation now lives in
//! its own sibling file so the file boundary mirrors the trait boundary
//! (SRP — one file, one reason to change).
//!
//! `lib.rs` still owns the struct definition, constructors, mappers, and the
//! native gateway loop. Everything here is **purely structural** — no
//! behaviour change relative to the pre-split state.

#[cfg(feature = "native")]
mod context_action;
#[cfg(feature = "native")]
mod dms_groups;
#[cfg(feature = "native")]
mod forum;
#[cfg(feature = "native")]
mod is_backend;
#[cfg(feature = "native")]
mod messaging;
#[cfg(feature = "native")]
mod moderation;
#[cfg(feature = "native")]
mod server_admin;
#[cfg(feature = "native")]
mod settings;
#[cfg(feature = "native")]
mod social_graph;
#[cfg(feature = "native")]
mod threads;
#[cfg(feature = "native")]
mod view_descriptor;
#[cfg(feature = "native")]
mod voice_transport;
