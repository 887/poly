//! # poly-github
//!
//! GitHub / GitHub Enterprise client for Poly.
//!
//! Uses the user's `gh` CLI as transport — no token extraction, no
//! direct HTTP. The CLI handles auth, rate limiting, and pagination.
//!
//! GitHub repos appear as Poly servers (filtered to owner + collaborator
//! repos with activity in the last two years). Each repo exposes:
//!
//! - an **issues** Forum channel
//! - a **pull-requests** Forum channel
//! - a **code** [`ChannelType::Code`] channel for the file/code explorer
//!
//! Code search is intentionally external — clients should open
//! `https://{instance}/{owner}/{repo}/search?type=code&q=…` for that.

#[cfg(feature = "native")]
mod api;
#[cfg(feature = "native")]
mod mapping;
#[cfg(feature = "native")]
pub mod signup;
#[cfg(feature = "native")]
mod types;

#[cfg(feature = "native")]
use std::pin::Pin;

#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use futures::stream::{self, Stream};
#[cfg(feature = "native")]
use poly_client::*;

#[cfg(feature = "native")]
pub use api::{GhCli, GhError};
#[cfg(feature = "native")]
pub use mapping::BACKEND_SLUG;

/// Number of years of `pushed_at` activity required for a repo to surface
/// in the server list. Two years matches the user's stated requirement.
#[cfg(feature = "native")]
const ACTIVITY_WINDOW_YEARS: i64 = 2;

/// Return FTL translation source for the GitHub client plugin.
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

/// GitHub / GitHub Enterprise client.
///
/// Each instance wraps one `gh` CLI configuration. Construct with
/// [`GitHubClient::dotcom`] for github.com or [`GitHubClient::enterprise`]
/// for a GHE hostname.
#[cfg(feature = "native")]
pub struct GitHubClient {
    cli: GhCli,
    session: Option<Session>,
    /// Cached repo list — refreshed on `get_servers`.
    repos: tokio::sync::Mutex<Vec<types::GhRepo>>,
}

#[cfg(feature = "native")]
impl GitHubClient {
    /// Wrap the user's gh CLI for github.com.
    #[must_use]
    pub fn dotcom() -> Self {
        Self {
            cli: GhCli::dotcom(),
            session: None,
            repos: tokio::sync::Mutex::new(Vec::new()),
        }
    }

    /// Wrap the user's gh CLI for a GitHub Enterprise hostname.
    pub fn enterprise(hostname: impl Into<String>) -> Self {
        Self {
            cli: GhCli::enterprise(hostname),
            session: None,
            repos: tokio::sync::Mutex::new(Vec::new()),
        }
    }

    /// Build an authenticated [`Session`] from a `gh` login.
    fn build_session(&self, login: &str) -> Session {
        let instance = self.cli.instance_id().to_string();
        Session {
            id: format!("gh-{}-{}", instance, login),
            user: User {
                id: login.to_string(),
                display_name: login.to_string(),
                avatar_url: Some(format!("https://github.com/{login}.png")),
                presence: PresenceStatus::Offline,
                backend: BackendType::from(BACKEND_SLUG),
            },
            token: String::new(), // gh CLI owns the token
            backend: BackendType::from(BACKEND_SLUG),
            icon_emoji: Some("🐙".to_string()),
            instance_id: instance.clone(),
            backend_url: Some(if instance == "github.com" {
                "https://github.com".to_string()
            } else {
                format!("https://{instance}")
            }),
        }
    }

    fn session_login(&self) -> &str {
        self.session
            .as_ref()
            .map(|s| s.user.id.as_str())
            .unwrap_or("anonymous")
    }

    fn session_id(&self) -> &str {
        self.session.as_ref().map(|s| s.id.as_str()).unwrap_or("gh")
    }

    fn convert_err(e: GhError) -> ClientError {
        match e {
            GhError::Spawn(msg) => ClientError::Internal(format!(
                "gh CLI not available: {msg} — install from https://cli.github.com"
            )),
            GhError::Exit { code: _, stderr } if stderr.contains("not authenticated") => {
                ClientError::AuthFailed(stderr)
            }
            GhError::Exit { code, stderr } => {
                ClientError::Network(format!("gh exited {code}: {stderr}"))
            }
            GhError::Parse(msg) => ClientError::Internal(format!("gh JSON parse: {msg}")),
        }
    }
}

