//! # poly-forgejo
//!
//! Forgejo / Gitea / Codeberg client for Poly.
//!
//! Implements [`poly_client::IsBackend`] using the Forgejo REST API v1
//! via direct HTTP through `poly_host_bridge::http::HttpClient`.
//!
//! Repos appear as Poly servers. Each repo exposes:
//! - an **issues** Forum channel
//! - a **pull-requests** Forum channel
//! - a **code** [`ChannelType::Code`] channel for the file/code explorer
//!
//! The backend is read-only — send_message returns NotSupported.

/// The backend slug used in all [`poly_client::BackendType`] constructions for this crate.
pub const SLUG: &str = "forgejo";

#[cfg(feature = "native")]
mod api;
#[cfg(feature = "native")]
mod channel_ids;
#[cfg(feature = "native")]
mod mapping;
#[cfg(feature = "native")]
pub mod signup;
#[cfg(feature = "native")]
mod types;

#[cfg(feature = "native")]
pub use api::ForgejoApi;
#[cfg(feature = "native")]
pub use mapping::{BACKEND_SLUG, issue_thread_channel_id, map_issue_to_viewrow};
#[cfg(feature = "native")]
pub use types::ForgejoIssue;

#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use futures::stream::{self, Stream};
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use poly_common_forge::{decode_b64, kind_from_string, split_owner_repo};
#[cfg(feature = "native")]
use std::pin::Pin;

// ── Not-supported message constants ─────────────────────────────────────────
// One cfg gate for all constants; the 30+ NotSupported sites don't allocate
// unique string literals.
#[cfg(feature = "native")]
mod ns {
    pub(super) const FRIEND: &str = "Forgejo has no friend system";
    pub(super) const USER_NOTE: &str = "Forgejo has no user note system";
    pub(super) const BLOCK: &str = "Forgejo: block not supported via this interface";
    pub(super) const UNBLOCK: &str = "Forgejo: unblock not supported via this interface";
    pub(super) const IGNORE: &str = "Forgejo has no ignore concept";
    pub(super) const PRESENCE: &str = "Forgejo has no presence model";
    pub(super) const DM: &str = "Forgejo has no DM concept";
    pub(super) const SAVED_MSG: &str = "Forgejo has no saved-messages concept";
    pub(super) const GROUP_DM: &str = "Forgejo has no group DMs";
    pub(super) const CONV_MUTE: &str = "Forgejo has no conversation mute";
}

/// Return FTL translation source for the Forgejo client plugin.
#[must_use] 
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

/// Forgejo / Gitea / Codeberg client.
///
/// Construct with [`ForgejoClient::new`] for any instance or
/// [`ForgejoClient::codeberg`] as a shortcut for `https://codeberg.org`.
#[cfg(feature = "native")]
pub struct ForgejoClient {
    api: ForgejoApi,
    session: Option<Session>,
    /// Cached repo list — refreshed on `get_servers`.
    repos: tokio::sync::Mutex<Vec<types::ForgejoRepo>>,
    /// In-memory settings storage for this client instance.
    settings_storage: SettingsStorageCell,
    /// Stored version override (None = use api::DEFAULT_CLIENT_VERSION).
    version_override: std::sync::Mutex<Option<String>>,
}

#[cfg(feature = "native")]
impl ForgejoClient {
    /// Create a new client pointed at `instance_url`.
    #[must_use]
    pub fn new(instance_url: &str) -> Self {
        Self {
            api: ForgejoApi::new(instance_url),
            session: None,
            repos: tokio::sync::Mutex::new(Vec::new()),
            settings_storage: SettingsStorageCell::new(),
            version_override: std::sync::Mutex::new(None),
        }
    }

    /// Shortcut for `https://codeberg.org`.
    #[must_use]
    pub fn codeberg() -> Self {
        Self::new("https://codeberg.org")
    }

    fn session_id(&self) -> &str {
        self.session.as_ref().map_or("fj", |s| s.id.as_str())
    }

    fn session_login(&self) -> &str {
        self.session
            .as_ref()
            .map_or("anonymous", |s| s.user.id.as_str())
    }

}

