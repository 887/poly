//! `SocialGraphBackend` implementation for `HackerNewsClient`.
//!
//! HN has no friend, block, ignore, or presence concepts.  The read
//! methods resolve `get_user` via the Firebase API and return
//! `Unknown` for `get_presence` (so the UI suppresses the dot).
//!
//! Tier 2 (`plan-trait-split-readable-vs-writable.md`): all mutating
//! methods (`add_friend`, `block_user`, …) are dropped — HN does not
//! implement [`WritableSocialGraphBackend`], so the read-trait's
//! default shims return `NotSupported` automatically.

use async_trait::async_trait;
use poly_client::{ClientResult, User, ClientError, PresenceStatus};

use crate::HackerNewsClient;
use crate::mapping::hn_user_to_user;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SocialGraphBackend for HackerNewsClient {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        let hn_user = self
            .api
            .get_user(id)
            .await?
            .ok_or_else(|| ClientError::NotFound(format!("user not found: {id}")))?;
        Ok(hn_user_to_user(&hn_user))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        Ok(Vec::new())
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        // HN has no presence concept. Use Unknown so the UI suppresses
        // the dot entirely. set_presence is handled by the read-trait's
        // shim (returns NotSupported via no WritableSocialGraphBackend).
        Ok(PresenceStatus::Unknown)
    }
}
