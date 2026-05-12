//! Discord REST API v10 HTTP client.

use std::sync::{Arc, Mutex};

use poly_client::ClientError;
use poly_host_bridge::http::HttpClient;

use crate::api::{
    DiscordActiveThreadsResponse, DiscordArchivedThreadsResponse, DiscordAuditLogResponse,
    DiscordBan, DiscordChannel, DiscordGuild, DiscordGuildMember, DiscordMessage, DiscordRole,
    DiscordUser,
};
use crate::super_properties::SuperProperties;

/// Default User-Agent — the browser-style UA that the Linux Discord desktop
/// client sends.  This is NOT a bot UA; it must never contain "DiscordBot".
///
/// Phase B replaces the old `poly-discord/0.0.0 (DiscordBot ...)` constant.
/// The value here is the fallback used before `SuperProperties` is constructed
/// (e.g. in the `test_version_override_clear_restores_default` test which
/// hard-codes this string).
pub const DEFAULT_CLIENT_VERSION: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
     (KHTML, like Gecko) discord/0.0.354133 Chrome/130.0.0.0 Electron/32.2.7 Safari/537.36";

pub struct DiscordHttpClient {
    base_url: String,
    token: Mutex<Option<String>>,
    http: HttpClient,
    /// Shared, hot-swappable super-properties.  The UA and X-Super-Properties
    /// are both derived from this — one source of truth (Phase B.4 + B.6).
    super_props: Arc<Mutex<SuperProperties>>,
    /// Optional UA override set via `set_user_agent`.  When `Some`, it is
    /// propagated into `super_props.browser_user_agent` (Phase B.5).
    ua_override: Arc<Mutex<Option<String>>>,
}

impl DiscordHttpClient {
    pub fn new(base_url: String) -> Self {
        let props = SuperProperties::for_platform(
            &crate::build_info::BuildInfo::default(),
            "en-US",
        );
        Self {
            base_url,
            token: Mutex::new(None),
            http: HttpClient::new(),
            super_props: Arc::new(Mutex::new(props)),
            ua_override: Arc::new(Mutex::new(None)),
        }
    }

