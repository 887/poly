//! Channel-scoped UI components (Pack C.3 / P19).
//!
//! Currently just the per-channel settings page. Mirrors
//! `crate::ui::account::server::settings` structurally but scoped to one
//! channel within a server.

pub mod settings;

pub use settings::ChannelSettingsPage;
