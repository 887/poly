//! `SocialGraphBackend` + `WritableSocialGraphBackend` for `StoatClient`.
//!
//! Stoat (Revolt) has a native friend / relationship system, block
//! support, and per-user presence. It has no per-friend-nickname or
//! user-note endpoint.
//!
//! Tier 2 (`plan-trait-split-readable-vs-writable.md`):
//! `WritableSocialGraphBackend` carries every mutator; reads stay on
//! `SocialGraphBackend`.

use crate::api::StoatRelationshipStatus;
use async_trait::async_trait;
use futures::future;
use poly_client::{ClientError, ClientResult, PresenceStatus, User};

use super::StoatClient;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SocialGraphBackend for StoatClient {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        let (user, root_config) =
            future::try_join(self.http.fetch_user(id), self.http.fetch_server_config()).await?;
        Ok(user.into_poly_user_with_autumn(root_config.autumn_base_url()))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        let (self_user, root_config) =
            future::try_join(self.http.fetch_self(), self.http.fetch_server_config()).await?;
        let autumn_base_url = root_config.autumn_base_url();

        let mut friends: Vec<User> = future::try_join_all(
            self_user
                .relations
                .into_iter()
                .filter(|relation| relation.status == StoatRelationshipStatus::Friend)
                .map(|relation| async move { self.http.fetch_user(&relation.user_id).await }),
        )
        .await?
        .into_iter()
        .map(|user| user.into_poly_user_with_autumn(autumn_base_url))
        .collect();

        friends.sort_by(|left, right| {
            left.display_name
                .cmp(&right.display_name)
                .then_with(|| left.id.cmp(&right.id))
        });

        Ok(friends)
    }

    async fn get_presence(&self, user_id: &str) -> ClientResult<PresenceStatus> {
        let user = self.http.fetch_user(user_id).await?;
        Ok(user.into_poly_user().presence)
    }

    fn as_writable_social_graph(
        &self,
    ) -> Option<&dyn poly_client::WritableSocialGraphBackend> {
        Some(self)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::WritableSocialGraphBackend for StoatClient {
    async fn add_friend(&self, user_id: &str) -> ClientResult<()> {
        self.http.add_friend(user_id).await
    }

    async fn remove_friend(&self, user_id: &str) -> ClientResult<()> {
        self.http.remove_friend_by_id(user_id).await
    }

    async fn respond_to_friend_request(&self, user_id: &str, accept: bool) -> ClientResult<()> {
        if accept {
            let _user = self.http.accept_friend_request(user_id).await?;
        } else {
            let _user = self.http.remove_friend(user_id).await?;
        }
        Ok(())
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        // Revolt/Stoat has no per-friend nickname endpoint.
        Err(ClientError::NotSupported(
            "set_friend_nickname: Stoat has no per-friend nickname endpoint".to_string(),
        ))
    }

    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        // Revolt/Stoat has no user-note endpoint.
        Err(ClientError::NotSupported(
            "set_user_note: Stoat has no user-note endpoint".to_string(),
        ))
    }

    async fn block_user(&self, user_id: &str) -> ClientResult<()> {
        self.http.block_user(user_id).await
    }

    async fn unblock_user(&self, user_id: &str) -> ClientResult<()> {
        self.http.unblock_user(user_id).await
    }

    /// Stoat has no separate "ignore" concept distinct from block.
    ///
    /// Decision: map `ignore_user` → `block_user` so the UI action produces a
    /// meaningful effect rather than silently failing.
    async fn ignore_user(&self, user_id: &str) -> ClientResult<()> {
        // TODO(stoat): Stoat has no separate ignore tier; mapped to block.
        self.http.block_user(user_id).await
    }

    /// Reverse of `ignore_user` — maps to unblock for the same reason.
    async fn unignore_user(&self, user_id: &str) -> ClientResult<()> {
        // TODO(stoat): Stoat has no separate ignore tier; mapped to unblock.
        self.http.unblock_user(user_id).await
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Ok(())
    }
}
