//! # poly-forgejo
//!
//! Forgejo / Gitea / Codeberg client for Poly.
//!
//! Implements [`poly_client::ClientBackend`] using the Forgejo REST API v1
//! via direct HTTP through `poly_host_bridge::http::HttpClient`.
//!
//! Repos appear as Poly servers. Each repo exposes:
//! - an **issues** Forum channel
//! - a **pull-requests** Forum channel
//! - a **code** [`ChannelType::Code`] channel for the file/code explorer
//!
//! The backend is read-only — send_message returns NotSupported.

#[cfg(feature = "native")]
mod api;
#[cfg(feature = "native")]
mod mapping;
#[cfg(feature = "native")]
pub mod signup;
#[cfg(feature = "native")]
mod types;

#[cfg(feature = "native")]
pub use api::ForgejoApi;
#[cfg(feature = "native")]
pub use mapping::{BACKEND_SLUG, issue_thread_channel_id};

#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use futures::stream::{self, Stream};
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use std::pin::Pin;

/// Return FTL translation source for the Forgejo client plugin.
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
        }
    }

    /// Shortcut for `https://codeberg.org`.
    #[must_use]
    pub fn codeberg() -> Self {
        Self::new("https://codeberg.org")
    }

    fn session_id(&self) -> &str {
        self.session.as_ref().map(|s| s.id.as_str()).unwrap_or("fj")
    }

    fn session_login(&self) -> &str {
        self.session
            .as_ref()
            .map(|s| s.user.id.as_str())
            .unwrap_or("anonymous")
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
impl ClientBackend for ForgejoClient {
    // --- Authentication ---

    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        let token = match credentials {
            AuthCredentials::Token(t) => t,
            other => {
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
        // Single issue thread (`fj-issue-{owner}-{repo}-{number}`)
        if let Some(rest) = channel_id.strip_prefix("fj-issue-") {
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

    // --- Users ---

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

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(Vec::new())
    }

    // --- Groups / DMs ---

    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(Vec::new())
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        Ok(Vec::new())
    }

    // --- Notifications ---

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(Vec::new())
    }

    // --- Voice ---

    async fn get_voice_participants(
        &self,
        _channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>> {
        Ok(Vec::new())
    }

    // --- Presence ---

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "forgejo has no presence model".to_string(),
        ))
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
            search_messages: true,
            landing: poly_client::LandingPage::ServerOverview,
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

    // --- Client-provided UI surface ---

    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        if target != MenuTargetKind::Server {
            return Ok(Vec::new());
        }
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
                label_key: "plugin-forgejo-menu-star-repo-label".to_string(),
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

    async fn poll_action(&self, _handle: PendingHandle) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound("no pending actions".into()))
    }

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

    async fn get_setting_value(
        &self,
        _scope: SettingsScope,
        _scope_id: &str,
        key: &str,
    ) -> ClientResult<String> {
        // TODO(WP 3): wire to host-api.kv_get once exposed to this plugin.
        for section in self.get_settings_sections().await? {
            for field in section.fields {
                if field.key == key {
                    return Ok(field.default_value);
                }
            }
        }
        Err(ClientError::NotFound(format!("setting: {key}")))
    }

    async fn set_setting_value(
        &self,
        _scope: SettingsScope,
        _scope_id: &str,
        _key: &str,
        _value: &str,
    ) -> ClientResult<()> {
        // TODO(WP 3): wire to host-api.kv_set once exposed to this plugin.
        Err(ClientError::NotSupported("settings storage not yet wired".into()))
    }

    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::RepoTree,
            sections: Vec::new(),
            header_block: None,
        })
    }

    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown sidebar action: {action_id}")))
    }

    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Err(ClientError::NotSupported("channel-view not yet implemented".into()))
    }

    async fn get_view_rows(
        &self,
        _channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        Err(ClientError::NotSupported("view-rows not yet implemented".into()))
    }

    async fn get_view_detail(
        &self,
        _channel_id: &str,
        _row_id: &str,
    ) -> ClientResult<ViewDetail> {
        Err(ClientError::NotSupported("view-detail not yet implemented".into()))
    }

    async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
        Ok(Vec::new())
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(Vec::new())
    }

    async fn invoke_composer_action(
        &self,
        action_id: &str,
        _channel_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown composer action: {action_id}")))
    }

    async fn invoke_message_action(
        &self,
        action_id: &str,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown message action: {action_id}")))
    }

    // --- Code repository channels ---

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

#[cfg(feature = "native")]
fn kind_from_string(s: &str) -> FileKind {
    match s {
        "dir" => FileKind::Directory,
        "symlink" => FileKind::Symlink,
        "submodule" => FileKind::Submodule,
        _ => FileKind::File,
    }
}

#[cfg(feature = "native")]
fn split_owner_repo(s: &str) -> ClientResult<(String, String)> {
    s.split_once('-')
        .map(|(o, r)| (o.to_string(), r.to_string()))
        .ok_or_else(|| ClientError::NotFound(format!("malformed owner-repo segment: {s}")))
}

#[cfg(feature = "native")]
fn decode_b64(s: &str) -> Vec<u8> {
    // Forgejo returns base64 with embedded newlines; strip them.
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    decode_b64_simple(&cleaned)
}

#[cfg(feature = "native")]
fn decode_b64_simple(input: &str) -> Vec<u8> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (i, &b) in TABLE.iter().enumerate() {
        if let Some(slot) = lookup.get_mut(usize::from(b)) {
            *slot = i as u8;
        }
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in bytes {
        if b == b'=' {
            break;
        }
        let v = lookup.get(usize::from(b)).copied().unwrap_or(255);
        if v == 255 {
            continue;
        }
        buf = (buf << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    out
}
