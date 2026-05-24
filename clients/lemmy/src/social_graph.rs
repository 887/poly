//! `impl SocialGraphBackend for LemmyClient` — friends/blocks/presence stubs (H.3.b).
//!
//! Lemmy has no friends system; most methods return `NotSupported`.
//! Split out of `lib.rs` for Single Responsibility (B.1).

use async_trait::async_trait;
use poly_client::*;

use crate::{FRIEND_SYS_UNSUPPORTED, IGNORE_UNSUPPORTED, LemmyClient};

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SocialGraphBackend for LemmyClient {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        // id is `lemmy-user-{n}` — we return a minimal user from session if it matches,
        // otherwise return an error (full user fetch is not needed for the current scope).
        if let Some(session) = self.http.session() {
            let own_id = format!("lemmy-user-{}", session.user_id);
            if id == own_id {
                return Ok(User {
                    id: own_id,
                    display_name: session.user_display_name,
                    avatar_url: session.user_avatar_url,
                    presence: PresenceStatus::Online,
                    backend: BackendType::from(crate::SLUG),
                });
            }
        }
        Err(ClientError::NotFound(format!("user not found: {id}")))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        // Lemmy has no friends concept
        Ok(vec![])
    }

    async fn add_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(FRIEND_SYS_UNSUPPORTED.to_string()))
    }

    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(FRIEND_SYS_UNSUPPORTED.to_string()))
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported(FRIEND_SYS_UNSUPPORTED.to_string()))
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(FRIEND_SYS_UNSUPPORTED.to_string()))
    }

    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no user note system".to_string()))
    }

    async fn block_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy: block not supported via this interface".to_string()))
    }

    async fn unblock_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy: unblock not supported via this interface".to_string()))
    }

    async fn ignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(IGNORE_UNSUPPORTED.to_string()))
    }

    async fn unignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(IGNORE_UNSUPPORTED.to_string()))
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no presence system".to_string()))
    }
}
