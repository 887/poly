//! `impl IsBackend for TeamsClient` — authentication, server/channel list,
//! messaging, and event stream.

use crate::TeamsClient;
#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use futures::stream::Stream;
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use std::pin::Pin;

/// Convert a `TeamsEvent` JSON payload (from `/test/events/poll`) to a
/// `ClientEvent`. Returns None for events we don't yet surface.
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) fn teams_event_to_client(ev: &serde_json::Value) -> Option<ClientEvent> {
    let ty = ev.get("type")?.as_str()?;
    match ty {
        "MessageCreated" => {
            let resource_id = ev.get("resourceId")?.as_str()?.to_string();
            let m = ev.get("message")?;
            let msg = poly_event_message_from_json(m)?;
            Some(ClientEvent::MessageReceived { channel_id: resource_id, message: msg })
        }
        "MessageUpdated" => {
            let resource_id = ev.get("resourceId")?.as_str()?.to_string();
            let m = ev.get("message")?;
            let msg = poly_event_message_from_json(m)?;
            Some(ClientEvent::MessageEdited { channel_id: resource_id, message: msg })
        }
        "MessageDeleted" => {
            let resource_id = ev.get("resourceId")?.as_str()?.to_string();
            let message_id = ev.get("messageId")?.as_str()?.to_string();
            Some(ClientEvent::MessageDeleted { channel_id: resource_id, message_id })
        }
        _ => None,
    }
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn poly_event_message_from_json(m: &serde_json::Value) -> Option<Message> {
    let id = m.get("id")?.as_str()?.to_string();
    let content = m.get("body")
        .and_then(|b| b.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();
    let timestamp = m.get("createdDateTime")
        .and_then(|t| t.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map_or_else(chrono::Utc::now, |dt| dt.with_timezone(&chrono::Utc));
    let author_id = m.get("from")
        .and_then(|f| f.get("user"))
        .and_then(|u| u.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let author_name = m.get("from")
        .and_then(|f| f.get("user"))
        .and_then(|u| u.get("displayName"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let edited = m.get("lastModifiedDateTime").is_some_and(|v| !v.is_null());
    Some(Message {
        id,
        author: User {
            id: author_id,
            display_name: author_name,
            avatar_url: None,
            presence: PresenceStatus::Online,
            backend: BackendType::from(crate::SLUG),
        },
        content: MessageContent::Text(content),
        timestamp,
        attachments: vec![],
        reactions: vec![],
        reply_to: None,
        edited,
        thread: None,
        preview_image_url: None,
    })
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl IsBackend for TeamsClient {
    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        let token = match credentials {
            AuthCredentials::Token(t) => t,
            AuthCredentials::OAuth { token } => token,
            AuthCredentials::EmailPassword { email, password } => {
                self.http.login(&email, &password).await?
            }
            AuthCredentials::DeviceCode { .. } | AuthCredentials::PolyServer { .. } => {
                return Err(ClientError::AuthFailed(
                    "Teams requires a Bearer token".into(),
                ));
            }
        };
        self.http.set_token(token.clone());
        let user = self.http.get_me().await?;
        self.account_id = Some(user.id.clone());
        self.account_display_name = Some(user.display_name.clone());
        Ok(Session {
            id: user.id.clone(),
            user: User {
                id: user.id.clone(),
                display_name: user.display_name.clone(),
                avatar_url: None,
                presence: PresenceStatus::Online,
                backend: BackendType::from(crate::SLUG),
            },
            token,
            backend: BackendType::from(crate::SLUG),
            icon_emoji: Some("💼".to_string()),
            // Strip the URL scheme so `instance_id` is a bare "host:port"
            // (e.g. "localhost:9103") instead of "http://localhost:9103".
            // Route path segments cannot contain "://" — a scheme-inclusive
            // instance_id causes the Dioxus router to parse every Teams route
            // as PageNotFound, triggering an on_update redirect cascade that
            // produces a burst of unconditional app_state.write() calls and
            // hangs the WASM main thread (reproduced 5/5 times in visual audit).
            instance_id: self.http.base_url()
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_end_matches('/')
                .to_string(),
            backend_url: Some(self.http.base_url().to_string()),
        })
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.account_id = None;
        self.account_display_name = None;
        Ok(())
    }

    fn is_authenticated(&self) -> bool {
        self.account_id.is_some()
    }

    fn plugin_manifest(&self) -> PluginManifest {
        PluginManifest {
            exec_programs: vec![],
            http_hosts: vec![
                "graph.microsoft.com".to_string(),
                "login.microsoftonline.com".to_string(),
            ],
            description: "Microsoft Teams backend. Connects to Microsoft Graph with a \
                          Bearer token. Dev-only: not shipped in release builds because \
                          Teams' enterprise licensing blocks third-party app-store distribution."
                .to_string(),
            homepage: Some("https://teams.microsoft.com".to_string()),
        }
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let account_id = self.account_id();
        let account_name = self.account_display_name();
        Ok(self.http.get_joined_teams().await?.into_iter().map(|t| Server {
            id: t.id,
            name: t.display_name,
            icon_url: None,
            banner_url: None,
            categories: vec![],
            backend: BackendType::from(crate::SLUG),
            unread_count: 0,
            mention_count: 0,
            account_id: account_id.clone(),
            account_display_name: account_name.clone(),
            default_channel_id: None,
            description: None,
            star_count: None,
            language: None,
            forks_count: None,
            open_issues_count: None,
        }).collect())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let account_id = self.account_id();
        let account_name = self.account_display_name();
        let t = self.http.get_team(id).await?;
        Ok(Server {
            id: t.id,
            name: t.display_name,
            icon_url: None,
            banner_url: None,
            categories: vec![],
            backend: BackendType::from(crate::SLUG),
            unread_count: 0,
            mention_count: 0,
            account_id,
            account_display_name: account_name,
            default_channel_id: None,
            description: None,
            star_count: None,
            language: None,
            forks_count: None,
            open_issues_count: None,
        })
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        Ok(self.http.get_team_channels(server_id).await?.into_iter().map(|ch| Channel {
            // Channel IDs are encoded as "team_id/channel_id" per the plugin's
            // own contract (see module docs at top of lib.rs); every op splits on
            // '/' to dispatch to the teams/channels endpoint vs the chats one.
            // Without the team prefix `get_messages` falls back to /chats/{id}/
            // messages (DM endpoint) and 404s on every team channel.
            id: format!("{server_id}/{}", ch.id),
            name: ch.display_name,
            channel_type: ChannelType::Text,
            server_id: server_id.to_string(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
        }).collect())
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        // id is expected as "team_id/channel_id"
        let (team_id, channel_id) = id.split_once('/').ok_or_else(|| {
            ClientError::Internal(format!("Teams channel id must be 'team_id/channel_id', got '{id}'"))
        })?;
        let channels = self.http.get_team_channels(team_id).await?;
        channels
            .into_iter()
            .find(|c| c.id == channel_id)
            .map(|ch| Channel {
                // Keep the composite id for round-trip consistency.
                id: format!("{team_id}/{}", ch.id),
                name: ch.display_name,
                channel_type: ChannelType::Text,
                server_id: team_id.to_string(),
                unread_count: 0,
                mention_count: 0,
                last_message_id: None,
                forum_tags: None,
                parent_channel_id: None,
                thread_metadata: None,
            })
            .ok_or_else(|| ClientError::NotFound(format!("Channel {id}")))
    }

    // ── Writable messaging (plan-trait-split-readable-vs-writable) ──────────

    fn as_writable_messaging(&self) -> Option<&dyn poly_client::WritableMessagingBackend> {
        Some(self)
    }

    async fn get_messages(&self, channel_id: &str, query: MessageQuery) -> ClientResult<Vec<Message>> {
        let msgs = if let Some((team_id, ch_id)) = channel_id.split_once('/') {
            self.http.get_channel_messages(team_id, ch_id, query.limit).await?
        } else {
            self.http.get_chat_messages(channel_id, query.limit).await?
        };
        Ok(msgs.into_iter().map(|m| self.graph_message_to_poly(m)).collect())
    }

    // ── Social graph (H.3.b — moved to SocialGraphBackend) ──────────────────

    fn as_social_graph(&self) -> Option<&dyn poly_client::SocialGraphBackend> {
        Some(self)
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(vec![])
    }

    // ── DMs and groups (H.3.c — moved to DmsAndGroupsBackend) ──────────────

    fn as_dms_and_groups(&self) -> Option<&dyn poly_client::DmsAndGroupsBackend> {
        Some(self)
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(vec![])
    }

    fn as_settings(&self) -> Option<&dyn poly_client::SettingsBackend> {
        Some(self)
    }

    fn as_view_descriptor(&self) -> Option<&dyn poly_client::ViewDescriptorBackend> {
        Some(self)
    }

    fn as_context_action(&self) -> Option<&dyn poly_client::ContextActionBackend> {
        Some(self)
    }

    // ── Moderation methods moved to ModerationBackend (H.3.a) ────────────────

    fn as_moderation(&self) -> Option<&dyn poly_client::ModerationBackend> {
        Some(self)
    }

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let http = self.http.clone();
            let (tx, rx) = tokio::sync::mpsc::channel::<ClientEvent>(128);
            tokio::spawn(async move {
                loop {
                    match http.poll_events().await {
                        Ok(events) => {
                            for ev in events {
                                if let Some(ce) = teams_event_to_client(&ev)
                                    && tx.send(ce).await.is_err()
                                {
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Teams poll_events error: {e:?}");
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        }
                    }
                }
            });
            Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
        }
        #[cfg(target_arch = "wasm32")]
        {
            Box::pin(futures::stream::empty())
        }
    }

    fn backend_type(&self) -> BackendType {
        BackendType::from(crate::SLUG)
    }

    fn backend_name(&self) -> &str {
        "Teams"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_typing_indicators: false,
            has_roles: false,       // owner/member binary; no role concept
            has_kick: true,
            has_ban: false,         // Teams has no ban concept — hide entirely
            has_timed_ban: false,
            has_channel_mgmt: true, // name + description only (no slow-mode / nsfw / position)
            has_moderation_log: false,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        }
    }

    /// Teams declares one mechanism:
    ///
    /// - `oauth-sandbox` — route the Microsoft Entra ID (AAD) OAuth popup
    ///   through a sandboxed host-managed browser window. The AAD device-code
    ///   flow works without a sandbox; the interactive OAuth popup flow does
    ///   not, because the WASM UI cannot open a full-featured browser window
    ///   itself. Requires `HostCap::SandboxBrowser`.
    ///
    /// Teams does NOT use hCaptcha at the login wall (unlike Discord), so there
    /// is no separate `captcha-sandbox` mechanism — `oauth-sandbox` covers the
    /// only browser-popup scenario in the Teams login flow.
    async fn client_mechanisms(&self) -> ClientResult<Vec<Mechanism>> {
        let oauth_sandbox_enabled = self
            .settings_storage
            .get(SettingsScope::AccountGlobal, "", "oauth-sandbox")
            .map(|v| v == "true")
            .unwrap_or(false);
        Ok(vec![Mechanism {
            id: "oauth-sandbox".to_string(),
            name_key: "plugin-teams-mechanism-oauth-sandbox-label".to_string(),
            enabled: oauth_sandbox_enabled,
            requires_host_cap: Some(HostCap::SandboxBrowser),
            description_key: Some("plugin-teams-mechanism-oauth-sandbox-desc".to_string()),
        }])
    }

    async fn set_client_mechanism(&self, id: &str, enabled: bool) -> ClientResult<()> {
        match id {
            "oauth-sandbox" => self.settings_storage.set(
                SettingsScope::AccountGlobal,
                "",
                "oauth-sandbox",
                if enabled { "true" } else { "false" },
            ),
            _ => Err(ClientError::NotFound(format!("unknown mechanism: {id}"))),
        }
    }


    fn get_signup_method(&self, _server_url: Option<&str>) -> SignupMethod {
        // Last verified 2026-04-30
        SignupMethod::External("https://signup.live.com/signup?lic=1".into())
    }

    fn client_version(&self) -> String {
        self.version_override
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| crate::http::DEFAULT_CLIENT_VERSION.to_string())
    }

    async fn set_client_version_override(
        &self,
        version_override: Option<String>,
    ) -> ClientResult<()> {
        let new_ua = version_override
            .clone()
            .unwrap_or_else(|| crate::http::DEFAULT_CLIENT_VERSION.to_string());
        if let Ok(mut lock) = self.version_override.lock() {
            *lock = version_override;
        }
        self.http.set_user_agent(new_ua);
        Ok(())
    }
}

// ── WritableMessagingBackend (plan-trait-split-readable-vs-writable) ─────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::WritableMessagingBackend for TeamsClient {
    async fn send_message(&self, channel_id: &str, content: MessageContent) -> ClientResult<Message> {
        let text = match content {
            MessageContent::Text(t) => t,
            MessageContent::WithAttachments { text, .. } => text,
        };
        // Channel IDs are "team_id/channel_id"; chat IDs have no slash.
        let m = if let Some((team_id, ch_id)) = channel_id.split_once('/') {
            self.http.send_channel_message(team_id, ch_id, &text).await?
        } else {
            self.http.send_chat_message(channel_id, &text).await?
        };
        Ok(self.graph_message_to_poly(m))
    }
}
