//! `SocialGraphBackend` + `WritableSocialGraphBackend` for `MatrixClient`.
//!
//! Matrix has partial social-graph support: block/ignore map to
//! `m.ignored_user_list` account data; the friend concept does not
//! exist.  `get_user` uses the profile endpoint.
//!
//! Tier 2 (`plan-trait-split-readable-vs-writable.md`):
//! `WritableSocialGraphBackend` carries the real `block_user` /
//! `unblock_user` / `ignore_user` / `unignore_user` / `set_presence`
//! impls; friend-system methods drop to the read-trait shim's
//! `NotSupported` (Matrix has no friends).

use poly_client::{ClientResult, User, PresenceStatus, BackendType, ClientError};

use crate::api;
use crate::mxc_to_http_thumbnail;
use crate::MatrixClient;

// ── H.3.b — SocialGraphBackend (reads + writable accessor) ───────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl poly_client::SocialGraphBackend for MatrixClient {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        let profile = self.http.fetch_profile(id).await?;
        let avatar_url = profile
            .avatar_url
            .as_deref()
            .map(|url| mxc_to_http_thumbnail(url, self.homeserver_url()));
        Ok(User {
            id: id.to_string(),
            display_name: profile.displayname.unwrap_or_else(|| id.to_string()),
            avatar_url,
            presence: PresenceStatus::Offline,
            backend: BackendType::from(crate::SLUG),
        })
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        // LSP: Matrix has no friend concept.  Returning `Ok(vec![])` lies to
        // callers ("you have no friends here") versus surfacing the truth
        // ("this backend doesn't have friends at all").  Every other method in
        // this impl returns `NotSupported`; align with that contract.
        // SOLID-audit-matrix (Phase B.2).
        Err(ClientError::NotSupported(
            "get_friends: Matrix has no native friend concept".to_string(),
        ))
    }

    /// Fetch a user's presence via
    /// `GET /_matrix/client/v3/presence/{userId}/status`.
    ///
    /// Maps Matrix `presence` strings (`online`, `unavailable`, `offline`) onto
    /// the host `PresenceStatus` enum. `unavailable` is treated as `Idle`
    /// (idle/away in spec language). When the homeserver reports the user as
    /// `online` but `currently_active = false`, surface `Idle` as well — that
    /// captures the "still logged in but away from keyboard" case Matrix
    /// otherwise hides behind a single `online` string.
    ///
    /// Federation-aware: the homeserver fetches presence from the user's
    /// home server when the queried user is remote. Soft-failure: if the
    /// homeserver has presence disabled (some servers do for privacy) and
    /// returns 404/403, surface `NotSupported` so the UI hides the dot
    /// instead of misrepresenting state as `Offline`.
    /// SOLID-audit-matrix (Phase D.2).
    async fn get_presence(&self, user_id: &str) -> ClientResult<PresenceStatus> {
        match self.http.get_presence(user_id).await {
            Ok(resp) => Ok(map_matrix_presence(&resp)),
            // Some homeservers return 404 or 403 when presence is disabled
            // server-wide. Treat these as "not supported on this homeserver"
            // rather than "offline" so the UI hides the dot.
            Err(ClientError::NotFound(_) | ClientError::PermissionDenied(_)) => {
                Err(ClientError::NotSupported(
                    "get_presence: homeserver has presence disabled".to_string(),
                ))
            }
            Err(e) => Err(e),
        }
    }

    fn as_writable_social_graph(
        &self,
    ) -> Option<&dyn poly_client::WritableSocialGraphBackend> {
        Some(self)
    }
}