    /// Construct with a pre-scraped `BuildInfo` (Phase A.5).
    #[must_use]
    pub fn with_build_info(
        base_url: String,
        build_info: &crate::build_info::BuildInfo,
        version_override: Option<String>,
    ) -> Self {
        let mut props = SuperProperties::for_platform(build_info, "en-US");
        if let Some(ref ua) = version_override {
            props.apply_ua_override(ua);
        }
        Self {
            base_url,
            token: Mutex::new(None),
            http: HttpClient::new(),
            super_props: Arc::new(Mutex::new(props)),
            ua_override: Arc::new(Mutex::new(version_override)),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Return the CDN base URL for guild icons / attachments.
    /// For real Discord, icon hashes are served from cdn.discordapp.com.
    /// For self-hosted Spacebar / test servers, the base URL itself acts as CDN.
    pub fn cdn_base_url(&self) -> String {
        if self.base_url.contains("discord.com") || self.base_url.contains("discordapp.com") {
            "https://cdn.discordapp.com".to_string()
        } else {
            self.base_url.trim_end_matches('/').to_string()
        }
    }

    pub fn set_token(&self, token: String) {
        if let Ok(mut lock) = self.token.lock() {
            *lock = Some(token);
        }
    }

    /// Update the User-Agent string sent with every request.
    ///
    /// The override is also propagated into `super_props.browser_user_agent`
    /// so HTTP and gateway IDENTIFY stay consistent (Phase B.5).
    pub fn set_user_agent(&self, ua: String) {
        if let Ok(mut lock) = self.ua_override.lock() {
            *lock = Some(ua.clone());
        }
        if let Ok(mut props) = self.super_props.lock() {
            props.apply_ua_override(&ua);
        }
    }

    /// Clear the UA override and reset `browser_user_agent` to the default
    /// derived from the current `super_props` build number.
    pub fn clear_user_agent_override(&self) {
        if let Ok(mut lock) = self.ua_override.lock() {
            *lock = None;
        }
        // Rebuild props from the current build_number so the UA is consistent.
        if let Ok(mut props) = self.super_props.lock() {
            let rebuild = SuperProperties::linux_chrome_desktop_template(
                props.client_build_number,
                crate::build_info::STABLE_CHROMIUM_VERSION,
                crate::build_info::STABLE_ELECTRON_VERSION,
                &props.system_locale.clone(),
            );
            *props = rebuild;
        }
    }

    /// Return the current effective User-Agent string.
    pub fn ua(&self) -> String {
        // If there's an explicit override, return it directly.
        if let Ok(lock) = self.ua_override.lock() {
            if let Some(ref ua) = *lock {
                return ua.clone();
            }
        }
        // Otherwise derive from super_props.browser_user_agent.
        self.super_props
            .lock()
            .ok()
            .map(|p| p.browser_user_agent.clone())
            .unwrap_or_else(|| DEFAULT_CLIENT_VERSION.to_string())
    }

    /// Get a snapshot of the current `SuperProperties` (for gateway IDENTIFY).
    pub fn super_properties(&self) -> SuperProperties {
        self.super_props
            .lock()
            .ok()
            .map(|p| p.clone())
            .unwrap_or_else(|| SuperProperties::for_platform(
                &crate::build_info::BuildInfo::default(),
                "en-US",
            ))
    }

    /// Hot-swap the `SuperProperties` (e.g. after a background build refresh).
    pub fn update_super_properties(&self, new_props: SuperProperties) {
        if let Ok(mut lock) = self.super_props.lock() {
            *lock = new_props;
        }
    }

    /// Apply version headers (User-Agent + X-Super-Properties) to a request.
    ///
    /// Both values are derived from `super_props` — single source of truth
    /// (Phase B.4). The `#[cfg(feature = "native")]` gate on base64 is gone:
    /// `SuperProperties::to_header_value()` works on native and WASM (Phase B.3).
    fn apply_version_headers(
        &self,
        req: poly_host_bridge::http::RequestBuilder,
    ) -> poly_host_bridge::http::RequestBuilder {
        let (ua, x_super) = self
            .super_props
            .lock()
            .ok()
            .map(|props| (props.browser_user_agent.clone(), props.to_header_value()))
            .unwrap_or_else(|| (DEFAULT_CLIENT_VERSION.to_string(), String::new()));

        // Respect explicit UA override.
        let ua = if let Ok(lock) = self.ua_override.lock() {
            lock.clone().unwrap_or(ua)
        } else {
            ua
        };

        req.header("User-Agent", ua)
            .header("X-Super-Properties", x_super)
    }

    /// Return the current auth token, if any.
    pub fn token(&self) -> Option<String> {
        self.token.lock().ok().and_then(|lock| lock.clone())
    }

    fn token_header(&self) -> String {
        // User tokens are sent raw (no "Bot " prefix). The "Bot " prefix is
        // only correct for bot tokens; using it with a user token is itself a
        // ban-bait signal (Discord can detect the mismatch at auth time).
        self.token
            .lock()
            .ok()
            .and_then(|lock| lock.clone())
            .unwrap_or_default()
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let resp = self
            .apply_version_headers(self.http.get(self.api_url(path)))
            .header("Authorization", self.token_header())
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            // F-DC-1: Map 403 to PermissionDenied so the UI can render a
            // styled empty state instead of silently swallowing the error.
            if status == 403 {
                return Err(ClientError::PermissionDenied(
                    "You need the VIEW_CHANNEL permission to read this channel.".into(),
                ));
            }
            return Err(ClientError::Network(format!("HTTP {status}")));
        }
        resp.json::<T>().await.map_err(|e| ClientError::Internal(e.to_string()))
    }

    async fn post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ClientError> {
        let resp = self
            .apply_version_headers(self.http.post(self.api_url(path)))
            .header("Authorization", self.token_header())
            .json(body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(ClientError::Network(format!("HTTP {status}")));
        }
        resp.json::<T>().await.map_err(|e| ClientError::Internal(e.to_string()))
    }

    /// Spacebar/Fosscord-compatible password login.
    /// Real Discord doesn't expose this without captcha+MFA; we use it for
    /// self-hosted Spacebar instances and the local test server.
    pub async fn login(&self, login: &str, password: &str) -> Result<String, ClientError> {
        #[derive(serde::Deserialize)]
        struct LoginResp {
            token: String,
        }
        let resp = self
            .apply_version_headers(self.http.post(self.api_url("/api/v10/auth/login")))
            .json(&serde_json::json!({ "login": login, "password": password }))
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(ClientError::AuthFailed(format!(
                "login failed: HTTP {status}"
            )));
        }
        let parsed: LoginResp = resp
            .json()
            .await
            .map_err(|e| ClientError::Internal(e.to_string()))?;
        Ok(parsed.token)
    }

