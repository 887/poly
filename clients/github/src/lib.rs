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
//!
//! ## Native vs WASM
//!
//! On native targets the [`api`] module spawns the user's `gh` CLI directly
//! via [`tokio::process::Command`]. On wasm32 (the dioxus web build that runs
//! inside the Wry / Electron shells) the same module instead POSTs to a
//! localhost subprocess bridge exposed by the native shell at
//! `http://127.0.0.1:9223/gh`. The shell forwards each call to its own
//! `gh` binary and pipes stdout/stderr/exit_code back to the WASM frontend,
//! so the rest of the crate is target-agnostic.

/// The backend slug used in all [`poly_client::BackendType`] constructions for this crate.
pub const SLUG: &str = "github";

mod api;
mod mapping;
pub mod signup;
mod types;

use std::pin::Pin;

use async_trait::async_trait;
use futures::stream::{self, Stream};
use poly_client::*;
use poly_common_forge::{decode_b64, kind_from_string, split_owner_repo};

pub use api::{GhCli, GhError, RepoPermissions};
pub use mapping::{BACKEND_SLUG, issue_thread_channel_id};

/// Number of years of `pushed_at` activity required for a repo to surface
/// in the server list. Two years matches the user's stated requirement.
const ACTIVITY_WINDOW_YEARS: i64 = 2;

/// Return FTL translation source for the GitHub client plugin.
#[must_use]
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
pub struct GitHubClient {
    cli: GhCli,
    session: Option<Session>,
    /// Cached repo list — refreshed on `get_servers`.
    repos: tokio::sync::Mutex<Vec<types::GhRepo>>,
    /// Pack C P18 — in-memory settings storage stub. TODO: migrate to
    /// `host-api.kv_set` once exposed to plugins for true persistence.
    settings_storage: SettingsStorageCell,
    /// Stored version override (None = use DEFAULT_CLIENT_VERSION).
    ///
    /// Note: the gh CLI controls the wire-level User-Agent for all HTTP
    /// requests. This field records the override for `client_version()` to
    /// return; it does NOT propagate to the wire because `GhCli` owns the
    /// transport and does not expose a User-Agent override surface.
    version_override: std::sync::Mutex<Option<String>>,
}

impl GitHubClient {
    /// Wrap the user's gh CLI for github.com.
    #[must_use]
    pub fn dotcom() -> Self {
        Self {
            cli: GhCli::dotcom(),
            session: None,
            repos: tokio::sync::Mutex::new(Vec::new()),
            settings_storage: SettingsStorageCell::new(),
            version_override: std::sync::Mutex::new(None),
        }
    }

    /// Wrap the user's gh CLI for a GitHub Enterprise hostname.
    pub fn enterprise(hostname: impl Into<String>) -> Self {
        Self {
            cli: GhCli::enterprise(hostname),
            session: None,
            repos: tokio::sync::Mutex::new(Vec::new()),
            settings_storage: SettingsStorageCell::new(),
            version_override: std::sync::Mutex::new(None),
        }
    }

