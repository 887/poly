//! `impl IsBackend for ForgejoClient` — authentication, servers, channels, messages,
//! event stream, capabilities, and capability-downcast accessors.

use async_trait::async_trait;
use futures::stream::{self as stream, Stream};
use poly_client::{IsBackend, AuthCredentials, ClientResult, Session, ClientError, BackendType, Server, Channel, MessageQuery, Message, User, Notification, ClientEvent, BackendCapabilities, PluginManifest, SignupMethod};
use poly_common_forge::split_owner_repo;
use std::pin::Pin;
use crate::{ForgejoClient, mapping, api};
use crate::mapping::BACKEND_SLUG;

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
        *self.repos.lock().await = active;
        Ok(servers)
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let server = {
            let cache = self.repos.lock().await;
            cache
                .iter()
                .find(|r| mapping::server_id_for_repo(r) == id)
                .map(|r| mapping::server_from_repo(r, self.session_id(), self.session_login()))
        };
        server.ok_or_else(|| ClientError::NotFound(format!("repo {id}")))
    }

    // --- Channels ---

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        let channels = {
            let cache = self.repos.lock().await;
            cache
                .iter()
                .find(|r| mapping::server_id_for_repo(r) == server_id)
                .map(mapping::channels_for_repo)
        };
        channels.ok_or_else(|| ClientError::NotFound(format!("repo {server_id}")))
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        let found = {
            let cache = self.repos.lock().await;
            let mut result = None;
            'outer: for repo in cache.iter() {
                for ch in mapping::channels_for_repo(repo) {
                    if ch.id == id {
                        result = Some(ch);
                        break 'outer;
                    }
                }
            }
            drop(cache);
            result
        };
        found.ok_or_else(|| ClientError::NotFound(format!("channel {id}")))
    }

    // --- Messages ---
    //
    // plan-trait-split-readable-vs-writable: Forgejo is read-only; we DROP
    // the `send_message` stub entirely.  Callers using the legacy
    // `backend.send_message(...)` form now hit the `IsBackend` default shim
    // which returns `Err(NotSupported)` because `as_writable_messaging`
    // returns `None`.  Direct callers using `as_writable_messaging()` will
    // get `None` and can surface a clearer "read-only backend" message.

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

    fn backend_name(&self) -> &'static str {
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
