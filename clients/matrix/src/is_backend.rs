//! `impl IsBackend for MatrixClient` — core backend trait implementation.

use async_trait::async_trait;
use futures::stream::Stream;
#[cfg(target_arch = "wasm32")]
use futures::stream;
use poly_client::*;
use std::pin::Pin;

use crate::api;
use crate::http::DEFAULT_CLIENT_VERSION;
use crate::MatrixClient;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl IsBackend for MatrixClient {
    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        use crate::config::MatrixAuthInput;
        let auth_input = MatrixAuthInput::try_from(credentials)?;

        match auth_input {
            MatrixAuthInput::AccessToken(token) => {
                let whoami = self.http.authenticate_with_token(token).await?;
                let profile = self.http.fetch_profile(&whoami.user_id).await.unwrap_or(
                    api::ProfileResponse {
                        displayname: None,
                        avatar_url: None,
                    },
                );
                self.cache_session_profile(&profile);
                let session_state = self.http.session().ok_or_else(|| {
                    ClientError::Internal("session not set after token auth".into())
                })?;
                Ok(self.build_session(&session_state, &profile))
            }
            MatrixAuthInput::UsernamePassword { username, password } => {
                let login = self.http.login_with_password(&username, &password).await?;
                let profile = self
                    .http
                    .fetch_profile(&login.user_id)
                    .await
                    .unwrap_or(api::ProfileResponse {
                        displayname: None,
                        avatar_url: None,
                    });
                self.cache_session_profile(&profile);
                let session_state = self.http.session().ok_or_else(|| {
                    ClientError::Internal("session not set after password auth".into())
                })?;
                Ok(self.build_session(&session_state, &profile))
            }
        }
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.http.logout().await
    }

    fn is_authenticated(&self) -> bool {
        self.http.is_authenticated()
    }

    fn plugin_manifest(&self) -> PluginManifest {
        PluginManifest {
            exec_programs: vec![],
            http_hosts: vec!["<homeserver from account>".to_string()],
            description: "Matrix backend. Federated, end-to-end-encrypted \
                          messaging via the client-server API. Connects to \
                          whichever homeserver each signed-in account specifies \
                          (matrix.org, your own, or any compliant server)."
                .to_string(),
            homepage: Some("https://matrix.org".to_string()),
        }
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let joined = self.http.fetch_joined_rooms().await?;
        let account_id = self.current_account_id()?;
        let display_name = self.current_user_id().unwrap_or_else(|_| account_id.clone());
        let mut servers = Vec::new();

        let homeserver_url = self.homeserver_url().to_string();
        for room_id in &joined.joined_rooms {
            let state = self.http.fetch_room_state(room_id).await.unwrap_or_default();
            if Self::is_space_room(&state) {
                servers.push(Server {
                    id: room_id.clone(),
                    name: Self::extract_room_name(&state, "Unnamed Space"),
                    icon_url: Self::extract_avatar_url(&state, &homeserver_url),
                    banner_url: None,
                    unread_count: 0,
                    mention_count: 0,
                    categories: vec![],
                    backend: BackendType::from(crate::SLUG),
                    account_id: account_id.clone(),
                    account_display_name: display_name.clone(),
                    default_channel_id: None,
                    description: None,
                    star_count: None,
                    language: None,
                    forks_count: None,
                    open_issues_count: None,
                });
            }
        }

        Ok(servers)
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let state = self.http.fetch_room_state(id).await?;
        let account_id = self.current_account_id()?;
        let display_name = self.current_user_id().unwrap_or_else(|_| account_id.clone());
        let homeserver_url = self.homeserver_url().to_string();

        Ok(Server {
            id: id.to_string(),
            name: Self::extract_room_name(&state, "Unnamed Space"),
            icon_url: Self::extract_avatar_url(&state, &homeserver_url),
            banner_url: None,
            unread_count: 0,
            mention_count: 0,
            categories: vec![],
            backend: BackendType::from(crate::SLUG),
            account_id: account_id.clone(),
            default_channel_id: None,
            description: None,
            star_count: None,
            language: None,
            forks_count: None,
            open_issues_count: None,
            account_display_name: display_name,
        })
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        let hierarchy = self.http.fetch_space_hierarchy(server_id).await?;
        let channels: Vec<Channel> = hierarchy
            .rooms
            .iter()
            .filter(|room| room.room_type.as_deref() != Some("m.space"))
            .map(|room| Channel {
                id: room.room_id.clone(),
                name: room.name.clone().unwrap_or_else(|| room.room_id.clone()),
                server_id: server_id.to_string(),
                channel_type: ChannelType::Text,
                unread_count: 0,
                mention_count: 0,
                last_message_id: None,
                forum_tags: None,
                parent_channel_id: None,
                thread_metadata: None,
            })
            .collect();

        Ok(channels)
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        let state = self.http.fetch_room_state(id).await?;

        Ok(Channel {
            id: id.to_string(),
            name: Self::extract_room_name(&state, id),
            server_id: String::new(),
            channel_type: ChannelType::Text,
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
        })
    }

    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let txn_id = uuid::Uuid::new_v4().to_string();
        let body = Self::extract_body(&content);

        let send_req = api::SendMessageRequest {
            msgtype: "m.text".to_string(),
            body: body.clone(),
            formatted_body: None,
            format: None,
            relates_to: None,
        };

        let result = self
            .http
            .send_message(channel_id, &txn_id, &send_req)
            .await?;

        self.build_message_from_send(result.event_id, body)
    }

    // ── Messaging extras (H.4.a — moved to MessagingBackend) ────────────────

    fn as_messaging(&self) -> Option<&dyn poly_client::MessagingBackend> {
        Some(self)
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        let from = if let Some(before) = &query.before {
            before.clone()
        } else {
            let session = self.http.session().ok_or_else(|| {
                ClientError::AuthFailed("not logged in".into())
            })?;
            session.sync_next_batch.unwrap_or_default()
        };

        let messages = if from.is_empty() {
            // No pagination token; do an initial sync to get one
            let sync = self.http.sync(None, Some(0)).await?;
            let prev_batch = sync
                .rooms
                .as_ref()
                .and_then(|rooms| rooms.join.as_ref())
                .and_then(|join| join.get(channel_id))
                .and_then(|room| room.timeline.as_ref())
                .and_then(|tl| tl.prev_batch.clone())
                .unwrap_or(sync.next_batch);

            let limit = u64::from(query.limit.unwrap_or(50));
            let response = self
                .http
                .fetch_messages(channel_id, &prev_batch, "b", Some(limit))
                .await?;

            response
                .chunk
                .iter()
                .filter_map(Self::room_event_to_message)
                .collect::<Vec<_>>()
        } else {
            let dir = if query.after.is_some() { "f" } else { "b" };
            let limit = u64::from(query.limit.unwrap_or(50));
            let response = self
                .http
                .fetch_messages(channel_id, &from, dir, Some(limit))
                .await?;

            response
                .chunk
                .iter()
                .filter_map(Self::room_event_to_message)
                .collect::<Vec<_>>()
        };

        Ok(self.hydrate_message_authors(messages).await)
    }

    // ── Social graph (H.3.b — moved to SocialGraphBackend) ──────────────────

    fn as_social_graph(&self) -> Option<&dyn poly_client::SocialGraphBackend> {
        Some(self)
    }

    async fn get_channel_members(&self, channel_id: &str) -> ClientResult<Vec<User>> {
        let members = self.http.fetch_room_members(channel_id).await?;
        let users: Vec<User> = members
            .chunk
            .iter()
            .filter(|ev| {
                ev.event_type == "m.room.member"
                    && ev
                        .content
                        .get("membership")
                        .and_then(serde_json::Value::as_str)
                        == Some("join")
            })
            .filter_map(|ev| {
                let user_id = ev.state_key.as_deref()?;
                let display_name = ev
                    .content
                    .get("displayname")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(user_id)
                    .to_string();
                let avatar_url = ev
                    .content
                    .get("avatar_url")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);

                Some(User {
                    id: user_id.to_string(),
                    display_name,
                    avatar_url,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from(crate::SLUG),
                })
            })
            .collect();

        Ok(users)
    }

    // ── DMs and groups (H.3.c — moved to DmsAndGroupsBackend) ──────────────

    fn as_dms_and_groups(&self) -> Option<&dyn poly_client::DmsAndGroupsBackend> {
        Some(self)
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(vec![])
    }

    // --- C.1 — UI surface / settings / views / context-actions moved below ---

    fn as_settings(&self) -> Option<&dyn poly_client::SettingsBackend> {
        Some(self)
    }

    fn as_view_descriptor(&self) -> Option<&dyn poly_client::ViewDescriptorBackend> {
        Some(self)
    }

    fn as_context_action(&self) -> Option<&dyn poly_client::ContextActionBackend> {
        Some(self)
    }

    // --- Moderation (H.3.a — moved to ModerationBackend) ---

    fn as_moderation(&self) -> Option<&dyn poly_client::ModerationBackend> {
        Some(self)
    }

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use tokio::sync::mpsc;
            let http = self.http.clone();
            let (tx, rx) = mpsc::channel::<ClientEvent>(128);

            tokio::spawn(async move {
                let mut since = http.sync_next_batch();
                loop {
                    match http.sync(since.as_deref(), Some(30000)).await {
                        Ok(response) => {
                            // Update the batch token
                            since = Some(response.next_batch.clone());
                            http.set_sync_next_batch(response.next_batch);

                            // Process joined rooms
                            if let Some(rooms) = &response.rooms
                                && let Some(joined) = &rooms.join {
                                    for (room_id, room) in joined {
                                        // Timeline events → MessageReceived
                                        if let Some(timeline) = &room.timeline {
                                            for event in &timeline.events {
                                                if let Some(msg) =
                                                    MatrixClient::room_event_to_message(event)
                                                {
                                                    drop(
                                                        tx.send(ClientEvent::MessageReceived {
                                                            channel_id: room_id.clone(),
                                                            message: msg,
                                                        })
                                                        .await,
                                                    );
                                                }
                                            }
                                        }
                                        // Ephemeral events → typing
                                        if let Some(ephemeral) = &room.ephemeral {
                                            for ev in &ephemeral.events {
                                                if ev
                                                    .get("type")
                                                    .and_then(|t| t.as_str())
                                                    == Some("m.typing")
                                                    && let Some(user_ids) = ev
                                                        .get("content")
                                                        .and_then(|c| c.get("user_ids"))
                                                        .and_then(|u| u.as_array())
                                                {
                                                    for uid in user_ids {
                                                        if let Some(user_id) = uid.as_str() {
                                                            drop(
                                                                tx.send(
                                                                    ClientEvent::TypingStarted {
                                                                        channel_id: room_id
                                                                            .clone(),
                                                                        user_id: user_id
                                                                            .to_string(),
                                                                        timestamp: chrono::Utc::now(),
                                                                    },
                                                                )
                                                                .await,
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Matrix sync error: {e:?}");
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        }
                    }
                }
            });

            Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
        }
        #[cfg(target_arch = "wasm32")]
        {
            Box::pin(stream::empty())
        }
    }

    // ── Moderation methods moved to ModerationBackend (H.3.a) ────────────────
    // ── Social graph methods moved to SocialGraphBackend (H.3.b) ─────────────
    // ── DMs and groups moved to DmsAndGroupsBackend (H.3.c) ─────────────────

    /// Invite a user to a server (Matrix Space).
    ///
    fn as_server_admin(&self) -> Option<&dyn poly_client::ServerAdminBackend> {
        Some(self)
    }

    // ── Server admin methods moved to ServerAdminBackend (H.4.b) ─────────────
    // invite_user_to_server → impl ServerAdminBackend below

    fn backend_type(&self) -> BackendType {
        BackendType::from(crate::SLUG)
    }

    fn backend_name(&self) -> &str {
        "Matrix"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            voice: VoiceSupport::None,
            landing: poly_client::LandingPage::Overview,
            // Moderation flags (B-MX)
            has_roles: false,        // Power levels are different; expose differently in UI
            has_kick: true,
            has_ban: true,
            has_timed_ban: false,    // No native timeout primitive
            has_channel_mgmt: true,  // Name + topic only
            has_moderation_log: false, // Expensive synthesis deferred
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        }
    }

    // --- Client-provided UI surface (WP 1 / plan-client-ui-surface) ---

    fn get_signup_method(&self, server_url: Option<&str>) -> SignupMethod {
        let base = server_url.unwrap_or("https://matrix.org");
        SignupMethod::External(format!("{}/_matrix/client/v3/register", base.trim_end_matches('/')))
    }

    fn client_version(&self) -> String {
        self.version_override
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| DEFAULT_CLIENT_VERSION.to_string())
    }

    async fn set_client_version_override(
        &self,
        version_override: Option<String>,
    ) -> ClientResult<()> {
        let new_ua = version_override
            .clone()
            .unwrap_or_else(|| DEFAULT_CLIENT_VERSION.to_string());
        if let Ok(mut lock) = self.version_override.lock() {
            *lock = version_override;
        }
        self.http.set_user_agent(new_ua);
        Ok(())
    }
}
