//! `SocialGraphBackend` impl for `GitHubClient`.
//!
//! GitHub has no friend / block / ignore / presence concepts (as a chat
//! backend).  Tier 2 (`plan-trait-split-readable-vs-writable.md`): all
//! mutating method stubs dropped — GitHub does NOT implement
//! [`WritableSocialGraphBackend`], so the read-trait shims return
//! `NotSupported` automatically.

use async_trait::async_trait;
use poly_client::{ClientResult, User, PresenceStatus, BackendType};

use crate::GitHubClient;
use crate::mapping::BACKEND_SLUG;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SocialGraphBackend for GitHubClient {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        Ok(User {
            id: id.to_string(),
            display_name: id.to_string(),
            avatar_url: Some(format!("https://github.com/{id}.png")),
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
