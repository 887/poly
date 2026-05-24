//! `SocialGraphBackend` + `WritableSocialGraphBackend` for `DiscordClient`.
//!
//! Discord has friend / block / note / presence support via the public
//! `/users/@me/relationships` API.
//!
//! Tier 2 (`plan-trait-split-readable-vs-writable.md`):
//! `WritableSocialGraphBackend` carries every mutator; reads stay on
//! `SocialGraphBackend`.

use super::super::*;
use async_trait::async_trait;
use poly_client::*;

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SocialGraphBackend for DiscordClient {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        let u = self.http.get_user(id).await?;
        Ok(self.discord_user_to_poly(u))
    }

    /// C.3 — `GET /users/@me/relationships` filtered to accepted friends
    /// (`type == 1`).  Blocked / incoming / outgoing requests are intentionally
    /// excluded here; expose them via dedicated methods if the UI grows the surface.
    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        let rels = self.http.get_relationships().await?;
        Ok(rels
            .into_iter()
            .filter(|r| r.relationship_type == 1)
            .map(|r| self.discord_user_to_poly(r.user))
            .collect())
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    fn as_writable_social_graph(
        &self,
    ) -> Option<&dyn poly_client::WritableSocialGraphBackend> {
        Some(self)
    }
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::WritableSocialGraphBackend for DiscordClient {
    async fn add_friend(&self, user_id: &str) -> ClientResult<()> {
        self.http.put_relationship(user_id, 1).await
    }

    async fn remove_friend(&self, user_id: &str) -> ClientResult<()> {
        self.http.delete_relationship(user_id).await
    }

    async fn respond_to_friend_request(
        &self,
        _user_id: &str,
        _accept: bool,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "respond_to_friend_request: Discord does not expose this endpoint".to_string(),
        ))
    }

    /// Discord does not expose per-friend nicknames via its public API.
    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "set_friend_nickname: Discord does not expose friend nicknames via API".to_string(),
        ))
    }

    /// Set or clear a private note about a user. `None` clears (sends empty string).
    async fn set_user_note(&self, user_id: &str, note: Option<&str>) -> ClientResult<()> {
        self.http.put_user_note(user_id, note.unwrap_or("")).await
    }

    /// Block a user. Sends `PUT /users/@me/relationships/:user_id` with `{"type": 2}`.
    async fn block_user(&self, user_id: &str) -> ClientResult<()> {
        self.http.put_relationship(user_id, 2).await
    }

    /// Unblock a user. Mirrors `block_user` using DELETE on the same endpoint.
    async fn unblock_user(&self, user_id: &str) -> ClientResult<()> {
        self.http.delete_relationship(user_id).await
    }

    /// Discord does not expose a distinct "ignore" concept separate from blocking.
    /// We fall back to block so the action has a real effect rather than silently
    /// dropping the request.
    async fn ignore_user(&self, user_id: &str) -> ClientResult<()> {
        // TODO(discord): Discord has no server-side "ignore" — mapping to block.
        self.http.put_relationship(user_id, 2).await
    }

    /// Reverse of `ignore_user` — same as unblock since we mapped ignore → block.
    async fn unignore_user(&self, user_id: &str) -> ClientResult<()> {
        // TODO(discord): mirroring unblock since ignore maps to block above.
        self.http.delete_relationship(user_id).await
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Ok(())
    }
}