    pub async fn get_me(&self) -> Result<DiscordUser, ClientError> {
        self.get("/api/v10/users/@me").await
    }

    pub async fn get_guilds(&self) -> Result<Vec<DiscordGuild>, ClientError> {
        self.get("/api/v10/users/@me/guilds").await
    }

    pub async fn get_guild(&self, guild_id: &str) -> Result<DiscordGuild, ClientError> {
        self.get(&format!("/api/v10/guilds/{guild_id}")).await
    }

    /// Fetch a guild including `approximate_member_count`.
    ///
    /// Passes `?with_counts=true` — real Discord includes the field; test
    /// servers that omit it parse as `None` due to `#[serde(default)]`.
    pub async fn get_guild_with_counts(&self, guild_id: &str) -> Result<DiscordGuild, ClientError> {
        self.get(&format!("/api/v10/guilds/{guild_id}?with_counts=true")).await
    }

    pub async fn get_channel(&self, channel_id: &str) -> Result<DiscordChannel, ClientError> {
        self.get(&format!("/api/v10/channels/{channel_id}")).await
    }

    pub async fn get_guild_channels(&self, guild_id: &str) -> Result<Vec<DiscordChannel>, ClientError> {
        self.get(&format!("/api/v10/guilds/{guild_id}/channels")).await
    }

    pub async fn get_dm_channels(&self) -> Result<Vec<DiscordChannel>, ClientError> {
        self.get("/api/v10/users/@me/channels").await
    }

    pub async fn get_messages(
        &self,
        channel_id: &str,
        limit: Option<u32>,
        before: Option<&str>,
    ) -> Result<Vec<DiscordMessage>, ClientError> {
        let limit = limit.unwrap_or(50);
        let mut path = format!("/api/v10/channels/{channel_id}/messages?limit={limit}");
        if let Some(b) = before {
            path.push_str(&format!("&before={b}"));
        }
        self.get(&path).await
    }

    pub async fn send_message(&self, channel_id: &str, content: &str) -> Result<DiscordMessage, ClientError> {
        self.post_json(
            &format!("/api/v10/channels/{channel_id}/messages"),
            &serde_json::json!({ "content": content }),
        ).await
    }

    pub async fn get_user(&self, user_id: &str) -> Result<DiscordUser, ClientError> {
        self.get(&format!("/api/v10/users/{user_id}")).await
    }

    /// `GET /guilds/{guild_id}/threads/active` — all active (non-archived) threads
    /// in the guild. May return `has_more = true` if there are over 100 threads;
    /// for now we fetch one page (Discord doesn't paginate this endpoint, but
    /// `has_more` signals a cap was applied).
    pub async fn get_active_threads(
        &self,
        guild_id: &str,
    ) -> Result<DiscordActiveThreadsResponse, ClientError> {
        self.get(&format!("/api/v10/guilds/{guild_id}/threads/active")).await
    }

    /// `GET /channels/{channel_id}/threads/archived/public` — archived public threads
    /// for a parent channel (text or forum).
    pub async fn get_archived_threads_public(
        &self,
        channel_id: &str,
        limit: Option<u32>,
    ) -> Result<DiscordArchivedThreadsResponse, ClientError> {
        let limit = limit.unwrap_or(50).min(100);
        self.get(&format!(
            "/api/v10/channels/{channel_id}/threads/archived/public?limit={limit}"
        ))
        .await
    }

