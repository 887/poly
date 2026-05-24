use std::pin::Pin;

use async_trait::async_trait;
use futures::stream::{self, Stream};
use poly_client::*;
use poly_common_forge::split_owner_repo;

use crate::mapping;
use crate::{GitHubClient, ACTIVITY_WINDOW_YEARS};
use crate::mapping::BACKEND_SLUG;

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

    /// Send a message on a GitHub channel.
    ///
    /// Only single-issue-thread channels (`gh-issue-{owner}~{repo}-{number}`)
    /// support posting: the text is submitted as a GitHub issue comment via
    /// `POST /repos/{owner}/{repo}/issues/{number}/comments`.
    ///
    /// Forum-index channels (`gh-issues-*`, `gh-pulls-*`) and the Discussions
    /// channel (`gh-discussions-*`) return a specific `NotSupported` error
    /// explaining why: creating a new issue, PR, or Discussion requires a
    /// form-driven workflow that cannot be expressed as a plain message send.
    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        // Route by channel kind — each prefix has a different write semantics.
        if channel_id.starts_with("gh-issues-") {
            return Err(ClientError::NotSupported(
                "GitHub: cannot post to the issues forum index — \
                 use the GitHub web UI to open a new issue"
                    .to_string(),
            ));
        }
        if channel_id.starts_with("gh-pulls-") {
            return Err(ClientError::NotSupported(
                "GitHub: cannot post to the pull-requests forum index — \
                 use the GitHub web UI or CLI to open a pull request"
                    .to_string(),
            ));
        }
        if channel_id.starts_with("gh-discussions-") {
            return Err(ClientError::NotSupported(
                "GitHub: cannot post to the discussions forum index — \
                 use the GitHub web UI to start a new discussion"
                    .to_string(),
            ));
        }
        if channel_id.starts_with("gh-code-") {
            return Err(ClientError::NotSupported(
                "GitHub: code explorer channel is read-only".to_string(),
            ));
        }
        // Single issue/PR thread: gh-issue-{owner}~{repo}-{number}
        if let Some(rest) = channel_id.strip_prefix("gh-issue-") {
            let parts: Vec<&str> = rest.rsplitn(2, '-').collect();
            if let [number_str, rest_pair] = parts.as_slice()
                && let Ok(number) = number_str.parse::<u64>()
            {
                let (owner, repo) = split_owner_repo(rest_pair)?;
                let text = match &content {
                    MessageContent::Text(t) => t.clone(),
                    MessageContent::WithAttachments { text, .. } => text.clone(),
                };
                let comment = self
                    .cli
                    .create_issue_comment(&owner, &repo, number, &text)
                    .await
                    .map_err(Self::convert_err)?;
                return Ok(mapping::comment_to_message(&comment));
            }
        }
        Err(ClientError::NotSupported(format!(
            "GitHub: unrecognised channel '{channel_id}' — cannot send message"
        )))
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
        // Discussions forum: fetch via GraphQL and map each discussion as a message.
        //
        // GitHub Discussions require GraphQL (no REST endpoint for listing).
        // We fetch the first 50 ordered by last-updated and map each to a
        // read-only Message so the UI can display the discussion index.
        if let Some(rest) = channel_id.strip_prefix("gh-discussions-") {
            let (owner, repo) = split_owner_repo(rest)?;
            let (discussions, _next) = self
                .cli
                .list_discussions(&owner, &repo, 50, None)
                .await
                .map_err(Self::convert_err)?;
            return Ok(discussions
                .iter()
                .map(mapping::discussion_to_message)
                .collect());
        }
        // Single issue thread (`gh-issue-{owner}~{repo}-{number}`).
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

    // --- Moderation methods moved to ModerationBackend (H.3.a) ---

    fn as_moderation(&self) -> Option<&dyn poly_client::ModerationBackend> {
        Some(self)
    }

    // --- Client-provided UI surface — moved to C.1 sub-trait impls below ---

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
