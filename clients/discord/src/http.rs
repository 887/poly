//! Discord REST API v10 HTTP client.

use std::sync::{Arc, Mutex};

use poly_client::ClientError;
use poly_host_bridge::http::HttpClient;

use crate::api::{
    DiscordActiveThreadsResponse, DiscordArchivedThreadsResponse, DiscordAuditLogResponse,
    DiscordBan, DiscordChannel, DiscordGuild, DiscordGuildMember, DiscordMessage,
    DiscordRelationship, DiscordRole,
    DiscordUser,
};
use crate::super_properties::SuperProperties;
use crate::guardrails::{GuardrailCounters, RateGuard};

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
    /// Phase F.1 — shared telemetry counters.  Cloned into `DiscordClient`
    /// so the same `Arc<Mutex<GuardrailStats>>` is exposed via
    /// `DiscordClient::guardrail_stats()`.
    pub(crate) counters: GuardrailCounters,
    /// Phase D.2 — outbound token-bucket rate guard.
    ///
    /// Lives here, not on `DiscordClient`, because this is the only type that
    /// can actually gate an outbound request; owning it one level up left it
    /// permanently unconsulted (and `DiscordHealth.recent_429_count` stuck at 0
    /// because nothing ever called `record_429`).
    pub(crate) rate_guard: RateGuard,
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
            counters: GuardrailCounters::new(),
            rate_guard: RateGuard::new(),
        }
    }

    /// Adopt a freshly scraped / cached `BuildInfo` (Phase A.5).
    ///
    /// Rebuilds `super_props` from `build_info` — so `X-Super-Properties` and
    /// the derived `User-Agent` both carry the real `client_build_number` —
    /// and re-applies any explicit UA override on top.
    ///
    /// Called by `DiscordClient::refresh_build_info` after
    /// `build_info::load_or_refresh`.  Before this existed the scraper had no
    /// call site at all and every request shipped the hard-coded floor build
    /// number forever, which is exactly the stale-client signal Discord flags.
    pub fn apply_build_info(&self, build_info: &crate::build_info::BuildInfo) {
        let locale = self
            .super_props
            .lock()
            .ok()
            .map_or_else(|| "en-US".to_string(), |p| p.system_locale.clone());
        let mut props = SuperProperties::for_platform(build_info, &locale);
        if let Ok(lock) = self.ua_override.lock()
            && let Some(ref ua) = *lock
        {
            props.apply_ua_override(ua);
        }
        if let Ok(mut lock) = self.super_props.lock() {
            *lock = props;
        }
    }

    /// The `client_build_number` currently being advertised.
    pub fn build_number_in_use(&self) -> u32 {
        self.super_props
            .lock()
            .ok()
            .map_or(crate::build_info::LATEST_KNOWN_STABLE_BUILD, |p| {
                p.client_build_number
            })
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
    pub fn set_user_agent(&self, ua: &str) {
        if let Ok(mut lock) = self.ua_override.lock() {
            *lock = Some(ua.to_owned());
        }
        if let Ok(mut props) = self.super_props.lock() {
            props.apply_ua_override(ua);
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
        if let Ok(lock) = self.ua_override.lock()
            && let Some(ref ua) = *lock
        {
            return ua.clone();
        }
        // Otherwise derive from super_props.browser_user_agent.
        self.super_props
            .lock()
            .ok()
            .map_or_else(|| DEFAULT_CLIENT_VERSION.to_string(), |p| p.browser_user_agent.clone())
    }

    /// Get a snapshot of the current `SuperProperties` (for gateway IDENTIFY).
    ///
    /// Only used on the gateway path; gated to avoid a dead-code warning when
    /// the gateway feature is off (default `native`-only build).
    #[cfg(feature = "gateway")]
    pub fn super_properties(&self) -> SuperProperties {
        self.super_props
            .lock()
            .ok()
            .map_or_else(
                || SuperProperties::for_platform(&crate::build_info::BuildInfo::default(), "en-US"),
                |p| (*p).clone(),
            )
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
            .map_or_else(|| (DEFAULT_CLIENT_VERSION.to_string(), String::new()), |props| (props.browser_user_agent.clone(), props.to_header_value()));

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
    ///
    /// Used by the gateway IDENTIFY paths (native gateway + wasm32 gateway-bridge)
    /// to forward the same bearer token to the WS handshake; gated to keep the
    /// default `native`-only build clean.
    #[cfg(any(feature = "voice", all(feature = "gateway-bridge", target_arch = "wasm32")))]
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

    /// Default `PermissionDenied` message for read routes.
    const FORBIDDEN_READ: &'static str =
        "You need the VIEW_CHANNEL permission to read this channel.";

    /// Returns `true` when this client is talking to real Discord infrastructure.
    ///
    /// The anti-ban budget (10 000 invalid requests / 10 min → IP ban) only
    /// exists on Discord's own hosts.  Spacebar / self-hosted / test servers
    /// have their own limits and must not be throttled by the Discord token
    /// bucket — otherwise the local test server would trip the guard.
    fn targets_real_discord(&self) -> bool {
        self.base_url.contains("discord.com") || self.base_url.contains("discordapp.com")
    }

    /// Coarse rate-limit bucket key for `path` (query string stripped).
    fn bucket_key(path: &str) -> &str {
        path.split('?').next().unwrap_or(path)
    }

    /// D.2 — pre-flight token-bucket gate applied to every outbound request.
    ///
    /// Previously `RateGuard::check` had no production call site at all, so the
    /// 10-burst / 5-per-second budget was never enforced and
    /// `GuardrailStats.rate_guard_trips` was permanently 0.
    fn guard_outbound(&self, _path: &str) -> Result<(), ClientError> {
        if !self.targets_real_discord() {
            return Ok(());
        }
        if let Err(e) = self.rate_guard.check() {
            self.counters.inc_rate_guard_trip();
            return Err(e);
        }
        Ok(())
    }

    /// Begin an outbound request: rate gate → version headers → auth header.
    ///
    /// Single choke point so no helper can accidentally skip the guard or the
    /// anti-ban headers (`login` is the one deliberate exception — it must not
    /// send an `Authorization` header).
    fn begin(
        &self,
        path: &str,
        req: poly_host_bridge::http::RequestBuilder,
    ) -> Result<poly_host_bridge::http::RequestBuilder, ClientError> {
        self.guard_outbound(path)?;
        Ok(self
            .apply_version_headers(req)
            .header("Authorization", self.token_header()))
    }

    /// D.7 + F.1 + D.2 — the single place where a response status becomes a result.
    ///
    /// Every helper routes through here, so 401/403/404/429/5xx telemetry, the
    /// per-bucket exponential 429 back-off, and the route-specific
    /// `PermissionDenied` message all live in one place.  Fourteen helpers used
    /// to match statuses inline without ever touching `counters`, which meant a
    /// throttled kick/ban was invisible to the health panel and its
    /// `Retry-After` header was discarded.
    ///
    /// `forbidden_msg` is the human-readable message used for 403 on this route.
    fn classify(
        &self,
        resp: &poly_host_bridge::http::Response,
        path: &str,
        forbidden_msg: &str,
    ) -> Result<(), ClientError> {
        let bucket = Self::bucket_key(path);
        if resp.status().is_success() {
            self.counters.inc_2xx();
            self.rate_guard.record_success(bucket);
            return Ok(());
        }
        let status = resp.status().as_u16();
        let retry_after = resp
            .headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        Err(match status {
            429 => {
                // Feed the exponential per-bucket back-off and report the
                // effective (multiplied) delay, not the raw header value.
                let backoff = self.rate_guard.record_429(bucket, retry_after.unwrap_or(1));
                let secs = backoff.as_secs();
                self.counters.inc_429(path, secs);
                ClientError::Network(format!("HTTP 429 retry-after={secs}s"))
            }
            401 => {
                self.counters.inc_401();
                ClientError::AuthFailed("Unauthorized".into())
            }
            403 => {
                self.counters.inc_403(path);
                ClientError::PermissionDenied(forbidden_msg.to_string())
            }
            404 => {
                self.counters.inc_404();
                ClientError::NotFound(format!("{path} not found"))
            }
            s if s >= 500 => {
                self.counters.inc_5xx(s);
                ClientError::Network(format!("HTTP {s}"))
            }
            s => ClientError::Network(format!("HTTP {s}")),
        })
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let resp = self
            .begin(path, self.http.get(self.api_url(path)))?
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, path, Self::FORBIDDEN_READ)?;
        resp.json::<T>().await.map_err(|e| ClientError::Internal(e.to_string()))
    }

    // reason: the returned future holds the non-Send reqwest builder across
    // awaits; this backend's futures run on a single-threaded local executor,
    // so Send is not required.
    #[allow(clippy::future_not_send)]
    async fn post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ClientError> {
        let resp = self
            .begin(path, self.http.post(self.api_url(path)))?
            .json(body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, path, Self::FORBIDDEN_READ)?;
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
            path.push_str("&before=");
            path.push_str(b);
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
            .begin(&path, self.http.patch(self.api_url(&path)))?
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(
            &resp,
            &path,
            "Guild banner requires the BANNER feature (Boost Tier 2 or higher).",
        )?;
        resp.json::<DiscordGuild>()
            .await
            .map_err(|e| ClientError::Internal(e.to_string()))
    }

    /// POST /api/v10/channels/{channel_id}/typing — trigger typing indicator.
    /// Discord returns 204 No Content on success.
    pub async fn trigger_typing(&self, channel_id: &str) -> Result<(), ClientError> {
        let path = format!("/api/v10/channels/{channel_id}/typing");
        let resp = self
            .begin(&path, self.http.post(self.api_url(&path)))?
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, &path, "Missing SEND_MESSAGES permission")
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
            path.push_str("&after=");
            path.push_str(a);
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
        let mut req = self.begin(&path, self.http.delete(self.api_url(&path)))?;
        if let Some(r) = reason {
            req = req.header("X-Audit-Log-Reason", r);
        }
        let resp = req.send().await.map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, &path, "Missing KICK_MEMBERS permission")
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
            .begin(&path, self.http.put(self.api_url(&path)))?
            .json(&body);
        if let Some(r) = reason {
            req = req.header("X-Audit-Log-Reason", r);
        }
        let resp = req.send().await.map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, &path, "Missing BAN_MEMBERS permission")
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
            .begin(&path, self.http.delete(self.api_url(&path)))?
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, &path, "Missing BAN_MEMBERS permission")
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
            .begin(&path, self.http.patch(self.api_url(&path)))?
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, &path, "Missing MODERATE_MEMBERS permission")
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
            .begin(&path, self.http.delete(self.api_url(&path)))?
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, &path, "Missing MANAGE_MESSAGES permission")
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
            .begin(&path, self.http.patch(self.api_url(&path)))?
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, &path, "Missing MANAGE_CHANNELS permission")?;
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
            .begin(&path, self.http.patch(self.api_url(&path)))?
            .json(ordering)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, &path, "Missing MANAGE_CHANNELS permission")
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

    /// `GET /users/@me/relationships` — list all relationships (friends, blocks,
    /// incoming/outgoing requests). Caller filters by `type`:
    ///   1 = accepted friend, 2 = blocked, 3 = incoming request, 4 = outgoing request.
    pub async fn get_relationships(&self) -> Result<Vec<DiscordRelationship>, ClientError> {
        self.get("/api/v10/users/@me/relationships").await
    }

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
            .begin(&path, self.http.put(self.api_url(&path)))?
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, &path, "Forbidden")
    }

    /// `DELETE /users/@me/relationships/{user_id}` — remove friend or unblock.
    pub async fn delete_relationship(&self, user_id: &str) -> Result<(), ClientError> {
        let path = format!("/api/v10/users/@me/relationships/{user_id}");
        let resp = self
            .begin(&path, self.http.delete(self.api_url(&path)))?
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, &path, "Forbidden")
    }

    /// `PUT /users/@me/notes/{user_id}` — set or clear a private user note.
    pub async fn put_user_note(&self, user_id: &str, note: &str) -> Result<(), ClientError> {
        let path = format!("/api/v10/users/@me/notes/{user_id}");
        let body = serde_json::json!({ "note": note });
        let resp = self
            .begin(&path, self.http.put(self.api_url(&path)))?
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, &path, "Forbidden")
    }

    // ── DM / channel lifecycle ────────────────────────────────────────────────

    /// `DELETE /channels/{channel_id}` — close DM or leave group DM.
    pub async fn delete_channel(&self, channel_id: &str) -> Result<(), ClientError> {
        let path = format!("/api/v10/channels/{channel_id}");
        let resp = self
            .begin(&path, self.http.delete(self.api_url(&path)))?
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, &path, "Forbidden")
    }

    /// `PUT /channels/{channel_id}/recipients/{user_id}` — add a user to a group DM.
    pub async fn add_group_dm_recipient(
        &self,
        channel_id: &str,
        user_id: &str,
    ) -> Result<(), ClientError> {
        let path = format!("/api/v10/channels/{channel_id}/recipients/{user_id}");
        let resp = self
            .begin(&path, self.http.put(self.api_url(&path)))?
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, &path, "Forbidden")
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
            .begin(&path, self.http.post(self.api_url(&path)))?
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, &path, "Missing CREATE_INSTANT_INVITE permission")?;
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
            .begin(path, self.http.post(self.api_url(path)))?
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, path, "Forbidden")?;
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
    /// `POST /api/v10/channels/{id}/call/ring/stop` (D.4) on the gateway path.
    /// Gated to keep the default `native`-only build clean.
    #[cfg(feature = "gateway")]
    pub async fn post_empty(&self, path: &str) -> Result<(), ClientError> {
        let resp = self
            .begin(path, self.http.post(self.api_url(path)))?
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        self.classify(&resp, path, "Forbidden")
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn bucket_key_strips_query_string() {
        assert_eq!(
            DiscordHttpClient::bucket_key("/api/v10/channels/1/messages?limit=50&before=9"),
            "/api/v10/channels/1/messages"
        );
        assert_eq!(
            DiscordHttpClient::bucket_key("/api/v10/guilds/7/bans/9"),
            "/api/v10/guilds/7/bans/9"
        );
    }

    #[test]
    fn rate_guard_gates_real_discord_only() {
        // The 10k-invalid-requests/10 min IP-ban budget only exists on
        // Discord's own hosts; Spacebar / self-hosted / test servers must not
        // be throttled by the Discord token bucket.
        let local = DiscordHttpClient::new("http://127.0.0.1:9102".to_string());
        assert!(!local.targets_real_discord());
        for _ in 0..50_u32 {
            assert!(
                local.guard_outbound("/api/v10/users/@me").is_ok(),
                "test server must not be rate-gated"
            );
        }

        let real = DiscordHttpClient::new("https://discord.com".to_string());
        assert!(real.targets_real_discord());
        // Burst is 10; the 11th call within the same instant must trip.
        let mut tripped = false;
        for _ in 0..20_u32 {
            if real.guard_outbound("/api/v10/users/@me").is_err() {
                tripped = true;
                break;
            }
        }
        assert!(tripped, "outbound token bucket must actually gate real Discord");
        assert!(
            real.counters.snapshot().rate_guard_trips > 0,
            "a trip must be recorded in GuardrailStats"
        );
    }

    #[test]
    fn apply_build_info_updates_advertised_build_number() {
        let http = DiscordHttpClient::new("https://discord.com".to_string());
        assert_eq!(
            http.build_number_in_use(),
            crate::build_info::LATEST_KNOWN_STABLE_BUILD
        );
        let fresh = crate::build_info::BuildInfo {
            build_number: 999_999,
            version_hash: "deadbee".to_string(),
            scraped_at: 1_800_000_000,
        };
        http.apply_build_info(&fresh);
        assert_eq!(http.build_number_in_use(), 999_999);
        assert!(
            http.ua().contains("999999"),
            "the derived UA must track the build number, got {}",
            http.ua()
        );
    }

    #[test]
    fn apply_build_info_preserves_explicit_ua_override() {
        let http = DiscordHttpClient::new("https://discord.com".to_string());
        http.set_user_agent("custom-agent/1.0");
        let fresh = crate::build_info::BuildInfo {
            build_number: 888_888,
            version_hash: "cafe".to_string(),
            scraped_at: 1_800_000_000,
        };
        http.apply_build_info(&fresh);
        assert_eq!(http.ua(), "custom-agent/1.0");
        assert_eq!(http.build_number_in_use(), 888_888);
    }
}
