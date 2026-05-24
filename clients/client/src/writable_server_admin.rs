//! `WritableServerAdminBackend` capability sub-trait
//! (Tier 2 of `plan-trait-split-readable-vs-writable.md`).
//!
//! Carves the creation / banner-update methods off
//! [`ServerAdminBackend`] so read-leaning backends can drop the
//! `NotSupported` stubs.
//!
//! [`ServerAdminBackend`]: crate::ServerAdminBackend

use async_trait::async_trait;

use crate::{Channel, ChannelType, ClientResult, Server};

/// Capability sub-trait for backends that mutate server / community
/// structure (create new servers / channels, update server banner).
///
/// Opt-in via [`ServerAdminBackend::as_writable_server_admin`] +
/// `impl WritableServerAdminBackend for X`.
///
/// [`ServerAdminBackend`]: crate::ServerAdminBackend
/// [`ServerAdminBackend::as_writable_server_admin`]: crate::ServerAdminBackend::as_writable_server_admin
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait WritableServerAdminBackend: Send + Sync {
    /// Create a new server/guild in this backend.
    async fn create_server(&self, name: &str) -> ClientResult<Server>;

    /// Create a new channel inside a server.
    async fn create_channel(
        &self,
        server_id: &str,
        name: &str,
        channel_type: ChannelType,
    ) -> ClientResult<Channel>;

    /// Update the banner image URL for a server.
    async fn update_server_banner(
        &self,
        server_id: &str,
        banner_url: Option<&str>,
    ) -> ClientResult<()>;
}