    /// Create a client using direct HTTP transport (for testing).
    pub fn with_http(base_url: impl Into<String>) -> Self {
        Self {
            cli: GhCli::with_http(base_url),
            session: None,
            repos: tokio::sync::Mutex::new(Vec::new()),
            settings_storage: SettingsStorageCell::new(),
            version_override: std::sync::Mutex::new(None),
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

    /// Look up the cached repo for a server ID and return `(owner, repo)`.
    /// Returns `None` if not found in cache.
    async fn resolve_owner_repo_from_server_id(
        &self,
        server_id: &str,
    ) -> Option<(String, String)> {
        let cache = self.repos.lock().await;
        cache.iter().find_map(|r| {
            if mapping::server_id_for_repo(r) == server_id {
                let (owner, repo) = mapping::split_full_name(&r.full_name);
                Some((owner, repo))
            } else {
                None
            }
        })
    }

    fn session_login(&self) -> &str {
        self.session
            .as_ref()
            .map_or("anonymous", |s| s.user.id.as_str())
    }

    fn session_id(&self) -> &str {
        self.session.as_ref().map_or("gh", |s| s.id.as_str())
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

impl Default for GitHubClient {
    fn default() -> Self {
        Self::dotcom()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl IsBackend for GitHubClient {
    // --- Authentication ---

    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        // In HTTP mode, extract the token from credentials and set it on the CLI transport.
        if let AuthCredentials::Token(ref token) = credentials
            && !token.is_empty()
        {
            self.cli.set_token(token.clone());
        }
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
        self.cli.clear_token();
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

    // --- Voice ---

    async fn get_voice_participants(
        &self,
        _channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>> {
        Ok(Vec::new())
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
            notifications: NotificationSupport::Activity,
            landing: poly_client::LandingPage::Overview,
            // GitHub moderation surface: only delete_message is supported.
            // kick/ban/timeout/channel-mgmt/modlog are all NotSupported.
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
        PluginManifest {
            exec_programs: vec!["gh".to_string()],
            http_hosts: vec!["api.github.com".to_string()],
            description: "GitHub / GHE backend with two transports: by default spawns the user's \
                          gh CLI (no tokens read from disk, all auth goes through gh) or — when an \
                          account is configured with a token — speaks the GitHub REST API directly."
                .to_string(),
            homepage: Some("https://cli.github.com".to_string()),
        }
    }

    // --- Code repository channels (H.2.a — moved to CodeRepoBackend) ---

    fn as_code_repo(&self) -> Option<&dyn poly_client::CodeRepoBackend> {
        Some(self)
    }

    // --- Client-provided UI surface (WP 1) ---

    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        if target != MenuTargetKind::Server {
            return Ok(Vec::new());
        }

        // Resolve state-aware star label when authenticated and target_id
        // resolves to a known repo.
        let star_label_key = if self.is_authenticated() {
            match self.resolve_owner_repo_from_server_id(target_id).await {
                Some((owner, repo)) => {
                    let starred = self.cli.is_starred(&owner, &repo).await.unwrap_or(false);
                    if starred {
                        "plugin-github-menu-unstar-repo-label"
                    } else {
                        "plugin-github-menu-star-repo-label"
                    }
                }
                None => "plugin-github-menu-star-repo-label",
            }
        } else {
            "plugin-github-menu-star-repo-label"
        };

        Ok(vec![
            MenuItem {
                id: "open-in-github".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: "plugin-github-menu-open-in-github-label".to_string(),
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
                label_key: "plugin-github-menu-watch-repo-label".to_string(),
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
            "open-in-github" | "star-repo" | "watch-repo" => Ok(ActionOutcome::Noop),
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

    fn settings_storage(&self) -> &SettingsStorageCell {
        &self.settings_storage
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

    /// Return a CardGrid overview of the user's repos with stars/forks/open-issue counts.
    ///
    /// The host passes an empty `channel_id` when it calls `get_view_rows` for
    /// this view (see `AccountOverviewView` in `crates/core/src/ui/client_ui/view/mod.rs`).
    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-github-overview-title".to_string()),
                subtitle_key: Some("plugin-github-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec {
                primary_field: "name".to_string(),
            }),
        })
    }

    async fn get_channel_view(&self, channel_id: &str) -> ClientResult<ViewDescriptor> {
        // Per-channel header: each of the 3 forum channels (issues / pulls /
        // discussions) is its own focused view in the sidebar so the
        // content area no longer needs the toolbar tab row to switch
        // between them. The sidebar's channel selection IS the switch.
        let title_key = if channel_id.starts_with("gh-pulls-") {
            "plugin-github-view-pulls-title"
        } else if channel_id.starts_with("gh-discussions-") {
            "plugin-github-view-discussions-title"
        } else {
            "plugin-github-view-issues-title"
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
                    ToolbarOption { id: "open".to_string(), label_key: "plugin-github-filter-open".to_string(), icon: None, default_selected: true },
                    ToolbarOption { id: "closed".to_string(), label_key: "plugin-github-filter-closed".to_string(), icon: None, default_selected: false },
                ],
                // Tabs row eliminated — the channel sidebar is the switcher.
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
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        filter_id: Option<&str>,
        tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        // Empty channel_id is the host's signal that this is the account
        // overview view (see `AccountOverviewView` — it calls get_view_rows
        // with channel_id = ""). Return one card per cached repo.
        if channel_id.is_empty() {
            let repos = self.repos.lock().await;
            let rows: Vec<ViewRow> = repos
                .iter()
                .map(|r| ViewRow {
                    id: mapping::server_id_for_repo(r),
                    primary_text: r.full_name.clone(),
                    secondary_text: r.description.clone(),
                    meta_text: Some(format!(
                        "★ {} · {} forks · {} open",
                        r.stargazers_count, r.forks_count, r.open_issues_count
                    )),
                    icon: None,
                    badge: r.language.clone(),
                    context_menu_target_kind: MenuTargetKind::Server,
                    preview_image_url: None,
                    is_video: false,
                })
                .collect();
            return Ok(ViewRowsPage { rows, next_cursor: None });
        }

        let (owner, repo) = parse_forum_channel(channel_id)?;

        // Discussions is now a top-level channel in the sidebar
        // (`gh-discussions-*`); keep the legacy `tab_id == "discussions"`
        // path for the host's older toolbar code-path during transition.
        if channel_id.starts_with("gh-discussions-") || tab_id == Some("discussions") {
            let (discussions, next_cursor) = self
                .cli
                .list_discussions(&owner, &repo, 50, None)
                .await
                .map_err(Self::convert_err)?;
            let rows = discussions
                .iter()
                .map(mapping::map_discussion_to_viewrow)
                .collect();
            return Ok(ViewRowsPage {
                rows,
                next_cursor: next_cursor.map(|v| Cursor {
                    kind: CursorKind::Opaque,
                    value: v,
                }),
            });
        }
        let state = filter_id.unwrap_or("open");

        // Determine which kind of items to return based on tab_id or channel prefix.
        let want_pulls = tab_id == Some("pulls")
            || channel_id.starts_with("gh-pulls-");
        let want_issues = tab_id == Some("issues")
            || channel_id.starts_with("gh-issues-");

        // Fetch via issues endpoint (returns issues + PRs mixed).
        let endpoint = format!(
            "/repos/{owner}/{repo}/issues?state={state}&per_page=50&sort=updated"
        );
        let raw: Vec<types::GhIssue> = self
            .cli
            .api_get(&endpoint, &[])
            .await
            .map_err(Self::convert_err)?;

        let rows: Vec<_> = raw
            .iter()
            .filter(|i| {
                if want_pulls {
                    i.is_pull_request()
                } else if want_issues {
                    !i.is_pull_request()
                } else {
                    true
                }
            })
            .map(mapping::map_issue_to_viewrow)
            .collect();

        Ok(ViewRowsPage { rows, next_cursor: None })
    }

    async fn get_view_detail(
        &self,
        channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        if channel_id.starts_with("gh-discussions-") {
            return Err(ClientError::NotSupported(
                "GitHub discussions detail is not available via the REST API; \
                 open the discussion in your browser for the full view."
                    .to_string(),
            ));
        }
        let (owner, repo) = parse_forum_channel(channel_id)?;
        let number: u64 = row_id
            .parse()
            .map_err(|_e| ClientError::NotFound(format!("row_id must be an issue number: {row_id}")))?;
        let issue = self
            .cli
            .get_issue(&owner, &repo, number)
            .await
            .map_err(Self::convert_err)?;
        // Fetch comments so the split-pane detail panel shows the full thread.
        // On failure (e.g. network error) fall back to an empty comment list so
        // the issue body is still shown rather than returning an error.
        let comments = self
            .cli
            .list_issue_comments(&owner, &repo, number)
            .await
            .unwrap_or_default();
        Ok(mapping::issue_to_view_detail(&issue, &comments))
    }

    async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
        // GitHub is a code-review/issue tracker — composer contributions are out of scope for this client.
        Ok(Vec::new())
    }

    // --- Moderation methods moved to ModerationBackend (H.3.a) ---

    fn as_moderation(&self) -> Option<&dyn poly_client::ModerationBackend> {
        Some(self)
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        // GitHub is a code-review/issue tracker — per-message actions are out of scope for this client.
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

    fn get_signup_method(&self, server_url: Option<&str>) -> SignupMethod {
        if let Some(url) = server_url {
            // GitHub Enterprise — point to instance signup
            SignupMethod::External(url.trim_end_matches('/').to_string())
        } else {
            SignupMethod::External("https://github.com/signup".into())
        }
    }

    fn client_version(&self) -> String {
        self.version_override
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| "poly-github/0.0.0".to_string())
    }

    async fn set_client_version_override(
        &self,
        version_override: Option<String>,
    ) -> ClientResult<()> {
        if let Ok(mut lock) = self.version_override.lock() {
            *lock = version_override;
        }
        // Note: the wire-level User-Agent is controlled by the gh CLI subprocess
        // and cannot be overridden from this layer. This method records the value
        // so client_version() returns it, but outbound HTTP UA is unaffected.
        Ok(())
    }
}

// ── H.2.a — CodeRepoBackend ──────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::CodeRepoBackend for GitHubClient {
    async fn list_files(&self, channel_id: &str, path: &str) -> ClientResult<Vec<FileEntry>> {
        let (owner, repo) = mapping::parse_code_channel(channel_id)
            .ok_or_else(|| ClientError::NotFound(format!("not a code channel: {channel_id}")))?;
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
        let (owner, repo) = mapping::parse_code_channel(channel_id)
            .ok_or_else(|| ClientError::NotFound(format!("not a code channel: {channel_id}")))?;
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
                )));
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

// ── H.3.a — ModerationBackend ────────────────────────────────────────────────
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ModerationBackend for GitHubClient {
    /// Get the calling user's effective permissions in a repo.
    ///
    /// Calls `GET /repos/{owner}/{repo}` and maps the `permissions` sub-object
    /// to [`MemberPermissions`]. GitHub vocabulary:
    /// - `admin` → manage server + manage channels + ban + kick + manage messages
    /// - `maintain` → manage channels + manage messages
    /// - `push` → manage messages (can delete comments in issues/PRs)
    async fn get_my_permissions(
        &self,
        server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        let (owner, repo) = self
            .resolve_owner_repo_from_server_id(server_id)
            .await
            .ok_or_else(|| ClientError::NotFound(format!("repo for server {server_id}")))?;

        let perms = self
            .cli
            .get_repo_permissions(&owner, &repo)
            .await
            .map_err(Self::convert_err)?;

        let display_role = if perms.admin {
            "Admin".to_string()
        } else if perms.maintain {
            "Maintainer".to_string()
        } else if perms.push {
            "Collaborator".to_string()
        } else if perms.triage {
            "Triager".to_string()
        } else {
            "Read".to_string()
        };

        Ok(MemberPermissions {
            manage_server: perms.admin,
            manage_channels: perms.admin || perms.maintain,
            manage_roles: perms.admin,
            kick_members: perms.admin,
            ban_members: perms.admin,
            manage_messages: perms.admin || perms.maintain || perms.push,
            timeout_members: false, // GitHub has no timeout concept
            display_role,
            power_level: None,
        })
    }

    async fn kick_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub: no kick concept".to_string()))
    }

    async fn ban_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _reason: Option<&str>,
        _delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub: no per-repo ban concept".to_string()))
    }

    async fn unban_member(&self, _server_id: &str, _member_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub: no per-repo ban concept".to_string()))
    }

    async fn timeout_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _until: chrono::DateTime<chrono::Utc>,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub: no timeout concept".to_string()))
    }

    async fn untimeout_member(&self, _server_id: &str, _member_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub: no timeout concept".to_string()))
    }

    async fn get_bans(&self, _server_id: &str) -> ClientResult<Vec<BannedMember>> {
        Err(ClientError::NotSupported("GitHub: no per-repo ban list".to_string()))
    }

    /// Delete a comment by ID.
    ///
    /// `message_id` prefix determines the endpoint:
    /// - `"comment:{id}"` → `DELETE /repos/{owner}/{repo}/issues/comments/{id}`
    /// - `"pr-comment:{id}"` → `DELETE /repos/{owner}/{repo}/pulls/comments/{id}`
    ///
    /// The `channel_id` must be an issues/pulls forum channel so that
    /// `(owner, repo)` can be resolved.
    async fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<()> {
        let (owner, repo) = parse_forum_channel(channel_id)?;

        if let Some(id_str) = message_id.strip_prefix("comment:") {
            let comment_id: u64 = id_str.parse().map_err(|_e| {
                ClientError::NotFound(format!("invalid comment id: {id_str}"))
            })?;
            let endpoint =
                format!("/repos/{owner}/{repo}/issues/comments/{comment_id}");
            self.cli.api_delete(&endpoint).await.map_err(Self::convert_err)
        } else if let Some(id_str) = message_id.strip_prefix("pr-comment:") {
            let comment_id: u64 = id_str.parse().map_err(|_e| {
                ClientError::NotFound(format!("invalid pr-comment id: {id_str}"))
            })?;
            let endpoint =
                format!("/repos/{owner}/{repo}/pulls/comments/{comment_id}");
            self.cli.api_delete(&endpoint).await.map_err(Self::convert_err)
        } else {
            Err(ClientError::NotSupported(format!(
                "GitHub: cannot delete message with unknown prefix in id '{message_id}'. \
                 Expected 'comment:<id>' or 'pr-comment:<id>'"
            )))
        }
    }

    async fn update_channel(
        &self,
        _channel_id: &str,
        _update: UpdateChannelParams,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub: channel update not supported".to_string()))
    }

    async fn reorder_channels(
        &self,
        _server_id: &str,
        _ordering: Vec<String>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub: channel reordering not supported".to_string()))
    }

    async fn get_moderation_log(
        &self,
        _server_id: &str,
        _limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        Err(ClientError::NotSupported("GitHub: no moderation log".to_string()))
    }

    async fn get_server_roles(&self, _server_id: &str) -> ClientResult<Vec<Role>> {
        Err(ClientError::NotSupported("GitHub: no role concept".to_string()))
    }
}

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
        Err(ClientError::NotSupported("GitHub has no friend system".to_string()))
    }

    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub has no friend system".to_string()))
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub has no friend system".to_string()))
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub has no friend system".to_string()))
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
        Err(ClientError::NotSupported("GitHub has no ignore concept".to_string()))
    }

    async fn unignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub has no ignore concept".to_string()))
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Err(ClientError::NotSupported("github has no presence model".to_string()))
    }
}

