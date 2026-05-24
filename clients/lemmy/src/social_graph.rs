//! `impl SocialGraphBackend for LemmyClient` — user lookup + presence.
//!
//! Lemmy has no friends concept and no presence model.  Tier 2
//! (`plan-trait-split-readable-vs-writable.md`): all friend / block /
//! ignore / presence-set stubs dropped — Lemmy does NOT implement
//! [`WritableSocialGraphBackend`], so the read-trait shims return
//! `NotSupported` automatically.

use async_trait::async_trait;
use poly_client::*;

use crate::LemmyClient;

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

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }
}