#[cfg(feature = "native")]
impl Default for ForgejoClient {
    fn default() -> Self {
        Self::codeberg()
    }
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl IsBackend for ForgejoClient {
    // --- Authentication ---

    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        let token = match credentials {
            AuthCredentials::Token(t) => t,
            other @ (AuthCredentials::EmailPassword { .. }
            | AuthCredentials::OAuth { .. }
            | AuthCredentials::DeviceCode { .. }
            | AuthCredentials::PolyServer { .. }) => {
                return Err(ClientError::AuthFailed(format!(
                    "Forgejo does not support {:?} credentials",
                    std::mem::discriminant(&other)
                )));
            }
        };

        self.api.set_token(token.clone());

        let user = self
            .api
            .get_authenticated_user()
            .await
            .map_err(|e| ClientError::AuthFailed(format!("Forgejo auth failed: {e}")))?;

        // Derive the instance URL from the api base_url by stripping `/api/v1`.
        let instance_url = self
            .api
            .base_url()
            .trim_end_matches("/api/v1")
            .to_string();
        let instance_id = instance_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_string();

        let session = Session {
            id: format!("fj-{}-{}", instance_id, user.login),
            user: mapping::user_from_fj(&user),
            token,
            backend: BackendType::from(BACKEND_SLUG),
            icon_emoji: Some("🦊".to_string()),
            instance_id,
            backend_url: Some(instance_url),
        };
        self.session = Some(session.clone());
        Ok(session)
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.session = None;
        // Clear the API token so subsequent requests are unauthenticated.
        self.api.clear_token();
        Ok(())
    }

    fn is_authenticated(&self) -> bool {
        self.session.is_some()
    }