    /// `PATCH /api/v10/guilds/{guild_id}` — update guild fields (partial update).
    ///
    /// The `body` argument is a partial JSON object (only the fields to update).
    /// Returns the updated [`DiscordGuild`] object.
    ///
    /// For setting a banner, pass `banner` as a base64 data URI
    /// (`data:image/png;base64,…`). The Discord API only accepts data URIs, not
    /// remote URLs. The test server accepts a URL string for test convenience.
    pub async fn patch_guild(
        &self,
        guild_id: &str,
        body: serde_json::Value,
    ) -> Result<DiscordGuild, ClientError> {
        let path = format!("/api/v10/guilds/{guild_id}");
        let resp = self
            .apply_version_headers(self.http.patch(self.api_url(&path)))
            .header("Authorization", self.token_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            if status == 403 {
                return Err(ClientError::PermissionDenied(
                    "Guild banner requires the BANNER feature (Boost Tier 2 or higher).".into(),
                ));
            }
            return Err(ClientError::Network(format!("PATCH guild HTTP {status}")));
        }
        resp.json::<DiscordGuild>()
            .await
            .map_err(|e| ClientError::Internal(e.to_string()))
    }

    /// POST /api/v10/channels/{channel_id}/typing — trigger typing indicator.
    /// Discord returns 204 No Content on success.
    pub async fn trigger_typing(&self, channel_id: &str) -> Result<(), ClientError> {
        let path = format!("/api/v10/channels/{channel_id}/typing");
        let resp = self
            .apply_version_headers(self.http.post(self.api_url(&path)))
            .header("Authorization", self.token_header())
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        if status == 204 || resp.status().is_success() {
            Ok(())
        } else {
            Err(ClientError::Network(format!("trigger_typing returned HTTP {status}")))
        }
    }

    /// Fetch messages from a thread channel. Uses the same messages endpoint —
    /// Discord thread IDs are valid channel IDs.
    pub async fn get_thread_messages(
        &self,
        thread_id: &str,
        limit: Option<u32>,
        after: Option<&str>,
    ) -> Result<Vec<DiscordMessage>, ClientError> {
        let limit = limit.unwrap_or(1).min(100);
        let mut path = format!("/api/v10/channels/{thread_id}/messages?limit={limit}");
        if let Some(a) = after {
            path.push_str(&format!("&after={a}"));
        }
        self.get(&path).await
    }

    // ── Moderation endpoints (B-DS) ────────────────────────────────────────

    /// `GET /guilds/{id}/members/@me` — get the authenticated user's guild member
    /// object (includes role IDs and `communication_disabled_until`).
    pub async fn get_guild_member_me(
        &self,
        guild_id: &str,
    ) -> Result<DiscordGuildMember, ClientError> {
        self.get(&format!("/api/v10/guilds/{guild_id}/members/@me")).await
    }

    /// `GET /guilds/{id}/roles` — list all roles in the guild.
    pub async fn get_guild_roles(&self, guild_id: &str) -> Result<Vec<DiscordRole>, ClientError> {
        self.get(&format!("/api/v10/guilds/{guild_id}/roles")).await
    }

    /// `DELETE /guilds/{guild_id}/members/{user_id}` — kick a member.
    /// Discord returns 204 No Content on success.
    pub async fn kick_member(
        &self,
        guild_id: &str,
        user_id: &str,
        reason: Option<&str>,
    ) -> Result<(), ClientError> {
        let path = format!("/api/v10/guilds/{guild_id}/members/{user_id}");
        let mut req = self
            .apply_version_headers(self.http.delete(self.api_url(&path)))
            .header("Authorization", self.token_header());
        if let Some(r) = reason {
            req = req.header("X-Audit-Log-Reason", r);
        }
        let resp = req.send().await.map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        match status {
            204 | 200 => Ok(()),
            401 => Err(ClientError::AuthFailed("Unauthorized".into())),
            403 => Err(ClientError::PermissionDenied("Missing KICK_MEMBERS permission".into())),
            _ => Err(ClientError::Network(format!("kick_member HTTP {status}"))),
        }
    }

