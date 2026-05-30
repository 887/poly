//! Extracted from lib.rs as part of SOLID B.1 split.
//!
//! Pure structural move — no behaviour change.

use super::super::{DiscordClient, Pin, Stream, stream};
use async_trait::async_trait;
use poly_client::{IsBackend, AuthCredentials, ClientResult, Session, ClientError, User, PresenceStatus, BackendType, PluginManifest, Server, Channel, MessageQuery, Message, Notification, ClientEvent, BackendCapabilities, VideoCaptureCapability, Mechanism, SettingsScope, HostCap, SignupMethod};
#[cfg(feature = "gateway")]
use super::super::gateway_connect_loop;
#[cfg(feature = "gateway")]
use std::sync::Arc;

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl IsBackend for DiscordClient {
    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        let token = match credentials {
            AuthCredentials::Token(t) => t,
            AuthCredentials::EmailPassword { email, password } => {
                self.http.login(&email, &password).await?
            }
            AuthCredentials::OAuth { token } => token,
            AuthCredentials::DeviceCode { .. } | AuthCredentials::PolyServer { .. } => {
                return Err(ClientError::AuthFailed(
                    "Discord requires a user token or email+password".into(),
                ));
            }
        };
        self.http.set_token(token.clone());
        let user = self.http.get_me().await?;
        let user_id = user.id.to_string();
        self.account_id = Some(user_id.clone());
        self.account_display_name = Some(user.username.clone());
        // E.3 — cache Nitro tier from premium_type on successful auth.
        if let Ok(mut info) = self.account_info.lock() {
            info.update_nitro_tier(user.premium_type);
        }
        Ok(Session {
            id: user_id.clone(),
            user: User {
                id: user_id,
                display_name: user.username,
                avatar_url: None,
                presence: PresenceStatus::Online,
                backend: BackendType::from(crate::SLUG),
            },
            token,
            backend: BackendType::from(crate::SLUG),
            icon_emoji: Some("💬".to_string()),
            // Session.instance_id flows into Route URL segments
            // (e.g. /discord/{instance_id}/{account}/{guild}). If the
            // scheme (http://) leaks through, the resulting path
            // contains `://` and the Dioxus router emits PageNotFound,
            // which the on_update handler then "recovers" from by
            // bouncing to some other account's last route. Strip the
            // scheme + trailing slash here, mirroring what matrix and
            // stoat do via their `instance_id()` helpers. backend_url
            // keeps the full URL with scheme — it's the actual HTTP target.
            instance_id: self
                .http
                .base_url()
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
            http_hosts: vec!["discord.com".to_string(), "cdn.discordapp.com".to_string()],
            description: "Discord chat backend. Connects to discord.com with a user token. \
                          Dev-only: not shipped in release builds because Discord's ToS \
                          forbids third-party clients on the app store."
                .to_string(),
            homepage: Some("https://discord.com".to_string()),
        }
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let account_id = self.account_id();
        let account_name = self.account_display_name();
        let cdn_base = self.http.cdn_base_url();
        Ok(self.http.get_guilds().await?.into_iter().map(|g| {
            let gid = g.id.to_string();
            let (icon_url, banner_url) = Self::guild_image_urls(
                &gid, g.icon.as_deref(), g.banner.as_deref(), &cdn_base,
            );
            Server {
                id: g.id.to_string(),
                name: g.name,
                icon_url,
                banner_url,
                categories: vec![],
                backend: BackendType::from(crate::SLUG),
                unread_count: 0,
                mention_count: 0,
                account_id: account_id.clone(),
                account_display_name: account_name.clone(),
                default_channel_id: g.system_channel_id,
                description: None,
                star_count: None,
                language: None,
                forks_count: None,
                open_issues_count: None,
            }
        }).collect())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let account_id = self.account_id();
        let account_name = self.account_display_name();
        let cdn_base = self.http.cdn_base_url();
        let g = self.http.get_guild(id).await?;
        let gid = g.id.to_string();
        let (icon_url, banner_url) = Self::guild_image_urls(
            &gid, g.icon.as_deref(), g.banner.as_deref(), &cdn_base,
        );
        Ok(Server {
            id: g.id.to_string(),
            name: g.name,
            icon_url,
            banner_url,
            categories: vec![],
            backend: BackendType::from(crate::SLUG),
            unread_count: 0,
            mention_count: 0,
            account_id,
            account_display_name: account_name,
            default_channel_id: g.system_channel_id,
            description: None,
            star_count: None,
            language: None,
            forks_count: None,
            open_issues_count: None,
        })
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        use twilight_model::channel::ChannelType as DcChType;
        Ok(self.http.get_guild_channels(server_id).await?.into_iter()
            .filter(|c| matches!(
                c.channel_type,
                DcChType::GuildText
                    | DcChType::GuildAnnouncement
                    | DcChType::GuildForum
                    | DcChType::GuildMedia
                    // Voice channels — needed so the sidebar can render them
                    // and the user can click to invoke join_voice_channel_transport
                    // through the gateway/voice-bridge transport added in phases C/D.
                    | DcChType::GuildVoice
                    | DcChType::GuildStageVoice
            ))
            .map(|c| self.discord_channel_to_poly(c, server_id))
            .collect())
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        let ch = self.http.get_channel(id).await?;
        let server_id = ch.guild_id.map(|gid| gid.to_string()).unwrap_or_default();
        Ok(self.discord_channel_to_poly(ch, &server_id))
    }

    // --- Forum channels (H.2.b — moved to ForumBackend) ---

    fn as_forum(&self) -> Option<&dyn poly_client::ForumBackend> {
        Some(self)
    }

    // --- Thread channels (H.2.c — moved to ThreadsBackend) ---

    fn as_threads(&self) -> Option<&dyn poly_client::ThreadsBackend> {
        Some(self)
    }

    // --- Moderation (H.3.a — moved to ModerationBackend) ---

    fn as_moderation(&self) -> Option<&dyn poly_client::ModerationBackend> {
        Some(self)
    }

    // ── Messaging extras (H.4.a — moved to MessagingBackend) ────────────────

    fn as_messaging(&self) -> Option<&dyn poly_client::MessagingBackend> {
        Some(self)
    }

    // ── Writable messaging (plan-trait-split-readable-vs-writable) ──────────

    fn as_writable_messaging(&self) -> Option<&dyn poly_client::WritableMessagingBackend> {
        Some(self)
    }

    // ── C.1 — moved to sub-traits at the bottom of this file ────────────────

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

    async fn get_messages(&self, channel_id: &str, query: MessageQuery) -> ClientResult<Vec<Message>> {
        let msgs = self.http.get_messages(channel_id, query.limit, query.before.as_deref()).await?;
        Ok(msgs.into_iter().map(|m| self.discord_message_to_poly(m)).collect())
    }

    // ── Social graph methods moved to SocialGraphBackend (H.3.b) ─────────────

    fn as_social_graph(&self) -> Option<&dyn poly_client::SocialGraphBackend> {
        Some(self)
    }

    /// C.3 — DEFERRED missing impl. Discord exposes guild members via
    /// `GET /guilds/{guild_id}/members?limit={n}&after={user_id}` which is
    /// rate-limit-sensitive (the gateway-side GUILD_MEMBERS_CHUNK opcode is
    /// the production path; HTTP is fallback only). Gated on rate-limit
    /// guardrails — implement when `MemberFetchGuard` ships. For now we
    /// return an empty list so the UI falls back to message-author scraping.
    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(vec![])
    }

    // ── DMs and groups (H.3.c — moved to DmsAndGroupsBackend) ──────────────

    fn as_dms_and_groups(&self) -> Option<&dyn poly_client::DmsAndGroupsBackend> {
        Some(self)
    }

    /// C.3 — BY DESIGN empty. Discord has no "list notifications" endpoint;
    /// notifications must be synthesised from `MESSAGE_CREATE` + mention parsing
    /// on the gateway side. The UI already builds its own notification feed from
    /// per-channel unread counts and stored mention events — exposing a list
    /// here would duplicate that surface. Returning empty is the contract.
    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(vec![])
    }

    // C.2 — return cached voice participants for `channel_id`.
    // The cache is populated by `VOICE_STATE_UPDATE` gateway events.
    // Returns an empty list if no participants are cached (not an error).

    // ── Moderation methods moved to ModerationBackend (H.3.a) ────────────────

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        #[cfg(feature = "gateway")]
        {
            if let Some(url) = &self.gateway_url {
                let url = url.clone();
                // Pass a snapshot of the current SuperProperties so the IDENTIFY
                // frame uses the same fingerprint as the HTTP headers (Phase C.2).
                let props = self.http.super_properties();
                let local_user_id = self.account_id();
                // Spawn a task that connects to the gateway WS and streams events.
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<ClientEvent>();
                // C.5/D.2 — wire a back-channel so set_self_mute / start_direct_call
                // can write op 4 / op 13 on the active gateway WS without opening a
                // second connection.
                let (gw_tx, gw_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
                {
                    if let Ok(mut guard) = self.gateway_tx.lock() {
                        *guard = Some(gw_tx);
                    }
                }
                // C.4 — store a clone of the event sender so voice speaking events
                // can be injected from the voice WS loop into the main event stream.
                if let Ok(mut guard) = self.gateway_event_tx.lock() {
                    *guard = Some(tx.clone());
                }
                tokio::spawn(gateway_connect_loop(
                    url,
                    props,
                    tx,
                    Arc::clone(&self.voice_states),
                    local_user_id,
                    gw_rx,
                ));
                return Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(rx));
            }
        }
        // gateway-bridge path: wasm32 + gateway-bridge feature.
        // Opens a browser WebSocket to wss://gateway.discord.gg, stashes
        // VOICE_STATE_UPDATE / VOICE_SERVER_UPDATE credentials for voice joining,
        // and wires a Send+Sync outbound channel so join_voice_channel_transport
        // can push op 4 Voice State Update without holding an Rc.
        #[cfg(all(feature = "gateway-bridge", target_arch = "wasm32"))]
        {
            if let Some(url) = &self.gateway_url {
                let url = url.clone();
                let token = self.http.token().unwrap_or_default();
                let local_user_id = self.account_id();
                let creds = std::sync::Arc::clone(&self.voice_server_creds);
                let gw_tx_arc = std::sync::Arc::clone(&self.gateway_bridge_tx);

                // `start` is async but `event_stream` is sync. Spawn a local future
                // that calls `start` (which opens the WS + spawns the receive loop)
                // then stores the resulting UnboundedSender in `gw_tx_arc`.
                wasm_bindgen_futures::spawn_local(async move {
                    match gateway_bridge::start(url, token, creds, local_user_id).await {
                        Ok(sender) => {
                            if let Ok(mut guard) = gw_tx_arc.lock() {
                                *guard = Some(sender);
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "poly_discord::gateway_bridge",
                                error = %e,
                                "gateway-bridge: start failed"
                            );
                        }
                    }
                });
            }
        }

        // When the `gateway` feature is disabled (WASM plugin target, plain
        // native core consumer), we can't open a WebSocket — return a pending
        // stream. Events arrive via other paths (WIT plugin host, REST poll).
        let _ = &self.gateway_url;
        Box::pin(stream::pending())
    }

    fn backend_type(&self) -> BackendType {
        BackendType::from(crate::SLUG)
    }

    fn backend_name(&self) -> &'static str {
        "Discord"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            has_roles: true,
            has_kick: true,
            has_ban: true,
            has_timed_ban: true,
            has_channel_mgmt: true,
            has_moderation_log: true,
            // Phase Y.4 — Discord is the only backend with the WebCodecs
            // video-capture pipeline (camera + screen share).
            video_capture: VideoCaptureCapability::Full,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        }
    }

    // ── WP 1 / F10 — state-aware context menus ──────────────────────────────



    /// Discord declares two mechanisms:
    ///
    /// - `super-properties` — include `X-Super-Properties` header on every
    ///   request. Default ON. Disabling it breaks Discord login.
    /// - `captcha-sandbox` — route CAPTCHA / hCaptcha login challenges through
    ///   a sandboxed host-managed browser window. Requires
    ///   `HostCap::SandboxBrowser`. Toggle renders as DISABLED-with-tooltip on
    ///   shells that don't advertise the cap.
    async fn client_mechanisms(&self) -> ClientResult<Vec<Mechanism>> {
        let super_props_enabled = self
            .settings_storage
            .get(SettingsScope::AccountGlobal, "", "super-properties")
            .is_none_or(|v| v != "false");
        let captcha_sandbox_enabled = self
            .settings_storage
            .get(SettingsScope::AccountGlobal, "", "captcha-sandbox")
            .map_or(false, |v| v == "true");
        Ok(vec![
            Mechanism {
                id: "super-properties".to_string(),
                name_key: "plugin-discord-mechanism-super-properties-label".to_string(),
                enabled: super_props_enabled,
                requires_host_cap: None,
                description_key: Some(
                    "plugin-discord-mechanism-super-properties-desc".to_string(),
                ),
            },
            Mechanism {
                id: "captcha-sandbox".to_string(),
                name_key: "plugin-discord-mechanism-captcha-sandbox-label".to_string(),
                enabled: captcha_sandbox_enabled,
                requires_host_cap: Some(HostCap::SandboxBrowser),
                description_key: Some(
                    "plugin-discord-mechanism-captcha-sandbox-desc".to_string(),
                ),
            },
        ])
    }

    async fn set_client_mechanism(&self, id: &str, enabled: bool) -> ClientResult<()> {
        match id {
            "super-properties" => self.settings_storage.set(
                SettingsScope::AccountGlobal,
                "",
                "super-properties",
                if enabled { "true" } else { "false" },
            ),
            "captcha-sandbox" => self.settings_storage.set(
                SettingsScope::AccountGlobal,
                "",
                "captcha-sandbox",
                if enabled { "true" } else { "false" },
            ),
            _ => Err(ClientError::NotFound(format!("unknown mechanism: {id}"))),
        }
    }



    // ── Social graph methods moved to SocialGraphBackend (H.3.b) ────────────
    // ── DMs and groups moved to DmsAndGroupsBackend (H.3.c) ─────────────────

    fn as_server_admin(&self) -> Option<&dyn poly_client::ServerAdminBackend> {
        Some(self)
    }

    // ── Server admin methods moved to ServerAdminBackend (H.4.b) ─────────────
    // update_server_banner, invite_user_to_server → impl ServerAdminBackend below

    fn get_signup_method(&self, _server_url: Option<&str>) -> SignupMethod {
        SignupMethod::External("https://discord.com/register".into())
    }

    fn client_version(&self) -> String {
        // Phase B.4: UA comes from SuperProperties — one source of truth.
        self.http.ua()
    }

    async fn set_client_version_override(
        &self,
        version_override: Option<String>,
    ) -> ClientResult<()> {
        // Phase B.5: UA override propagates into super_props.browser_user_agent.
        if let Ok(mut lock) = self.version_override.lock() {
            lock.clone_from(&version_override);
        }
        match version_override {
            Some(ua) => self.http.set_user_agent(&ua),
            None => self.http.clear_user_agent_override(),
        }
        Ok(())
    }
}
