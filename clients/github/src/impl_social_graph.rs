use async_trait::async_trait;
use poly_client::*;

use crate::{GitHubClient, NS_NO_FRIEND_SYSTEM, NS_NO_IGNORE_CONCEPT};
use crate::mapping::BACKEND_SLUG;

// ── H.3.b — SocialGraphBackend ────────────────────────────────────────────────

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

    async fn add_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_FRIEND_SYSTEM.to_string()))
    }

    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_FRIEND_SYSTEM.to_string()))
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_FRIEND_SYSTEM.to_string()))
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_FRIEND_SYSTEM.to_string()))
    }

    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub has no user note system".to_string()))
    }

    async fn block_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub: block not supported via this interface".to_string()))
    }

    async fn unblock_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub: unblock not supported via this interface".to_string()))
    }

    async fn ignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_IGNORE_CONCEPT.to_string()))
    }

    async fn unignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_IGNORE_CONCEPT.to_string()))
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Err(ClientError::NotSupported("github has no presence model".to_string()))
    }
}