    /// `PUT /guilds/{guild_id}/bans/{user_id}` — permanently ban a member.
    /// `delete_message_seconds`: 0-604800 (0 = don't delete history).
    /// Discord returns 204 on success.
    pub async fn ban_member(
        &self,
        guild_id: &str,
        user_id: &str,
        reason: Option<&str>,
        delete_message_seconds: Option<u64>,
    ) -> Result<(), ClientError> {
        let path = format!("/api/v10/guilds/{guild_id}/bans/{user_id}");
        let mut body = serde_json::json!({});
        if let Some(secs) = delete_message_seconds
            && let Some(obj) = body.as_object_mut()
        {
            obj.insert(
                "delete_message_seconds".to_string(),
                serde_json::json!(secs.min(604_800)),
            );
        }
        let mut req = self
            .apply_version_headers(self.http.put(self.api_url(&path)))
            .header("Authorization", self.token_header())
            .json(&body);
        if let Some(r) = reason {
            req = req.header("X-Audit-Log-Reason", r);
        }
        let resp = req.send().await.map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        match status {
            204 | 200 => Ok(()),
            401 => Err(ClientError::AuthFailed("Unauthorized".into())),
            403 => Err(ClientError::PermissionDenied("Missing BAN_MEMBERS permission".into())),
            _ => Err(ClientError::Network(format!("ban_member HTTP {status}"))),
        }
    }

    /// `DELETE /guilds/{guild_id}/bans/{user_id}` — unban a member.
    /// Discord returns 204 on success.
    pub async fn unban_member(
        &self,
        guild_id: &str,
        user_id: &str,
    ) -> Result<(), ClientError> {
        let path = format!("/api/v10/guilds/{guild_id}/bans/{user_id}");
        let resp = self
            .apply_version_headers(self.http.delete(self.api_url(&path)))
            .header("Authorization", self.token_header())
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        match status {
            204 | 200 => Ok(()),
            401 => Err(ClientError::AuthFailed("Unauthorized".into())),
            403 => Err(ClientError::PermissionDenied("Missing BAN_MEMBERS permission".into())),
            _ => Err(ClientError::Network(format!("unban_member HTTP {status}"))),
        }
    }

    /// `GET /guilds/{guild_id}/bans` — list all bans (paginated; fetches first page).
    pub async fn get_bans(&self, guild_id: &str) -> Result<Vec<DiscordBan>, ClientError> {
        self.get(&format!("/api/v10/guilds/{guild_id}/bans?limit=1000")).await
    }

