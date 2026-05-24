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
pub(super) mod context_action;
#[cfg(feature = "native")]
pub(super) mod dms_groups;
#[cfg(feature = "native")]
pub(super) mod forum;
#[cfg(feature = "native")]
pub(super) mod is_backend;
#[cfg(feature = "native")]
pub(super) mod messaging;
#[cfg(feature = "native")]
pub(super) mod moderation;
#[cfg(feature = "native")]
pub(super) mod server_admin;
#[cfg(feature = "native")]
pub(super) mod settings;
#[cfg(feature = "native")]
pub(super) mod social_graph;
#[cfg(feature = "native")]
pub(super) mod threads;
#[cfg(feature = "native")]
pub(super) mod view_descriptor;
#[cfg(feature = "native")]
pub(super) mod voice_transport;
