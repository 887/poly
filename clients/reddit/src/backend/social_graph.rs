//! `SocialGraphBackend` impl for [`super::RedditBackend`].
//!
//! Reddit has no friend/block/presence system — every social-graph
//! mutation returns `NotSupported`. The one real lookup is `get_user`,
//! which scrapes the user-overview HTML.

use async_trait::async_trait;
use poly_client::{ClientError, ClientResult, PresenceStatus, User};

use super::error::{NS_BLOCK, NS_FRIEND_SYSTEM, NS_IGNORE, NS_PRESENCE, NS_UNBLOCK, NS_USER_NOTE};
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

    async fn add_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_FRIEND_SYSTEM.to_string()))
    }

    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_FRIEND_SYSTEM.to_string()))
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_FRIEND_SYSTEM.to_string()))
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_FRIEND_SYSTEM.to_string()))
    }

    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_USER_NOTE.to_string()))
    }

    async fn block_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_BLOCK.to_string()))
    }

    async fn unblock_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_UNBLOCK.to_string()))
    }

    async fn ignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_IGNORE.to_string()))
    }

    async fn unignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_IGNORE.to_string()))
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_PRESENCE.to_string()))
    }
}
