//! `impl IsBackend for StoatClient` and the Bonfire WS event parser.
//!
//! Split out from `lib.rs` in SOLID-audit-stoat D.2 — the IsBackend impl was
//! a 543-line god block that mixed authentication, server/channel fetches,
//! the Bonfire event stream, and the trait-object capability accessors
//! (`as_messaging`, `as_voice_transport`, …). Each capability trait now
//! lives in its own sibling file (`messaging.rs`, `voice_transport.rs`,
//! `moderation.rs`, etc.); this module keeps only the IsBackend dispatch +
//! event-stream plumbing.

use crate::api::{
    self, StoatBulkMessageResponse, StoatChannelUnread, reply_preview_from_message,
};
use crate::config::StoatAuthInput;
use crate::http;
use async_trait::async_trait;
use futures::{
    future,
    stream::{self, Stream},
};
use poly_client::{
    AuthCredentials, BackendCapabilities, BackendType, Channel, ClientError, ClientEvent,
    ClientResult, IsBackend, Message, MessageContent, MessageQuery, MessageReplyPreview,
    Notification, NotificationKind, Server, Session, SignupMethod, User, VoiceParticipant,
    VoiceSupport,
};
use std::collections::HashMap;
use std::pin::Pin;

use super::StoatClient;

// ── Bonfire reconnect policy ──────────────────────────────────────────────────

/// First delay before retrying a dropped Bonfire WS connection.
const INITIAL_RECONNECT_BACKOFF: std::time::Duration = std::time::Duration::from_secs(1);

/// Ceiling for the exponential reconnect backoff. A wedged homeserver must not
/// be hammered, but a returning laptop must reconnect within ~30 s.
const MAX_RECONNECT_BACKOFF: std::time::Duration = std::time::Duration::from_secs(30);

/// Double the backoff, capped at [`MAX_RECONNECT_BACKOFF`].
fn next_reconnect_backoff(current: std::time::Duration) -> std::time::Duration {
    current.saturating_mul(2).min(MAX_RECONNECT_BACKOFF)
}

impl StoatClient {
    pub(crate) fn build_session(&self, authenticated: api::StoatAuthenticatedSession) -> Session {
        Session {
            id: authenticated.session_id,
            user: authenticated.user,
            token: authenticated.token,
            backend: BackendType::from(crate::SLUG),
            icon_emoji: Some("🦦".to_string()),
            instance_id: self.instance_id(),
            backend_url: Some(self.base_url().to_string()),
        }
    }

    pub(crate) fn current_account_metadata(&self) -> ClientResult<(String, String)> {
        let session = self.http.session().ok_or_else(|| {
            ClientError::AuthFailed("Stoat client is not authenticated".to_string())
        })?;

        let account_id = session.user_id.ok_or_else(|| {
            ClientError::Internal("Stoat session is missing the authenticated user id".to_string())
        })?;

        let account_display_name = session
            .user_display_name
            .unwrap_or_else(|| account_id.clone());

        Ok((account_id, account_display_name))
    }

    pub(crate) fn index_unreads(
        unreads: Vec<StoatChannelUnread>,
    ) -> HashMap<String, StoatChannelUnread> {
        unreads
            .into_iter()
            .map(|unread| (unread.key.channel.clone(), unread))
            .collect()
    }

    pub(crate) fn unread_count_for_channel(
        unread_index: &HashMap<String, StoatChannelUnread>,
        channel_id: &str,
    ) -> u32 {
        unread_index
            .get(channel_id)
            .map_or(0, StoatChannelUnread::approximate_unread_count)
    }

    pub(crate) fn current_user_id(&self) -> Option<String> {
        self.http.session().and_then(|session| session.user_id)
    }

