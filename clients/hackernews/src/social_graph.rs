//! `SocialGraphBackend` implementation for `HackerNewsClient`.
//!
//! HN has no friend, block, ignore, or presence concepts. All mutating
//! operations return `NotSupported`; `get_user` resolves via the Firebase API.

use async_trait::async_trait;
use poly_client::*;

use crate::HackerNewsClient;
use crate::mapping::hn_user_to_user;

// ── NotSupported constants ───────────────────────────────────────────────────

const ERR_NO_FRIENDS: &str = "Hacker News has no friend system";
const ERR_NO_USER_NOTES: &str = "Hacker News has no user note system";
const ERR_NO_BLOCK: &str = "Hacker News: block not supported via this interface";
const ERR_NO_UNBLOCK: &str = "Hacker News: unblock not supported via this interface";
const ERR_NO_IGNORE: &str = "Hacker News has no ignore concept";
const ERR_NO_PRESENCE: &str = "Hacker News has no presence system";

// ── H.3.b — SocialGraphBackend ───────────────────────────────────────────────

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

    async fn add_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_FRIENDS.to_string()))
    }

    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_FRIENDS.to_string()))
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_FRIENDS.to_string()))
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_FRIENDS.to_string()))
    }

    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_USER_NOTES.to_string()))
    }

    async fn block_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_BLOCK.to_string()))
    }

    async fn unblock_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_UNBLOCK.to_string()))
    }

    async fn ignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_IGNORE.to_string()))
    }

    async fn unignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_IGNORE.to_string()))
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        // HN has no presence concept. Returning Ok(Offline) used to lie to
        // the UI (presence dot would show grey "offline" forever); use
        // Unknown so the dot is suppressed entirely. set_presence already
        // returns NotSupported below — read/write are now consistent.
        Ok(PresenceStatus::Unknown)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_PRESENCE.to_string()))
    }
}
