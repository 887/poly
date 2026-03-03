//! Account-specific settings sub-modules.
//!
//! Components here are scoped to a single account and live under
//! `settings/account/` to clearly separate them from app-level settings
//! such as theme, language, backup, and identity.

pub(super) mod notifications;
pub(super) use notifications::NotificationsSettings;
