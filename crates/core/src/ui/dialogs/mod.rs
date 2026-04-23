//! Moderation dialog components (Wave 1 scaffolding).
//!
//! All components in this module are empty stubs. Wave 2/3 backend agents
//! will fill the dialog bodies once the per-backend moderation APIs land.

pub mod ban_member;
pub mod edit_channel;
pub mod kick_member;
pub mod timeout_member;

pub use ban_member::BanMemberDialog;
pub use edit_channel::EditChannelDialog;
pub use kick_member::KickMemberDialog;
pub use timeout_member::TimeoutMemberDialog;