// ── H.3.c — DmsAndGroupsBackend ───────────────────────────────────────────────
// GitHub has no DM or group DM concept.

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DmsAndGroupsBackend for GitHubClient {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(Vec::new())
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        Ok(Vec::new())
    }

    async fn open_direct_message_channel(&self, _user_id: &str) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported("GitHub has no DM concept".to_string()))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported("GitHub has no saved-messages concept".to_string()))
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub has no group DMs".to_string()))
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub has no group DMs".to_string()))
    }

    async fn add_users_to_group_dm(&self, _channel_id: &str, _user_ids: &[String]) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub has no group DMs".to_string()))
    }

    async fn close_dm_channel(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub has no DM concept".to_string()))
    }

    async fn mute_conversation(
        &self,
        _channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub has no conversation mute".to_string()))
    }

    async fn unmute_conversation(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub has no conversation mute".to_string()))
    }

    async fn leave_group_dm(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub has no group DMs".to_string()))
    }

    async fn edit_group_dm(
        &self,
        _channel_id: &str,
        _name: Option<&str>,
        _avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("GitHub has no group DMs".to_string()))
    }
}

/// Extract `(owner, repo)` from a forum channel ID.
///
/// Handles `gh-issues-{owner}/{repo}`, `gh-pulls-{owner}/{repo}`,
/// and `gh-discussions-{owner}/{repo}`.
fn parse_forum_channel(channel_id: &str) -> ClientResult<(String, String)> {
    let rest = channel_id
        .strip_prefix("gh-issues-")
        .or_else(|| channel_id.strip_prefix("gh-pulls-"))
        .or_else(|| channel_id.strip_prefix("gh-discussions-"))
        .ok_or_else(|| ClientError::NotFound(format!("not a forum channel: {channel_id}")))?;
    split_owner_repo(rest)
}