    // --- Servers ---

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let raw = self.api.list_user_repos().await?;
        let active = mapping::filter_active_repos(raw);
        let account_id = self.session_id().to_string();
        let display_name = self.session_login().to_string();
        let servers: Vec<Server> = active
            .iter()
            .map(|r| mapping::server_from_repo(r, &account_id, &display_name))
            .collect();
        let mut cache = self.repos.lock().await;
        *cache = active;
        Ok(servers)
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let cache = self.repos.lock().await;
        cache
            .iter()
            .find(|r| mapping::server_id_for_repo(r) == id)
            .map(|r| mapping::server_from_repo(r, self.session_id(), self.session_login()))
            .ok_or_else(|| ClientError::NotFound(format!("repo {id}")))
    }

    // --- Channels ---

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        let cache = self.repos.lock().await;
        let repo = cache
            .iter()
            .find(|r| mapping::server_id_for_repo(r) == server_id)
            .ok_or_else(|| ClientError::NotFound(format!("repo {server_id}")))?;
        Ok(mapping::channels_for_repo(repo))
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        let cache = self.repos.lock().await;
        for repo in cache.iter() {
            for ch in mapping::channels_for_repo(repo) {
                if ch.id == id {
                    return Ok(ch);
                }
            }
        }
        Err(ClientError::NotFound(format!("channel {id}")))
    }

    // --- Messages ---

    async fn send_message(
        &self,
        _channel_id: &str,
        _content: MessageContent,
    ) -> ClientResult<Message> {
        Err(ClientError::NotSupported(
            "forgejo backend is read-only — open the instance web UI to comment".to_string(),
        ))
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        _query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        // Issues forum channel
        if let Some(rest) = channel_id.strip_prefix("fj-issues-") {
            let (owner, repo) = split_owner_repo(rest)?;
            let issues = self.api.list_repo_issues(&owner, &repo).await?;
            return Ok(issues.iter().map(mapping::issue_to_message).collect());
        }
        // Pull requests forum channel
        if let Some(rest) = channel_id.strip_prefix("fj-pulls-") {
            let (owner, repo) = split_owner_repo(rest)?;
            let pulls = self.api.list_repo_pulls(&owner, &repo).await?;
            return Ok(pulls.iter().map(mapping::issue_to_message).collect());
        }
        // Single issue thread (`fj-issue-{owner}~{repo}-{number}`)
        if let Some(rest) = channel_id.strip_prefix("fj-issue-") {
            // rest = "{owner}~{repo}-{number}"; split off the trailing "-{number}"
            let parts: Vec<&str> = rest.rsplitn(2, '-').collect();
            if let [number_str, rest_pair] = parts.as_slice()
                && let Ok(number) = number_str.parse::<u64>()
            {
                let (owner, repo) = split_owner_repo(rest_pair)?;
                let comments = self.api.list_issue_comments(&owner, &repo, number).await?;
                return Ok(comments.iter().map(mapping::comment_to_message).collect());
            }
        }
        Ok(Vec::new())
    }

    // ── Social graph (H.3.b — moved to SocialGraphBackend) ──────────────────

    fn as_social_graph(&self) -> Option<&dyn poly_client::SocialGraphBackend> {
        Some(self)
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(Vec::new())
    }

    // ── DMs and groups (H.3.c — moved to DmsAndGroupsBackend) ──────────────

    fn as_dms_and_groups(&self) -> Option<&dyn poly_client::DmsAndGroupsBackend> {
        Some(self)
    }

    // --- Notifications ---

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(Vec::new())
    }

    // --- Voice / Settings / Views / Context: moved to C.1 sub-traits below ---

    fn as_settings(&self) -> Option<&dyn poly_client::SettingsBackend> {
        Some(self)
    }

    fn as_view_descriptor(&self) -> Option<&dyn poly_client::ViewDescriptorBackend> {
        Some(self)
    }

    fn as_context_action(&self) -> Option<&dyn poly_client::ContextActionBackend> {
        Some(self)
    }

    // --- Moderation methods moved to ModerationBackend (H.3.a) ---

    fn as_moderation(&self) -> Option<&dyn poly_client::ModerationBackend> {
        Some(self)
    }

    // --- Real-time events ---

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        Box::pin(stream::empty())
    }

    // --- Backend info ---

    fn backend_type(&self) -> BackendType {
        BackendType::from(BACKEND_SLUG)
    }

    fn backend_name(&self) -> &str {
        "Forgejo"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            landing: poly_client::LandingPage::Overview,
            // Forgejo moderation surface: only delete_message is supported.
            // All other flags remain false (no kick/ban/roles/channel-mgmt/modlog).
            has_roles: false,
            has_kick: false,
            has_ban: false,
            has_timed_ban: false,
            has_channel_mgmt: false,
            has_moderation_log: false,
            ..BackendCapabilities::READ_ONLY_FEED
        }
    }

    fn plugin_manifest(&self) -> PluginManifest {
        let instance_url = self
            .api
            .base_url()
            .trim_end_matches("/api/v1")
            .to_string();
        PluginManifest {
            exec_programs: vec![],
            http_hosts: vec![instance_url],
            description: "Reads repos, issues, pull requests, and source code from any \
                          Forgejo, Gitea, or Codeberg instance via the REST API v1."
                .to_string(),
            homepage: Some("https://forgejo.org".to_string()),
        }
    }

    // --- Code repository channels (H.2.a — moved to CodeRepoBackend) ---

    fn as_code_repo(&self) -> Option<&dyn poly_client::CodeRepoBackend> {
        Some(self)
    }

    fn get_signup_method(&self, server_url: Option<&str>) -> SignupMethod {
        let base = server_url.unwrap_or("https://codeberg.org");
        SignupMethod::External(format!("{}/user/sign_up", base.trim_end_matches('/')))
    }

    fn client_version(&self) -> String {
        self.version_override
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| api::DEFAULT_CLIENT_VERSION.to_string())
    }

    async fn set_client_version_override(
        &self,
        version_override: Option<String>,
    ) -> ClientResult<()> {
        let new_ua = version_override
            .clone()
            .unwrap_or_else(|| api::DEFAULT_CLIENT_VERSION.to_string());
        if let Ok(mut lock) = self.version_override.lock() {
            *lock = version_override;
        }
        self.api.set_user_agent(new_ua);
        Ok(())
    }
}

// ── H.2.a — CodeRepoBackend ──────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::CodeRepoBackend for ForgejoClient {
    async fn list_files(&self, channel_id: &str, path: &str) -> ClientResult<Vec<FileEntry>> {
        let (owner, repo) = mapping::parse_code_channel(channel_id)
            .ok_or_else(|| ClientError::NotFound(format!("not a code channel: {channel_id}")))?;
        let entries = self.api.get_contents(&owner, &repo, path).await?;
        Ok(entries
            .into_iter()
            .map(|e| FileEntry {
                kind: kind_from_string(&e.kind),
                path: e.path,
                name: e.name,
                size: e.size,
            })
            .collect())
    }

    async fn read_file(&self, channel_id: &str, path: &str) -> ClientResult<FileContent> {
        let (owner, repo) = mapping::parse_code_channel(channel_id)
            .ok_or_else(|| ClientError::NotFound(format!("not a code channel: {channel_id}")))?;
        let entry = self.api.get_file_content(&owner, &repo, path).await?;
        let bytes = match (entry.encoding.as_deref(), entry.content) {
            (Some("base64"), Some(b64)) => decode_b64(&b64),
            (_, Some(raw)) => raw.into_bytes(),
            _ => Vec::new(),
        };
        Ok(FileContent {
            path: entry.path,
            bytes,
            truncated: false,
        })
    }
}

