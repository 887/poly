//! Moderation dialog components (Wave 2 implementation).
//!
//! All components are functional overlays that call backend moderation APIs.

pub mod ban_member;
pub mod edit_channel;
pub mod kick_member;
pub mod timeout_member;

pub use ban_member::BanMemberDialog;
pub use edit_channel::EditChannelDialog;
pub use kick_member::KickMemberDialog;
pub use timeout_member::TimeoutMemberDialog;
