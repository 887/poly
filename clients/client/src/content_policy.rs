//! `ContentPolicyBackend` capability sub-trait (Phase H.1).
//!
//! Carved out of [`ClientBackend`] in Phase H.1.  No backend currently
//! implements this — every implementation in the pre-H.1 world returned the
//! `NotSupported` / `Ok(vec![])` default.  The methods live here so that a
//! future backend can opt in by implementing this trait and overriding
//! [`IsBackend::as_content_policy`] to return `Some(self)`.
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(cp) = backend.as_content_policy() {
//!     let policy = cp.get_content_policy().await?;
//!     // …
//! }
//! ```
//!
//! [`ClientBackend`]: crate::ClientBackend
//! [`IsBackend::as_content_policy`]: crate::IsBackend::as_content_policy

use async_trait::async_trait;

use crate::{BlockedUser, ClientResult, ContentPolicy};

/// Capability sub-trait for content and social policy settings.
///
/// WIT note: there is currently no `poly:client/content-policy` WIT interface
/// — the three methods exist as a pure Rust-side contract.  If a WIT interface
/// is added in the future, this trait MUST mirror its surface exactly to keep
/// the plugin-host bridge in sync.
///
/// No default impls: presence of `impl ContentPolicyBackend` is the opt-in
/// signal.  Backends that do not support content policy simply leave
/// [`IsBackend::as_content_policy`] returning `None` (the default).
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ContentPolicyBackend: Send + Sync {
    /// Get the account's content and social policy settings.
    ///
    /// Returns [`crate::ClientError::NotSupported`] if the backend does not
    /// expose content policy settings — callers should fall back to
    /// locally-stored defaults.
    async fn get_content_policy(&self) -> ClientResult<ContentPolicy>;

    /// Update the account's content and social policy settings.
    async fn set_content_policy(&self, policy: ContentPolicy) -> ClientResult<()>;

    /// Get the list of users blocked by the authenticated user.
    async fn get_blocked_users(&self) -> ClientResult<Vec<BlockedUser>>;
}