// ── Tier 2 — WritableSocialGraphBackend (block/ignore/set_presence) ─────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl poly_client::WritableSocialGraphBackend for MatrixClient {
    async fn add_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "add_friend: Matrix has no native friend concept".to_string(),
        ))
    }

    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "remove_friend: Matrix has no native friend concept".to_string(),
        ))
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "respond_to_friend_request: Matrix has no native friend concept".to_string(),
        ))
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "set_friend_nickname: Matrix has no native friend concept".to_string(),
        ))
    }

    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "set_user_note: Matrix has no native user note system".to_string(),
        ))
    }

    /// Block a user. Matrix conflates block and ignore via `m.ignored_user_list`.
    ///
    /// Fetches the current ignore list, adds `user_id`, and writes it back via
    /// `PUT /_matrix/client/v3/user/:user_id/account_data/m.ignored_user_list`.
    async fn block_user(&self, user_id: &str) -> ClientResult<()> {
        let me = self.current_user_id()?;
        let mut list = self.http.fetch_ignored_user_list(&me).await?;
        list.ignored_users
            .entry(user_id.to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        self.http.put_ignored_user_list(&me, &list).await
    }

    async fn unblock_user(&self, user_id: &str) -> ClientResult<()> {
        let me = self.current_user_id()?;
        let mut list = self.http.fetch_ignored_user_list(&me).await?;
        list.ignored_users.remove(user_id);
        self.http.put_ignored_user_list(&me, &list).await
    }

    /// Ignore a user — Matrix conflates block and ignore via `m.ignored_user_list`.
    async fn ignore_user(&self, user_id: &str) -> ClientResult<()> {
        // Same operation as block_user — Matrix uses m.ignored_user_list for both.
        let me = self.current_user_id()?;
        let mut list = self.http.fetch_ignored_user_list(&me).await?;
        list.ignored_users
            .entry(user_id.to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        self.http.put_ignored_user_list(&me, &list).await
    }

    async fn unignore_user(&self, user_id: &str) -> ClientResult<()> {
        // Same operation as unblock_user — Matrix uses m.ignored_user_list for both.
        let me = self.current_user_id()?;
        let mut list = self.http.fetch_ignored_user_list(&me).await?;
        list.ignored_users.remove(user_id);
        self.http.put_ignored_user_list(&me, &list).await
    }

    /// Set the authenticated user's presence via
    /// `PUT /_matrix/client/v3/presence/{userId}/status`.
    ///
    /// Maps the host `PresenceStatus` onto Matrix's three-valued surface
    /// (`online`/`unavailable`/`offline`). DND/Invisible collapse onto
    /// `unavailable` — Matrix has no first-class equivalent and signalling
    /// `offline` would silently drop the session from peers' rosters.
    /// SOLID-audit-matrix (Phase D.2).
    async fn set_presence(&self, status: PresenceStatus) -> ClientResult<()> {
        let user_id = self.current_user_id()?;
        let presence = match status {
            PresenceStatus::Online => "online",
            PresenceStatus::Idle | PresenceStatus::DoNotDisturb => "unavailable",
            PresenceStatus::Offline | PresenceStatus::Invisible => "offline",
            // Unknown is a host-side sentinel; do not transmit upstream —
            // skip the call so we don't overwrite a meaningful prior state.
            PresenceStatus::Unknown => return Ok(()),
        };
        let body = api::PutPresenceRequest {
            presence: presence.to_string(),
            status_msg: None,
        };
        match self.http.put_presence(&user_id, &body).await {
            Ok(()) => Ok(()),
            // Homeserver with presence disabled — treat as NotSupported so
            // the UI can mark the toggle as inert instead of failing loudly.
            Err(ClientError::NotFound(_) | ClientError::PermissionDenied(_)) => {
                Err(ClientError::NotSupported(
                    "set_presence: homeserver has presence disabled".to_string(),
                ))
            }
            Err(e) => Err(e),
        }
    }
}

/// Project a Matrix presence response onto the host `PresenceStatus` enum.
///
/// - `online` + `currently_active = false` → `Idle` (away-from-keyboard).
/// - `online` (otherwise) → `Online`.
/// - `unavailable` → `Idle`.
/// - `offline` → `Offline`.
/// - Any other / future variant → `Unknown` so the UI can hide the dot
///   instead of guessing.
pub fn map_matrix_presence(resp: &api::PresenceStatusResponse) -> PresenceStatus {
    match resp.presence.as_str() {
        "online" => {
            if matches!(resp.currently_active, Some(false)) {
                PresenceStatus::Idle
            } else {
                PresenceStatus::Online
            }
        }
        "unavailable" => PresenceStatus::Idle,
        "offline" => PresenceStatus::Offline,
        _ => PresenceStatus::Unknown,
    }
}