#[cfg(feature = "native")]
impl Default for GitHubClient {
    fn default() -> Self {
        Self::dotcom()
    }
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClientBackend for GitHubClient {
    // --- Authentication ---

    async fn authenticate(&mut self, _credentials: AuthCredentials) -> ClientResult<Session> {
        let login = self
            .cli
            .auth_status_login()
            .await
            .map_err(Self::convert_err)?;
        let session = self.build_session(&login);
        self.session = Some(session.clone());
        Ok(session)
    }

    async fn logout(&mut self) -> ClientResult<()> {
        // The gh CLI keeps its own credentials; we just drop our session.
        self.session = None;
        Ok(())
    }

    fn is_authenticated(&self) -> bool {
        self.session.is_some()
    }

    // --- Servers ---

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let raw = self
            .cli
            .list_user_repos()
            .await
            .map_err(Self::convert_err)?;
        let active = mapping::filter_active_repos(raw, ACTIVITY_WINDOW_YEARS);
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
            .map(|r| {
                mapping::server_from_repo(
                    r,
                    self.session_id(),
                    self.session_login(),
                )
            })
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
            "github backend is read-only — open the GitHub web UI to comment".to_string(),
        ))
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        _query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        // Issues forum: list all issues+PRs filtered by kind.
        if let Some(rest) = channel_id.strip_prefix("gh-issues-") {
            let (owner, repo) = split_owner_repo(rest)?;
            let issues = self
                .cli
                .list_repo_issues(&owner, &repo)
                .await
                .map_err(Self::convert_err)?;
            return Ok(issues
                .iter()
                .filter(|i| !i.is_pull_request())
                .map(mapping::issue_to_message)
                .collect());
        }
        if let Some(rest) = channel_id.strip_prefix("gh-pulls-") {
            let (owner, repo) = split_owner_repo(rest)?;
            let issues = self
                .cli
                .list_repo_issues(&owner, &repo)
                .await
                .map_err(Self::convert_err)?;
            return Ok(issues
                .iter()
                .filter(|i| i.is_pull_request())
                .map(mapping::issue_to_message)
                .collect());
        }
        // Single issue thread (`gh-issue-{owner}-{repo}-{number}`).
        if let Some(rest) = channel_id.strip_prefix("gh-issue-") {
            let parts: Vec<&str> = rest.rsplitn(2, '-').collect();
            if let [number_str, rest_pair] = parts.as_slice()
                && let Ok(number) = number_str.parse::<u64>()
            {
                let (owner, repo) = split_owner_repo(rest_pair)?;
                let comments = self
                    .cli
                    .list_issue_comments(&owner, &repo, number)
                    .await
                    .map_err(Self::convert_err)?;
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
            avatar_url: Some(format!("https://github.com/{id}.png")),
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
            "github has no presence model".to_string(),
        ))
    }

    // --- Real-time events ---

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        // No webhooks in this client — UI can poll if it cares.
        Box::pin(stream::empty())
    }

    // --- Backend info ---

    fn backend_type(&self) -> BackendType {
        BackendType::from(BACKEND_SLUG)
    }

    fn backend_name(&self) -> &str {
        "GitHub"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_voice: false,
            supports_video: false,
            supports_dms: false,
            supports_groups: false,
            supports_send_messages: false,
            supports_presence: false,
            supports_search: false,
            supports_reactions: false,
            supports_typing_indicators: false,
            supports_file_upload: false,
        }
    }

    fn plugin_manifest(&self) -> PluginManifest {
        PluginManifest {
            exec_programs: vec!["gh".to_string()],
            http_hosts: vec![],
            description: "Wraps the user's gh CLI to surface GitHub / GHE repos as Poly servers. \
                          No tokens are read from disk; all auth flows go through gh."
                .to_string(),
            homepage: Some("https://cli.github.com".to_string()),
        }
    }

    // --- Code repository channels ---

    async fn list_files(&self, channel_id: &str, path: &str) -> ClientResult<Vec<FileEntry>> {
        let (owner, repo) = mapping::parse_code_channel(channel_id).ok_or_else(|| {
            ClientError::NotFound(format!("not a code channel: {channel_id}"))
        })?;
        let contents = self
            .cli
            .get_contents(&owner, &repo, path)
            .await
            .map_err(Self::convert_err)?;
        let entries = match contents {
            types::GhContents::Dir(entries) => entries,
            types::GhContents::File(entry) => vec![entry],
        };
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
        let (owner, repo) = mapping::parse_code_channel(channel_id).ok_or_else(|| {
            ClientError::NotFound(format!("not a code channel: {channel_id}"))
        })?;
        let contents = self
            .cli
            .get_contents(&owner, &repo, path)
            .await
            .map_err(Self::convert_err)?;
        let entry = match contents {
            types::GhContents::File(e) => e,
            types::GhContents::Dir(_) => {
                return Err(ClientError::NotFound(format!(
                    "{path} is a directory, not a file"
                )))
            }
        };
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
    // GitHub returns base64 with embedded newlines; strip them.
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    decode_b64_simple(&cleaned)
}

#[cfg(feature = "native")]
#[allow(clippy::indexing_slicing)]
fn decode_b64_simple(input: &str) -> Vec<u8> {
    const TABLE: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (i, &b) in TABLE.iter().enumerate() {
        lookup[b as usize] = i as u8;
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in bytes {
        if b == b'=' {
            break;
        }
        let v = lookup[b as usize];
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
