//! `impl SocialGraphBackend for TeamsClient` — presence, friends stubs.
//! H.3.b: all social-graph-surface methods live here.

use crate::TeamsClient;
#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use poly_client::*;

// ── H.3.b — SocialGraphBackend ────────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SocialGraphBackend for TeamsClient {
    async fn get_user(&self, _id: &str) -> ClientResult<User> {
        // The trait contract is "Ok(User) | Err(NotFound | Network | Auth)".
        // Returning NotFound for "this backend has no user-lookup endpoint"
        // would lie to callers — they'd give up looking elsewhere when in
        // fact the user might exist, just not on Teams. Use NotSupported.
        Err(ClientError::NotSupported("Teams user lookup not supported".into()))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        // LSP: Teams has no friend concept (`add_friend` etc. below return
        // `NotSupported`). Returning `Ok(vec![])` lies to callers ("you have
        // no friends in Teams") instead of disclosing "no such API".
        // SOLID-audit-teams (Phase B.1).
        Err(ClientError::NotSupported(
            "get_friends: Teams has no friend system".into(),
        ))
    }

    async fn add_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams has no friend system".into()))
    }

    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams has no friend system".into()))
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams has no friend system".into()))
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams has no friend system".into()))
    }

    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams has no user note system".into()))
    }

    async fn block_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams: block not supported via this interface".into()))
    }

    async fn unblock_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams: unblock not supported via this interface".into()))
    }

    async fn ignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams has no ignore concept".into()))
    }

    async fn unignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams has no ignore concept".into()))
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, status: PresenceStatus) -> ClientResult<()> {
        let availability = match status {
            PresenceStatus::Online => "Available",
            PresenceStatus::Idle => "Away",
            PresenceStatus::DoNotDisturb => "DoNotDisturb",
            PresenceStatus::Offline
            | PresenceStatus::Invisible
            | PresenceStatus::Unknown => "Offline",
        };
        self.http.set_presence(availability).await
    }
}