    pub(crate) fn map_messages_response(
        &self,
        response: StoatBulkMessageResponse,
        autumn_base_url: Option<&str>,
    ) -> Vec<Message> {
        let current_user_id = self.current_user_id();
        let (raw_messages, bundled_users, bundled_members) = response.into_parts();

        let messages_with_replies: Vec<(Message, Option<String>)> = raw_messages
            .into_iter()
            .map(|raw| {
                let reply_id = raw.primary_reply_id().map(str::to_string);
                let message = raw.into_poly_message(
                    &bundled_users,
                    &bundled_members,
                    current_user_id.as_deref(),
                    autumn_base_url,
                );
                (message, reply_id)
            })
            .collect();

        let preview_index: HashMap<String, MessageReplyPreview> = messages_with_replies
            .iter()
            .map(|(message, _)| (message.id.clone(), reply_preview_from_message(message)))
            .collect();

        let mut messages: Vec<Message> = messages_with_replies
            .into_iter()
            .map(|(mut message, reply_id)| {
                message.reply_to =
                    reply_id.and_then(|reply_id| preview_index.get(&reply_id).cloned());
                message
            })
            .collect();

        messages.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| left.id.cmp(&right.id))
        });

        messages
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl IsBackend for StoatClient {
    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        let authenticated = match StoatAuthInput::try_from(credentials)? {
            StoatAuthInput::SessionToken(token) => self.http.authenticate_with_token(token).await?,
            StoatAuthInput::EmailPassword { email, password } => {
                self.http
                    .login_with_password(&email, &password, Some("Poly"))
                    .await?
            }
        };

        Ok(self.build_session(authenticated))
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.http.logout().await
    }

    fn is_authenticated(&self) -> bool {
        self.http.is_authenticated()
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let (servers, unreads, root_config) = future::try_join3(
            self.http.fetch_my_servers(),
            self.http.fetch_unreads(),
            self.http.fetch_server_config(),
        )
        .await?;

        let (account_id, account_display_name) = self.current_account_metadata()?;
        let unread_index = Self::index_unreads(unreads);
        let autumn_base_url = root_config.autumn_base_url();

        Ok(servers
            .into_iter()
            .map(|s| {
                let (unread_count, mention_count) = s
                    .channels
                    .iter()
                    .filter_map(|channel_id| unread_index.get(channel_id))
                    .fold((0_u32, 0_u32), |(unreads_acc, mentions_acc), unread| {
                        (
                            unreads_acc.saturating_add(unread.approximate_unread_count()),
                            mentions_acc.saturating_add(unread.mention_count()),
                        )
                    });
                s.into_poly_server(
                    account_id.clone(),
                    account_display_name.clone(),
                    unread_count,
                    mention_count,
                    autumn_base_url,
                )
            })
            .collect())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let (server, unreads, root_config) = future::try_join3(
            self.http.fetch_server(id),
            self.http.fetch_unreads(),
            self.http.fetch_server_config(),
        )
        .await?;
        let (account_id, account_display_name) = self.current_account_metadata()?;
        let unread_index = Self::index_unreads(unreads);
        let autumn_base_url = root_config.autumn_base_url();

        let (unread_count, mention_count) = server
            .channels
            .iter()
            .filter_map(|channel_id| unread_index.get(channel_id))
            .fold((0_u32, 0_u32), |(unreads_acc, mentions_acc), unread| {
                (
                    unreads_acc.saturating_add(unread.approximate_unread_count()),
                    mentions_acc.saturating_add(unread.mention_count()),
                )
            });

        Ok(server.into_poly_server(
            account_id,
            account_display_name,
            unread_count,
            mention_count,
            autumn_base_url,
        ))
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        let (server, unreads) =
            future::try_join(self.http.fetch_server(server_id), self.http.fetch_unreads()).await?;
        let unread_index = Self::index_unreads(unreads);

        let channels = future::try_join_all(
            server
                .channels
                .iter()
                .map(|channel_id| self.http.fetch_channel(channel_id)),
        )
        .await?;

        channels
            .into_iter()
            .map(|channel| {
                let unread = unread_index.get(&channel.id);
                let unread_count = unread
                    .map_or(0, StoatChannelUnread::approximate_unread_count);
                let mention_count = unread.map_or(0, StoatChannelUnread::mention_count);

                channel.into_poly_server_channel(unread_count, mention_count)
            })
            .collect()
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        let (channel, unreads) =
            future::try_join(self.http.fetch_channel(id), self.http.fetch_unreads()).await?;
        let unread_index = Self::index_unreads(unreads);
        let unread = unread_index.get(&channel.id);
        let unread_count = unread
            .map_or(0, StoatChannelUnread::approximate_unread_count);
        let mention_count = unread.map_or(0, StoatChannelUnread::mention_count);

        channel.into_poly_server_channel(unread_count, mention_count)
    }

    // ── Messaging extras (H.4.a — moved to MessagingBackend) ────────────────

    fn as_messaging(&self) -> Option<&dyn poly_client::MessagingBackend> {
        Some(self)
    }

    // ── Writable messaging (plan-trait-split-readable-vs-writable) ──────────

    fn as_writable_messaging(&self) -> Option<&dyn poly_client::WritableMessagingBackend> {
        Some(self)
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        let (response, root_config) = future::try_join(
            self.http.fetch_messages(channel_id, &query),
            self.http.fetch_server_config(),
        )
        .await?;
        Ok(self.map_messages_response(response, root_config.autumn_base_url()))
    }

    // ── Social graph (H.3.b — moved to SocialGraphBackend) ──────────────────

    fn as_social_graph(&self) -> Option<&dyn poly_client::SocialGraphBackend> {
        Some(self)
    }

    async fn get_channel_members(&self, channel_id: &str) -> ClientResult<Vec<User>> {
        let (channel, root_config) = future::try_join(
            self.http.fetch_channel(channel_id),
            self.http.fetch_server_config(),
        )
        .await?;
        let autumn_base_url = root_config.autumn_base_url();

        if let Some(server_id) = channel.server.clone() {
            let members_response = self.http.fetch_server_members(&server_id).await?;
            let user_index: HashMap<String, api::StoatUser> = members_response
                .users
                .into_iter()
                .map(|user| (user.id.clone(), user))
                .collect();

            return Ok(members_response
                .members
                .into_iter()
                .filter(|member| member.key.server == server_id)
                .filter_map(|member| {
                    let mut user = user_index
                        .get(&member.key.user)
                        .cloned()?
                        .into_poly_user_with_autumn(autumn_base_url);

                    if let Some(nickname) = member.nickname {
                        user.display_name = nickname;
                    }
                    if let Some(avatar_url) = member
                        .avatar
                        .and_then(|avatar| avatar.download_url(autumn_base_url))
                    {
                        user.avatar_url = Some(avatar_url);
                    }

                    Some(user)
                })
                .collect());
        }

        if channel.is_group() {
            return Ok(self
                .http
                .fetch_group_members(channel_id)
                .await?
                .into_iter()
                .map(|user| user.into_poly_user_with_autumn(autumn_base_url))
                .collect());
        }

        if channel.is_direct_message() || channel.is_saved_messages() {
            let recipient_ids = channel
                .recipients
                .clone()
                .unwrap_or_else(|| channel.user.into_iter().collect());

            return future::try_join_all(
                recipient_ids
                    .iter()
                    .map(|user_id| self.http.fetch_user(user_id)),
            )
            .await
            .map(|users| {
                users
                    .into_iter()
                    .map(|user| user.into_poly_user_with_autumn(autumn_base_url))
                    .collect()
            });
        }

        Err(ClientError::NotSupported(format!(
            "Stoat channel {channel_id} does not expose member lists"
        )))
    }

    // ── DMs and groups (H.3.c — moved to DmsAndGroupsBackend) ──────────────

    fn as_dms_and_groups(&self) -> Option<&dyn poly_client::DmsAndGroupsBackend> {
        Some(self)
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        let self_user = self.http.fetch_self().await?;
        let account_id = self.current_account_metadata()?.0;

        let mut notifications = future::try_join_all(
            self_user
                .relations
                .into_iter()
                .filter(|relation| relation.status == api::StoatRelationshipStatus::Incoming)
                .map(|relation| {
                    let account_id = account_id.clone();
                    async move {
                        let user = self.http.fetch_user(&relation.user_id).await?;
                        Ok(Notification {
                            id: format!("stoat-friend-request-{}", user.id),
                            kind: NotificationKind::FriendRequest {
                                from_user_id: user.id.clone(),
                            },
                            backend: BackendType::from(crate::SLUG),
                            account_id: account_id.clone(),
                            timestamp: chrono::Utc::now(),
                            read: false,
                            preview: format!(
                                "{} sent you a friend request",
                                user.display_name.unwrap_or(user.username)
                            ),
                        })
                    }
                }),
        )
        .await?;

        notifications.sort_by_key(|n| std::cmp::Reverse(n.timestamp));
        Ok(notifications)
    }

    // ── Moderation methods moved to ModerationBackend (H.3.a) ────────────────

    fn as_moderation(&self) -> Option<&dyn poly_client::ModerationBackend> {
        Some(self)
    }

    fn as_voice_transport(&self) -> Option<&dyn poly_client::VoiceTransportBackend> {
        Some(self)
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

    // lint-allow-unused: event_stream spans three cfg-gated platform arms (native WS, wasm32 WS,
    // fallback empty) that cannot be split without losing the cfg context.
    #[allow(clippy::too_many_lines)]
    #[cfg_attr(not(feature = "voice"), allow(unused_variables))]
    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use tokio::sync::mpsc;
            use tokio_tungstenite::tungstenite::Message as WsMessage;

            let Some(ws_url) = self.http.ws_url() else {
                return Box::pin(stream::empty());
            };
            let Some(token) = self.http.session().map(|s| s.token) else {
                return Box::pin(stream::empty());
            };

            let (tx, rx) = mpsc::channel::<ClientEvent>(128);

            // C.1 — outbound WS write channel (typing indicators, heartbeat, etc.)
            // The sender is stored on `StoatClient` so `send_typing` can write
            // `ChannelStartTyping` frames without holding a mutable reference to
            // the WS stream (which lives inside the spawned task).
            let (ws_out_tx, mut ws_out_rx) = mpsc::unbounded_channel::<String>();
            if let Ok(mut guard) = self.ws_write_tx.lock() {
                *guard = Some(Box::new(move |json: String| {
                    // Best-effort: ignore send errors (WS may not be connected yet).
                    drop(ws_out_tx.send(json));
                }));
            }

            let http = self.http.clone();

            tokio::spawn(async move {
                use futures::{SinkExt, StreamExt};

                // Media base URL for attachment download links. Fetched once —
                // it is instance configuration, not per-message state.
                let autumn_base_url = http
                    .fetch_server_config()
                    .await
                    .ok()
                    .and_then(|cfg| cfg.autumn_base_url().map(str::to_string));

                let mut backoff = INITIAL_RECONNECT_BACKOFF;
                loop {
                    // `receiver_gone` distinguishes "our consumer dropped the
                    // stream" (terminate the task — otherwise it would poll the
                    // homeserver forever, surviving logout) from "the socket
                    // died" (reconnect — a 10-second wifi blip must not kill
                    // live events for the rest of the process lifetime).
                    let mut receiver_gone = false;

                    let connected = match tokio_tungstenite::connect_async(&ws_url).await {
                        Ok((mut ws_stream, _)) => {
                            let auth_msg =
                                serde_json::json!({"type": "Authenticate", "token": token});
                            if ws_stream
                                .send(WsMessage::Text(auth_msg.to_string().into()))
                                .await
                                .is_ok()
                            {
                                Some(ws_stream)
                            } else {
                                tracing::warn!("Bonfire WS authenticate send failed");
                                None
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Bonfire WS connect failed: {e}");
                            None
                        }
                    };

                    if let Some(mut ws_stream) = connected {
                        backoff = INITIAL_RECONNECT_BACKOFF;
                        if tx
                            .send(ClientEvent::ConnectionStateChanged {
                                backend: BackendType::from(crate::SLUG),
                                connected: true,
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }

                        loop {
                            tokio::select! {
                                // Inbound: Bonfire events → parse → forward to event channel.
                                msg = ws_stream.next() => {
                                    match msg {
                                        Some(Ok(WsMessage::Text(text))) => {
                                            if let Ok(event_json) =
                                                serde_json::from_str::<serde_json::Value>(&text)
                                                && let Some(ev) = parse_bonfire_event(
                                                    &event_json,
                                                    autumn_base_url.as_deref(),
                                                )
                                                && tx.send(ev).await.is_err()
                                            {
                                                receiver_gone = true;
                                                break;
                                            }
                                        }
                                        Some(Ok(WsMessage::Close(_)) | Err(_)) | None => break,
                                        _ => {}
                                    }
                                }
                                // Outbound: write commands from send_typing / future callers.
                                // `ws_out_rx` outlives each socket, so typing writes
                                // queued during a reconnect are drained by the next one.
                                Some(json) = ws_out_rx.recv() => {
                                    if ws_stream
                                        .send(WsMessage::Text(json.into()))
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    if receiver_gone {
                        break;
                    }
                    if tx
                        .send(ClientEvent::ConnectionStateChanged {
                            backend: BackendType::from(crate::SLUG),
                            connected: false,
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                    tokio::time::sleep(backoff).await;
                    backoff = next_reconnect_backoff(backoff);
                }
            });

            Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
        }
        #[cfg(target_arch = "wasm32")]
        {
            // WASM Bonfire WS — mirror the native path using gloo_net instead
            // of tokio_tungstenite, and spawn_local instead of tokio::spawn.
            // Events are bridged through a futures::channel::mpsc so the
            // returned stream is Send (required by the IsBackend trait).
            let ws_url = match self.http.ws_url() {
                Some(url) => url,
                None => {
                    // Not yet authenticated — emit Connected so the UI unblocks,
                    // then end the stream. Voice joins will retry when ready.
                    return Box::pin(stream::once(async {
                        ClientEvent::ConnectionStateChanged {
                            backend: BackendType::from(crate::SLUG),
                            connected: true,
                        }
                    }));
                }
            };
            let token = match self.http.session().map(|s| s.token) {
                Some(t) => t,
                None => {
                    return Box::pin(stream::once(async {
                        ClientEvent::ConnectionStateChanged {
                            backend: BackendType::from(crate::SLUG),
                            connected: true,
                        }
                    }));
                }
            };

            let (tx, rx) = futures::channel::mpsc::unbounded::<ClientEvent>();
            let http = self.http.clone();

            wasm_bindgen_futures::spawn_local(async move {
                use futures::StreamExt as _;
                use gloo_net::websocket::Message as GlooWsMsg;
                use gloo_net::websocket::futures::WebSocket;

                // Media base URL for attachment download links (fetched once).
                let autumn_base_url = http
                    .fetch_server_config()
                    .await
                    .ok()
                    .and_then(|cfg| cfg.autumn_base_url().map(str::to_string));

                let mut backoff = INITIAL_RECONNECT_BACKOFF;
                loop {
                    // See the native arm: `receiver_gone` terminates the task,
                    // a dead socket only triggers a backed-off reconnect.
                    let mut receiver_gone = false;

                    let opened = match WebSocket::open(&ws_url) {
                        Ok(ws) => {
                            let (mut write, read) = futures::StreamExt::split(ws);
                            let auth_msg =
                                serde_json::json!({"type": "Authenticate", "token": token});
                            match futures::SinkExt::send(
                                &mut write,
                                GlooWsMsg::Text(auth_msg.to_string()),
                            )
                            .await
                            {
                                Ok(()) => Some(read),
                                Err(e) => {
                                    tracing::warn!(
                                        "Bonfire WS authenticate failed (WASM): {e:?}"
                                    );
                                    None
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Bonfire WS open failed (WASM): {e:?}");
                            None
                        }
                    };

                    if let Some(mut read) = opened {
                        backoff = INITIAL_RECONNECT_BACKOFF;
                        if tx
                            .unbounded_send(ClientEvent::ConnectionStateChanged {
                                backend: BackendType::from(crate::SLUG),
                                connected: true,
                            })
                            .is_err()
                        {
                            break;
                        }

                        // Forward all Bonfire text events through parse_bonfire_event.
                        while let Some(msg) = read.next().await {
                            match msg {
                                Ok(GlooWsMsg::Text(text)) => {
                                    if let Ok(event_json) =
                                        serde_json::from_str::<serde_json::Value>(&text)
                                        && let Some(ev) = parse_bonfire_event(
                                            &event_json,
                                            autumn_base_url.as_deref(),
                                        )
                                        && tx.unbounded_send(ev).is_err()
                                    {
                                        receiver_gone = true; // stream consumer dropped
                                        break;
                                    }
                                }
                                Ok(GlooWsMsg::Bytes(_)) => {}
                                Err(e) => {
                                    tracing::warn!("Bonfire WS error (WASM): {e:?}");
                                    break;
                                }
                            }
                        }
                    }

                    if receiver_gone {
                        break;
                    }
                    if tx
                        .unbounded_send(ClientEvent::ConnectionStateChanged {
                            backend: BackendType::from(crate::SLUG),
                            connected: false,
                        })
                        .is_err()
                    {
                        break;
                    }
                    let backoff_ms =
                        i32::try_from(backoff.as_millis()).unwrap_or(i32::MAX);
                    crate::voice_common::sleep_ms(backoff_ms).await;
                    backoff = next_reconnect_backoff(backoff);
                }
            });

            Box::pin(rx)
        }
    }

    fn backend_type(&self) -> BackendType {
        BackendType::from(crate::SLUG)
    }

    fn backend_name(&self) -> &'static str {
        "Stoat"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            // G.1 — expose voice support when the voice feature is compiled in (native)
            // OR on wasm32 (voice_wasm.rs path, shipped in `docs/plans/plan-stoat-voice-wasm.md`).
            // The test-stoat mock provides the required join_call + Vortex WS endpoints.
            #[cfg(any(feature = "voice", target_arch = "wasm32"))]
            voice: VoiceSupport::Full,
            #[cfg(not(any(feature = "voice", target_arch = "wasm32")))]
            voice: VoiceSupport::None,
            landing: poly_client::LandingPage::DirectMessages,
            has_roles: true,
            has_kick: true,
            has_ban: true,
            has_timed_ban: true,
            has_channel_mgmt: true,
            has_moderation_log: false,
            // Phase C of `plan-stoat-video-wasm.md` — video capture is `None`
            // until the WebCodecs pipeline actually encodes and decodes.
            //
            // status: gated — `video_wasm_capture::start_video_capture` acquires
            // the camera but its encoder is a skeleton (`send_h264_nal` is never
            // called), and `video_wasm_playback::decode_and_draw` only logs. So
            // advertising `Full` turned the camera light on, flipped `is_video_on`
            // true, and sent zero frames — an LSP break: the capability contract
            // says "this backend can capture video" and the impl cannot.
            // Re-open trigger: flip this back to `Full` in the same change that
            // lands the `VideoEncoder` output callback + `VideoDecoder` canvas
            // draw, so the flag and the pipeline ship together.
            #[cfg(target_arch = "wasm32")]
            video_capture: poly_client::VideoCaptureCapability::None,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        }
    }

    // ── Social graph methods moved to SocialGraphBackend (H.3.b) ─────────────
    // ── DMs and groups moved to DmsAndGroupsBackend (H.3.c) ─────────────────

    // invite_user_to_server → impl ServerAdminBackend below (C.4).

    fn as_server_admin(&self) -> Option<&dyn poly_client::ServerAdminBackend> {
        Some(self)
    }

    fn get_signup_method(&self, server_url: Option<&str>) -> SignupMethod {
        let base = server_url.unwrap_or("https://app.stoat.chat");
        SignupMethod::External(base.trim_end_matches('/').to_string())
    }

    fn client_version(&self) -> String {
        self.version_override
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| http::DEFAULT_CLIENT_VERSION.to_string())
    }

    async fn set_client_version_override(
        &self,
        version_override: Option<String>,
    ) -> ClientResult<()> {
        let new_ua = version_override
            .clone()
            .unwrap_or_else(|| http::DEFAULT_CLIENT_VERSION.to_string());
        if let Ok(mut lock) = self.version_override.lock() {
            *lock = version_override;
        }
        self.http.set_user_agent(new_ua);
        Ok(())
    }
}

/// Parser for the Bonfire WebSocket event protocol — translates JSON event
/// frames into `poly_client::ClientEvent`. Called from `event_stream` above
/// on both native and wasm32 targets.
// Complexity comes from the event-type dispatch table, not from nesting.
#[allow(clippy::cognitive_complexity)]
pub fn parse_bonfire_event(
    json: &serde_json::Value,
    autumn_base_url: Option<&str>,
) -> Option<ClientEvent> {
    match json.get("type")?.as_str()? {
        "Message" => {
            let channel_id = json.get("channel")?.as_str()?.to_string();
            let msg_json = json.get("message")?;
            // Decode through the SAME path as the REST fetch
            // (`map_messages_response` → `StoatMessage::into_poly_message`) so a
            // live message and a refetched one agree on timestamp (ULID-derived,
            // not `Utc::now()`), author name/avatar, attachments and reactions.
            // Hand-rolling the mapping here made a live message sort by local
            // clock and re-materialise elsewhere on refetch.
            let stoat_msg: api::StoatMessage =
                serde_json::from_value(msg_json.clone()).ok()?;
            let message = stoat_msg.into_poly_message(
                &HashMap::new(),
                &HashMap::new(),
                None,
                autumn_base_url,
            );
            Some(ClientEvent::MessageReceived { channel_id, message })
        }
        "ChannelStartTyping" => {
            let channel_id = json.get("id")?.as_str()?.to_string();
            let user_id = json.get("user")?.as_str()?.to_string();
            Some(ClientEvent::TypingStarted {
                channel_id,
                user_id,
                timestamp: chrono::Utc::now(),
            })
        }

        // F.6 — Bonfire WebSocket voice events (emitted by test-stoat mock).
        // These mirror the same events the Vortex WS sends, but delivered
        // via the existing Bonfire connection so the UI event_stream consumer
        // gets voice updates without a separate WS subscription.
        "VoiceUserJoined" => {
            let channel_id = json.get("channel_id")?.as_str()?.to_string();
            let user_id = json.get("user_id")?.as_str()?.to_string();
            let display_name = json
                .get("display_name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&user_id)
                .to_string();
            let avatar_url = json
                .get("avatar_url")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let participant = VoiceParticipant {
                user: User {
                    id: user_id,
                    display_name,
                    avatar_url,
                    presence: poly_client::PresenceStatus::Online,
                    backend: BackendType::from(crate::SLUG),
                },
                is_muted: json
                    .get("is_muted")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                is_deafened: false,
                is_streaming: false,
                is_video_on: false,
                is_speaking: false,
            };
            Some(ClientEvent::VoiceUserJoined { channel_id, participant })
        }

        "VoiceUserLeft" => {
            let channel_id = json.get("channel_id")?.as_str()?.to_string();
            let user_id = json.get("user_id")?.as_str()?.to_string();
            Some(ClientEvent::VoiceUserLeft { channel_id, user_id })
        }

        "VoiceSpeakingUpdate" => {
            let channel_id = json.get("channel_id")?.as_str()?.to_string();
            let user_id = json.get("user_id")?.as_str()?.to_string();
            let is_speaking = json
                .get("speaking")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            Some(ClientEvent::VoiceSpeakingUpdate { channel_id, user_id, is_speaking })
        }

        // Bonfire WS sends {"type":"Authenticated"} as its first message after
        // successful token validation. Translate this into ConnectionStateChanged so
        // the event_stream listener can mark the account as Connected in ClientManager.
        "Authenticated" => Some(ClientEvent::ConnectionStateChanged {
            backend: BackendType::from(crate::SLUG),
            connected: true,
        }),

        _ => None,
    }
}

// ── WritableMessagingBackend (plan-trait-split-readable-vs-writable) ─────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::WritableMessagingBackend for StoatClient {
    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        self.send_message_internal(channel_id, content, None).await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{
        next_reconnect_backoff, parse_bonfire_event, INITIAL_RECONNECT_BACKOFF,
        MAX_RECONNECT_BACKOFF,
    };
    use poly_client::ClientEvent;

    #[test]
    fn reconnect_backoff_grows_and_is_capped() {
        let mut d = INITIAL_RECONNECT_BACKOFF;
        assert_eq!(next_reconnect_backoff(d), INITIAL_RECONNECT_BACKOFF * 2_u32);
        for _ in 0..32_u32 {
            d = next_reconnect_backoff(d);
        }
        assert_eq!(d, MAX_RECONNECT_BACKOFF, "backoff must saturate, not overflow");
    }

    /// A live Bonfire `Message` must derive its timestamp from the ULID in
    /// `_id`, exactly like the REST path (`StoatMessage::into_poly_message`).
    /// Stamping `Utc::now()` made the same message sort differently depending on
    /// whether it arrived live or was refetched.
    #[test]
    fn bonfire_message_timestamp_comes_from_the_ulid_not_the_clock() {
        // 01ARZ3NDEK... is the canonical ULID example: 2016-07-30T23:54:10.259Z.
        let json = serde_json::json!({
            "type": "Message",
            "channel": "CHAN1",
            "message": {
                "_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
                "channel": "CHAN1",
                "author": "USER1",
                "content": "hi",
            }
        });
        let ev = parse_bonfire_event(&json, None).expect("Message event should parse");
        let ClientEvent::MessageReceived { channel_id, message } = ev else {
            panic!("expected MessageReceived");
        };
        assert_eq!(channel_id, "CHAN1");
        assert_eq!(message.id, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        assert_eq!(message.timestamp.timestamp_millis(), 1_469_922_850_259);
        assert!(
            message.timestamp < chrono::Utc::now(),
            "ULID timestamp, not the local clock"
        );
    }

    /// A message whose only payload is an attachment used to be dropped
    /// entirely, because the old parser required a `content` string.
    #[test]
    fn bonfire_message_without_content_is_not_dropped() {
        let json = serde_json::json!({
            "type": "Message",
            "channel": "CHAN1",
            "message": {
                "_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
                "channel": "CHAN1",
                "author": "USER1",
                "attachments": [{
                    "_id": "FILE1",
                    "tag": "attachments",
                    "filename": "cat.png",
                    "content_type": "image/png",
                    "size": 10_u64,
                }],
            }
        });
        let ev = parse_bonfire_event(&json, Some("https://autumn.test"))
            .expect("attachment-only message should still surface");
        let ClientEvent::MessageReceived { message, .. } = ev else {
            panic!("expected MessageReceived");
        };
        assert_eq!(message.attachments.len(), 1, "attachment must survive");
    }

    /// The bundled `user` object on the WS payload must populate the author so a
    /// live message does not render with a blank name until refetch.
    #[test]
    fn bonfire_message_resolves_the_bundled_author() {
        let json = serde_json::json!({
            "type": "Message",
            "channel": "CHAN1",
            "message": {
                "_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
                "channel": "CHAN1",
                "author": "USER1",
                "content": "hi",
                "user": { "_id": "USER1", "username": "otter", "discriminator": "0001", "online": true },
            }
        });
        let ev = parse_bonfire_event(&json, None).expect("Message event should parse");
        let ClientEvent::MessageReceived { message, .. } = ev else {
            panic!("expected MessageReceived");
        };
        assert_eq!(message.author.id, "USER1");
        assert_eq!(message.author.display_name, "otter");
    }
}