// ── H.3.a — ModerationBackend ────────────────────────────────────────────────
#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ModerationBackend for ForgejoClient {
    /// Return the caller's effective permissions on a Forgejo repo.
    ///
    /// Calls `GET /repos/{owner}/{repo}` and reads the `permissions` object.
    /// `manage_messages` is true when the caller has admin or push access
    /// (which lets them delete issue comments via the REST API).
    async fn get_my_permissions(
        &self,
        server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        let (owner, repo) = channel_ids::repo_owner_name_from_server_id(self, server_id).await?;
        let resp = self.api.get_repo_permissions(&owner, &repo).await?;
        let p = resp.permissions;
        let can_manage = p.admin || p.push;
        let display_role = if p.admin {
            "Admin".to_string()
        } else if p.push {
            "Write".to_string()
        } else {
            "Read".to_string()
        };
        Ok(MemberPermissions {
            manage_server: p.admin,
            manage_channels: false,
            manage_roles: false,
            kick_members: false,
            ban_members: false,
            manage_messages: can_manage,
            timeout_members: false,
            display_role,
            power_level: None,
        })
    }

    /// Kick is not a concept on Forgejo (collaborator management is separate).
    async fn kick_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Forgejo: collaborators have no kick concept; use the org settings to remove access"
                .to_string(),
        ))
    }

    /// Forgejo has no per-repo ban concept.
    async fn ban_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _reason: Option<&str>,
        _delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Forgejo: no per-repo ban; site admins can suspend users via the admin panel only"
                .to_string(),
        ))
    }

    async fn unban_member(&self, _server_id: &str, _member_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Forgejo: no per-repo ban/unban".to_string(),
        ))
    }

    async fn timeout_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _until: chrono::DateTime<chrono::Utc>,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Forgejo: no timeout concept".to_string(),
        ))
    }

    async fn untimeout_member(&self, _server_id: &str, _member_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Forgejo: no timeout concept".to_string(),
        ))
    }

    async fn get_bans(&self, _server_id: &str) -> ClientResult<Vec<BannedMember>> {
        Err(ClientError::NotSupported(
            "Forgejo: no per-repo ban list".to_string(),
        ))
    }

    /// Delete an issue comment.
    ///
    /// `channel_id` must be an issue thread channel (`fj-issue-{owner}-{repo}-{n}`).
    /// `message_id` must be a comment message ID (`fj-comment-{numeric_id}`).
    async fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<()> {
        // Parse the numeric comment id from the message_id prefix.
        let comment_id_str = message_id
            .strip_prefix("fj-comment-")
            .ok_or_else(|| {
                ClientError::NotFound(format!(
                    "delete_message: not a Forgejo comment id: {message_id}"
                ))
            })?;
        let comment_id: u64 = comment_id_str.parse().map_err(|_err| {
            ClientError::NotFound(format!(
                "delete_message: malformed comment id: {message_id}"
            ))
        })?;

        // Parse owner/repo from the issue thread channel id.
        let (owner, repo) = channel_ids::parse_issue_thread_owner_repo(channel_id)?;
        self.api.delete_issue_comment(&owner, &repo, comment_id).await
    }

    /// Channel update is not supported for Forgejo repos.
    async fn update_channel(
        &self,
        _channel_id: &str,
        _update: UpdateChannelParams,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Forgejo: channel concept maps to issue/PR sections; renaming/reordering not exposed"
                .to_string(),
        ))
    }

    async fn reorder_channels(
        &self,
        _server_id: &str,
        _ordering: Vec<String>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Forgejo: channel reordering not supported".to_string(),
        ))
    }

    async fn get_moderation_log(
        &self,
        _server_id: &str,
        _limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        Err(ClientError::NotSupported(
            "Forgejo: admin audit log is not available via the REST API".to_string(),
        ))
    }

    async fn get_server_roles(&self, _server_id: &str) -> ClientResult<Vec<Role>> {
        Err(ClientError::NotSupported(
            "Forgejo: no role concept".to_string(),
        ))
    }
}

