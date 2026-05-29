//! `impl SocialGraphBackend for ForgejoClient` — user lookup + presence.
//!
//! Tier 2 (`plan-trait-split-readable-vs-writable.md`): all friend /
//! block / ignore / presence-set stubs dropped — Forgejo does NOT
//! implement [`WritableSocialGraphBackend`], so the read-trait shims
//! return `NotSupported` automatically.

use async_trait::async_trait;
use poly_client::{ClientResult, User, PresenceStatus, BackendType};
use crate::{ForgejoClient};
use crate::mapping::BACKEND_SLUG;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SocialGraphBackend for ForgejoClient {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        Ok(User {
            id: id.to_string(),
            display_name: id.to_string(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: BackendType::from(BACKEND_SLUG),
        })
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        Ok(Vec::new())
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }
}