    /// `PATCH /guilds/{guild_id}/members/{user_id}` — set `communication_disabled_until`.
    /// Pass `None` to clear an active timeout.
    pub async fn set_member_timeout(
        &self,
        guild_id: &str,
        user_id: &str,
        until_iso8601: Option<&str>,
    ) -> Result<(), ClientError> {
        let path = format!("/api/v10/guilds/{guild_id}/members/{user_id}");
        let body = serde_json::json!({ "communication_disabled_until": until_iso8601 });
        let resp = self
            .apply_version_headers(self.http.patch(self.api_url(&path)))
            .header("Authorization", self.token_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        match status {
            200 | 204 => Ok(()),
            401 => Err(ClientError::AuthFailed("Unauthorized".into())),
            403 => Err(ClientError::PermissionDenied(
                "Missing MODERATE_MEMBERS permission".into(),
            )),
            _ => Err(ClientError::Network(format!("set_member_timeout HTTP {status}"))),
        }
    }

    /// `DELETE /channels/{channel_id}/messages/{message_id}` — delete a single message.
    /// Discord returns 204 on success.
    pub async fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> Result<(), ClientError> {
        let path = format!("/api/v10/channels/{channel_id}/messages/{message_id}");
        let resp = self
            .apply_version_headers(self.http.delete(self.api_url(&path)))
            .header("Authorization", self.token_header())
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        match status {
            204 | 200 => Ok(()),
            401 => Err(ClientError::AuthFailed("Unauthorized".into())),
            403 => Err(ClientError::PermissionDenied(
                "Missing MANAGE_MESSAGES permission".into(),
            )),
            404 => Err(ClientError::NotFound("message not found".into())),
            _ => Err(ClientError::Network(format!("delete_message HTTP {status}"))),
        }
    }

    /// `PATCH /channels/{channel_id}` — update channel metadata.
    /// Returns the updated channel object.
    pub async fn patch_channel(
        &self,
        channel_id: &str,
        body: serde_json::Value,
    ) -> Result<DiscordChannel, ClientError> {
        let path = format!("/api/v10/channels/{channel_id}");
        let resp = self
            .apply_version_headers(self.http.patch(self.api_url(&path)))
            .header("Authorization", self.token_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            if status == 403 {
                return Err(ClientError::PermissionDenied(
                    "Missing MANAGE_CHANNELS permission".into(),
                ));
            }
            return Err(ClientError::Network(format!("patch_channel HTTP {status}")));
        }
        resp.json::<DiscordChannel>()
            .await
            .map_err(|e| ClientError::Internal(e.to_string()))
    }

    /// `PATCH /guilds/{guild_id}/channels` — reorder channels.
    /// `ordering` is `[{id, position}]`. Discord returns 204.
    pub async fn reorder_channels(
        &self,
        guild_id: &str,
        ordering: &[serde_json::Value],
    ) -> Result<(), ClientError> {
        let path = format!("/api/v10/guilds/{guild_id}/channels");
        let resp = self
            .apply_version_headers(self.http.patch(self.api_url(&path)))
            .header("Authorization", self.token_header())
            .json(ordering)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        match status {
            204 | 200 => Ok(()),
            401 => Err(ClientError::AuthFailed("Unauthorized".into())),
            403 => Err(ClientError::PermissionDenied(
                "Missing MANAGE_CHANNELS permission".into(),
            )),
            _ => Err(ClientError::Network(format!("reorder_channels HTTP {status}"))),
        }
    }

    /// `GET /guilds/{guild_id}/audit-logs` — fetch recent audit log entries.
    ///
    /// Filters to moderation-relevant action types:
    /// - 20 = MEMBER_KICK
    /// - 22 = MEMBER_BAN_ADD
    /// - 23 = MEMBER_BAN_REMOVE
    /// - 12 = CHANNEL_UPDATE
    /// - 72 = MESSAGE_DELETE
    pub async fn get_audit_log(
        &self,
        guild_id: &str,
        limit: usize,
    ) -> Result<DiscordAuditLogResponse, ClientError> {
        let limit = limit.min(100);
        // Fetch without action_type filter — the caller maps relevant entries.
        let path = format!("/api/v10/guilds/{guild_id}/audit-logs?limit={limit}");
        self.get(&path).await
    }

    // ── Social / Relationship operations ─────────────────────────────────────

    /// `PUT /users/@me/relationships/{user_id}` with `{"type": relationship_type}`.
    ///
    /// `relationship_type` values: 1 = friend request, 2 = block.
    pub async fn put_relationship(
        &self,
        user_id: &str,
        relationship_type: u8,
    ) -> Result<(), ClientError> {
        let path = format!("/api/v10/users/@me/relationships/{user_id}");
        let body = serde_json::json!({ "type": relationship_type });
        let resp = self
            .apply_version_headers(self.http.put(self.api_url(&path)))
            .header("Authorization", self.token_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        match status {
            200 | 201 | 204 => Ok(()),
            401 => Err(ClientError::AuthFailed("Unauthorized".into())),
            403 => Err(ClientError::PermissionDenied("Forbidden".into())),
            _ => Err(ClientError::Network(format!("put_relationship HTTP {status}"))),
        }
    }

    /// `DELETE /users/@me/relationships/{user_id}` — remove friend or unblock.
    pub async fn delete_relationship(&self, user_id: &str) -> Result<(), ClientError> {
        let path = format!("/api/v10/users/@me/relationships/{user_id}");
        let resp = self
            .apply_version_headers(self.http.delete(self.api_url(&path)))
            .header("Authorization", self.token_header())
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        match status {
            200 | 204 => Ok(()),
            401 => Err(ClientError::AuthFailed("Unauthorized".into())),
            403 => Err(ClientError::PermissionDenied("Forbidden".into())),
            404 => Err(ClientError::NotFound("relationship not found".into())),
            _ => Err(ClientError::Network(format!("delete_relationship HTTP {status}"))),
        }
    }

    /// `PUT /users/@me/notes/{user_id}` — set or clear a private user note.
    pub async fn put_user_note(&self, user_id: &str, note: &str) -> Result<(), ClientError> {
        let path = format!("/api/v10/users/@me/notes/{user_id}");
        let body = serde_json::json!({ "note": note });
        let resp = self
            .apply_version_headers(self.http.put(self.api_url(&path)))
            .header("Authorization", self.token_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        match status {
            200 | 204 => Ok(()),
            401 => Err(ClientError::AuthFailed("Unauthorized".into())),
            404 => Err(ClientError::NotFound("user not found".into())),
            _ => Err(ClientError::Network(format!("put_user_note HTTP {status}"))),
        }
    }

    // ── DM / channel lifecycle ────────────────────────────────────────────────

    /// `DELETE /channels/{channel_id}` — close DM or leave group DM.
    pub async fn delete_channel(&self, channel_id: &str) -> Result<(), ClientError> {
        let path = format!("/api/v10/channels/{channel_id}");
        let resp = self
            .apply_version_headers(self.http.delete(self.api_url(&path)))
            .header("Authorization", self.token_header())
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        match status {
            200 | 204 => Ok(()),
            401 => Err(ClientError::AuthFailed("Unauthorized".into())),
            403 => Err(ClientError::PermissionDenied("Forbidden".into())),
            404 => Err(ClientError::NotFound("channel not found".into())),
            _ => Err(ClientError::Network(format!("delete_channel HTTP {status}"))),
        }
    }

    /// `PUT /channels/{channel_id}/recipients/{user_id}` — add a user to a group DM.
    pub async fn add_group_dm_recipient(
        &self,
        channel_id: &str,
        user_id: &str,
    ) -> Result<(), ClientError> {
        let path = format!("/api/v10/channels/{channel_id}/recipients/{user_id}");
        let resp = self
            .apply_version_headers(self.http.put(self.api_url(&path)))
            .header("Authorization", self.token_header())
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        match status {
            200 | 204 => Ok(()),
            401 => Err(ClientError::AuthFailed("Unauthorized".into())),
            403 => Err(ClientError::PermissionDenied("Forbidden".into())),
            _ => Err(ClientError::Network(format!("add_group_dm_recipient HTTP {status}"))),
        }
    }

    /// `POST /channels/{channel_id}/invites` — create a new invite.
    ///
    /// Returns the invite code string.
    pub async fn create_invite(
        &self,
        channel_id: &str,
        max_age_secs: u64,
        max_uses: u32,
    ) -> Result<String, ClientError> {
        let path = format!("/api/v10/channels/{channel_id}/invites");
        let body = serde_json::json!({
            "max_age": max_age_secs,
            "max_uses": max_uses,
            "unique": true,
        });
        let resp = self
            .apply_version_headers(self.http.post(self.api_url(&path)))
            .header("Authorization", self.token_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!("create_invite HTTP {status}")));
        }
        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ClientError::Internal(e.to_string()))?;
        value
            .get("code")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| ClientError::Internal("create_invite: missing 'code' field".into()))
    }

    /// `POST /users/@me/channels` — open a DM with a user.
    ///
    /// Returns the channel ID.
    pub async fn open_dm(&self, user_id: &str) -> Result<String, ClientError> {
        let path = "/api/v10/users/@me/channels";
        let body = serde_json::json!({ "recipient_id": user_id });
        let resp = self
            .apply_version_headers(self.http.post(self.api_url(path)))
            .header("Authorization", self.token_header())
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!("open_dm HTTP {status}")));
        }
        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ClientError::Internal(e.to_string()))?;
        value
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| ClientError::Internal("open_dm: missing 'id' field".into()))
    }

    /// POST to `path` (full path including `/api/v10/…`) with an empty body.
    ///
    /// Used for one-shot REST calls that carry no request body, e.g.
    /// `POST /api/v10/channels/{id}/call/ring/stop` (D.4).
    pub async fn post_empty(&self, path: &str) -> Result<(), ClientError> {
        let resp = self
            .apply_version_headers(self.http.post(self.api_url(path)))
            .header("Authorization", self.token_header())
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        if status == 204 || resp.status().is_success() {
            Ok(())
        } else {
            Err(ClientError::Network(format!("post_empty {path} returned HTTP {status}")))
        }
    }
}
