//! `SocialGraphBackend` impl for [`super::RedditBackend`].
//!
//! Reddit has no friend/block/presence system — the one real lookup is
//! `get_user`, which scrapes the user-overview HTML.
//!
//! Tier 2 (`plan-trait-split-readable-vs-writable.md`): all mutating
//! stubs dropped — Reddit does NOT implement
//! [`WritableSocialGraphBackend`], so the read-trait shims return
//! `NotSupported` automatically.

use async_trait::async_trait;
use poly_client::{ClientError, ClientResult, PresenceStatus, User};

use super::mapping::user_profile_to_user;
use super::RedditBackend;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SocialGraphBackend for RedditBackend {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        let name = id
            .strip_prefix("u_")
            .ok_or_else(|| ClientError::NotFound(format!("user not found: {id}")))?;

        let profile = self
            .client
            .get_user(name)
            .await
            .map_err(ClientError::from)?;

        Ok(user_profile_to_user(&profile, &Self::backend_type()))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        Ok(Vec::new())
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }
}