// ── H.3.b — SocialGraphBackend ────────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SocialGraphBackend for ForgejoClient {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        Ok(User {
            id: id.to_string(),
            display_name: id.to_string(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: BackendType::from(BACKEND_SLUG),
        })
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        Ok(Vec::new())
    }

    async fn add_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::FRIEND.to_string()))
    }

    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::FRIEND.to_string()))
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::FRIEND.to_string()))
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::FRIEND.to_string()))
    }

    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::USER_NOTE.to_string()))
    }

    async fn block_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::BLOCK.to_string()))
    }

    async fn unblock_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::UNBLOCK.to_string()))
    }

    async fn ignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::IGNORE.to_string()))
    }

    async fn unignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::IGNORE.to_string()))
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::PRESENCE.to_string()))
    }
}

// ── H.3.c — DmsAndGroupsBackend ───────────────────────────────────────────────
// Forgejo has no DM or group DM concept.

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DmsAndGroupsBackend for ForgejoClient {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(Vec::new())
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        Ok(Vec::new())
    }

    async fn open_direct_message_channel(&self, _user_id: &str) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(ns::DM.to_string()))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(ns::SAVED_MSG.to_string()))
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::GROUP_DM.to_string()))
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::GROUP_DM.to_string()))
    }

    async fn add_users_to_group_dm(&self, _channel_id: &str, _user_ids: &[String]) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::GROUP_DM.to_string()))
    }

    async fn close_dm_channel(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::DM.to_string()))
    }

    async fn mute_conversation(
        &self,
        _channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::CONV_MUTE.to_string()))
    }

    async fn unmute_conversation(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::CONV_MUTE.to_string()))
    }

    async fn leave_group_dm(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::GROUP_DM.to_string()))
    }

    async fn edit_group_dm(
        &self,
        _channel_id: &str,
        _name: Option<&str>,
        _avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(ns::GROUP_DM.to_string()))
    }
}

// ── C.1 — SettingsBackend ────────────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SettingsBackend for ForgejoClient {
    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        Ok(vec![SettingsSection {
            scope: SettingsScope::AccountGlobal,
            section_key: "preferences".to_string(),
            icon: None,
            fields: vec![
                SettingDescriptor {
                    key: "show-private-repos".to_string(),
                    kind: SettingKind::Toggle,
                    default_value: "true".to_string(),
                    extra: String::new(),
                },
                SettingDescriptor {
                    key: "default-issue-state".to_string(),
                    kind: SettingKind::Select,
                    default_value: "\"open\"".to_string(),
                    extra: "[\"open\",\"closed\",\"all\"]".to_string(),
                },
            ],
            info_block: None,
        }])
    }

    fn settings_storage(&self) -> &SettingsStorageCell {
        &self.settings_storage
    }
}

// ── C.1 — ViewDescriptorBackend ──────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ViewDescriptorBackend for ForgejoClient {
    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::RepoTree,
            sections: Vec::new(),
            header_block: None,
        })
    }

    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-forgejo-overview-title".to_string()),
                subtitle_key: Some("plugin-forgejo-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec {
                primary_field: "name".to_string(),
            }),
        })
    }

    async fn get_channel_view(&self, channel_id: &str) -> ClientResult<ViewDescriptor> {
        let title_key = if channel_id.starts_with("fj-pulls-") {
            "plugin-forgejo-view-pulls-title"
        } else if channel_id.starts_with("fj-discussions-") {
            "plugin-forgejo-view-discussions-title"
        } else {
            "plugin-forgejo-view-issues-title"
        };
        Ok(ViewDescriptor {
            kind: ViewKind::Split,
            header: Some(ViewHeader {
                title_key: Some(title_key.to_string()),
                subtitle_key: None,
                info_block: None,
            }),
            toolbar: Some(ViewToolbar {
                sort_options: vec![],
                filter_options: vec![
                    ToolbarOption { id: "open".to_string(), label_key: "plugin-forgejo-filter-open".to_string(), icon: None, default_selected: true },
                    ToolbarOption { id: "closed".to_string(), label_key: "plugin-forgejo-filter-closed".to_string(), icon: None, default_selected: false },
                ],
                tabs: vec![],
                action_items: vec![],
            }),
            body: ViewBody::SplitBody(SplitSpec {
                list_side: ListSpec {
                    row_template: RowTemplate {
                        primary_field: "title".to_string(),
                        secondary_field: Some("number".to_string()),
                        meta_field: Some("state-labels-author".to_string()),
                        icon_field: None,
                    },
                    page_size: 30,
                },
                detail_view_kind: ViewKind::FlatList,
            }),
        })
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        filter_id: Option<&str>,
        tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        if channel_id.is_empty() || channel_id == "fj-overview" {
            let repos = self.repos.lock().await;
            let page: u32 = cursor
                .as_ref()
                .and_then(|c| c.value.parse().ok())
                .unwrap_or(1);
            let page_size: usize = 30;
            let start = usize::try_from(page.saturating_sub(1))
                .unwrap_or(usize::MAX)
                .saturating_mul(page_size);
            let slice: Vec<_> = repos.iter().skip(start).take(page_size).collect();
            let rows: Vec<ViewRow> = slice
                .iter()
                .map(|r| ViewRow {
                    id: mapping::server_id_for_repo(r),
                    primary_text: r.full_name.clone(),
                    secondary_text: r.description.clone(),
                    meta_text: Some(format!(
                        "⭐ {} · 🍴 {} · {} open issues",
                        r.stars_count, r.forks_count, r.open_issues_count
                    )),
                    icon: None,
                    badge: None,
                    context_menu_target_kind: MenuTargetKind::Server,
                    preview_image_url: None,
                    is_video: false,
                })
                .collect();
            let next_cursor = if repos.len() > start.saturating_add(page_size) {
                Some(Cursor { kind: CursorKind::Offset, value: page.saturating_add(1).to_string() })
            } else {
                None
            };
            return Ok(ViewRowsPage { rows, next_cursor });
        }

        if tab_id == Some("discussions") || channel_id.starts_with("fj-discussions-") {
            return Ok(ViewRowsPage { rows: Vec::new(), next_cursor: None });
        }

        let (owner, repo) = channel_ids::parse_forum_channel(channel_id)?;
        let state = filter_id.unwrap_or("open");

        let want_pulls = tab_id == Some("pulls") || channel_id.starts_with("fj-pulls-");
        let issue_type = if want_pulls { "pulls" } else { "issues" };

        let page: u32 = cursor
            .as_ref()
            .and_then(|c| c.value.parse().ok())
            .unwrap_or(1);

        let raw = self
            .api
            .list_repo_issues_paged(&owner, &repo, state, issue_type, page)
            .await?;

        let rows: Vec<_> = raw.iter().map(mapping::map_issue_to_viewrow).collect();

        let next_cursor = if rows.len() == 30 {
            Some(Cursor { kind: CursorKind::Offset, value: page.saturating_add(1).to_string() })
        } else {
            None
        };

        Ok(ViewRowsPage { rows, next_cursor })
    }

    async fn get_view_detail(
        &self,
        channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        let (owner, repo) = channel_ids::parse_forum_channel(channel_id)?;
        let index: u64 = row_id
            .parse()
            .map_err(|_err| ClientError::NotFound(format!("row_id must be an issue number: {row_id}")))?;
        let issue = self.api.get_issue(&owner, &repo, index).await?;
        let comments = self
            .api
            .list_issue_comments(&owner, &repo, index)
            .await
            .unwrap_or_default();
        Ok(mapping::issue_to_view_detail(&issue, &comments))
    }
}

// ── C.1 — ContextActionBackend ───────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ContextActionBackend for ForgejoClient {
    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        if target != MenuTargetKind::Server {
            return Ok(Vec::new());
        }

        let star_label_key = if self.is_authenticated() {
            let maybe_owner_repo = {
                let cache = self.repos.lock().await;
                cache.iter().find_map(|r| {
                    if mapping::server_id_for_repo(r) == target_id {
                        let (o, n) = mapping::split_full_name(&r.full_name);
                        Some((o, n))
                    } else {
                        None
                    }
                })
            };
            if let Some((owner, repo)) = maybe_owner_repo {
                let starred = self.api.is_starred(&owner, &repo).await.unwrap_or(false);
                if starred {
                    "plugin-forgejo-menu-unstar-repo-label"
                } else {
                    "plugin-forgejo-menu-star-repo-label"
                }
            } else {
                "plugin-forgejo-menu-star-repo-label"
            }
        } else {
            "plugin-forgejo-menu-star-repo-label"
        };

        Ok(vec![
            MenuItem {
                id: "open-in-forgejo".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: "plugin-forgejo-menu-open-in-forgejo-label".to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            },
            MenuItem {
                id: "star-repo".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: star_label_key.to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            },
            MenuItem {
                id: "watch-repo".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: "plugin-forgejo-menu-watch-repo-label".to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            },
        ])
    }

    async fn invoke_context_action(
        &self,
        action_id: &str,
        _target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "open-in-forgejo" | "star-repo" | "watch-repo" => Ok(ActionOutcome::Noop),
            _ => Err(ClientError::NotFound(format!("unknown action: {action_id}"))),
        }
    }
}
