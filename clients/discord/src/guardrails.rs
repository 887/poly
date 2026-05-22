//! Anti-ban guardrails for the Discord client (Phase D + F).
//!
//! Provides three guards:
//!
//! - [`RateGuard`] — token-bucket rate limiter capping outbound HTTP at 10
//!   req/s burst, 5 req/s sustained, plus 429 back-off tracking.
//! - [`SlowModeGuard`] — per-channel slow-mode enforcement: refuses to send a
//!   message if the previous send was less than `rate_limit_per_user` seconds ago.
//! - [`PermissionGuard`] — pre-flight permission check against a cached
//!   `permissions` bitfield before issuing moderator-only requests.
//! - [`VoiceManager`] — sketch that enforces single active voice session per account.
//! - [`DiscordHealth`] — soft-warning signal surface (D.8) for the UI to
//!   render a "backend health" panel.
//! - [`GuardrailStats`] — snapshot of all telemetry counters (Phase F.1), exposed
//!   via [`DiscordClient::guardrail_stats`] for the "Backend health" panel.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
// web_time::Instant is a drop-in replacement that uses Performance.now() on wasm32.
// std::time::Instant::now() panics "time not implemented" on wasm32-unknown-unknown.
use web_time::Instant;

use poly_client::ClientError;

// ── Permission bitfield constants (Discord API v10) ────────────────────────
pub const PERM_KICK_MEMBERS: i64 = 1 << 1;
pub const PERM_BAN_MEMBERS: i64 = 1 << 2;
pub const PERM_MANAGE_CHANNELS: i64 = 1 << 4;
pub const PERM_MANAGE_GUILD: i64 = 1 << 5;
pub const PERM_MANAGE_MESSAGES: i64 = 1 << 13;
pub const PERM_MODERATE_MEMBERS: i64 = 1 << 40;
pub const PERM_ADMINISTRATOR: i64 = 1 << 3;

// ── D.1 / D.2 — Token-bucket rate guard ───────────────────────────────────

/// Simple token-bucket rate guard.
///
/// Quota: 10 req/s burst, 5 req/s sustained (well under Discord's 50 req/s
/// global limit and the 10 000-invalid-requests/10 min IP ban threshold).
///
/// Thread-safe via `Arc<Mutex<RateGuardInner>>`.
#[derive(Clone)]
pub struct RateGuard {
    inner: Arc<Mutex<RateGuardInner>>,
}

struct RateGuardInner {
    /// Current token count (fractional buckets in microseconds).
    tokens: f64,
    /// Last refill timestamp.
    last_refill: Instant,
    /// Tokens added per second (sustained rate).
    rate_per_sec: f64,
    /// Maximum burst size (tokens).
    burst: f64,
    /// Per-bucket 429 tracking: bucket_key → (last_429_at, backoff_multiplier).
    backoff: HashMap<String, (Instant, u32)>,
    /// Number of 429s seen in the last `HEALTH_WINDOW`.
    recent_429_count: u32,
    /// When the most recent 429 was seen (for health surface D.8).
    last_429_at: Option<Instant>,
}

impl RateGuardInner {
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.rate_per_sec).min(self.burst);
        self.last_refill = now;
    }
}

impl RateGuard {
    /// Create a new guard with 10 req/s burst, 5 req/s sustained.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RateGuardInner {
                tokens: 10.0,
                last_refill: Instant::now(),
                rate_per_sec: 5.0,
                burst: 10.0,
                backoff: HashMap::new(),
                recent_429_count: 0,
                last_429_at: None,
            })),
        }
    }

    /// Consume one token from the bucket.
    ///
    /// Returns `Ok(())` if a token was available, or `Err(ClientError::Network)`
    /// with a message indicating how long to wait.
    ///
    /// In production this should be called before every outbound HTTP helper.
    /// The caller may `tokio::time::sleep` for the returned duration if desired.
    pub fn check(&self) -> Result<(), ClientError> {
        let mut inner = self.inner.lock().expect("RateGuard lock poisoned");
        inner.refill();
        if inner.tokens >= 1.0 {
            inner.tokens -= 1.0;
            Ok(())
        } else {
            let wait_ms = ((1.0 - inner.tokens) / inner.rate_per_sec * 1000.0) as u64;
            Err(ClientError::Network(format!(
                "rate limit: too many requests — retry after {wait_ms}ms"
            )))
        }
    }

    /// Record a 429 response for the given bucket key and return the
    /// `Retry-After` duration the caller should sleep for.
    ///
    /// Implements exponential back-off: second 429 within 60s on the same
    /// bucket doubles `Retry-After`.
    pub fn record_429(&self, bucket: &str, retry_after_secs: u64) -> Duration {
        let mut inner = self.inner.lock().expect("RateGuard lock poisoned");
        inner.recent_429_count = inner.recent_429_count.saturating_add(1);
        let now = Instant::now();
        inner.last_429_at = Some(now);

        let multiplier = inner
            .backoff
            .get(bucket)
            .map(|(last, mult)| {
                if now.duration_since(*last) < Duration::from_secs(60) {
                    (*mult + 1).min(4) // cap at 4×
                } else {
                    1
                }
            })
            .unwrap_or(1);

        inner
            .backoff
            .insert(bucket.to_string(), (now, multiplier));

        Duration::from_secs(retry_after_secs * u64::from(multiplier))
    }

    /// Reset the 429 counter for a bucket on a successful response.
    pub fn record_success(&self, bucket: &str) {
        let mut inner = self.inner.lock().expect("RateGuard lock poisoned");
        inner.backoff.remove(bucket);
    }

    /// Return a snapshot of the recent 429 count and last 429 timestamp (D.8).
    pub fn health_snapshot(&self) -> (u32, Option<Instant>) {
        let inner = self.inner.lock().expect("RateGuard lock poisoned");
        (inner.recent_429_count, inner.last_429_at)
    }
}

impl Default for RateGuard {
    fn default() -> Self {
        Self::new()
    }
}

// ── D.1 / D.5 — Slow-mode guard ───────────────────────────────────────────

/// Per-channel slow-mode enforcement.
///
/// On each `send_message` the caller registers the send via [`SlowModeGuard::check`],
/// passing the channel's `rate_limit_per_user` (0 = no slow mode).  The guard
/// records `last_send_at` per channel_id and refuses to let a message through
/// if `now < last_send_at + rate_limit_per_user`.
#[derive(Clone, Default)]
pub struct SlowModeGuard {
    inner: Arc<Mutex<SlowModeInner>>,
}

#[derive(Default)]
struct SlowModeInner {
    /// channel_id → last_send_at
    last_send: HashMap<String, Instant>,
}

impl SlowModeGuard {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check whether a message can be sent on `channel_id`.
    ///
    /// `rate_limit_per_user` is the channel's slow-mode interval in seconds
    /// (0 = no limit).  Returns `Ok(())` when the send is allowed and records
    /// the current time as `last_send_at`.  Returns `Err(ClientError::Network)`
    /// with a human-readable "slow mode: retry in Xs" message when blocked.
    pub fn check(&self, channel_id: &str, rate_limit_per_user: u32) -> Result<(), ClientError> {
        if rate_limit_per_user == 0 {
            return Ok(());
        }
        let mut inner = self.inner.lock().expect("SlowModeGuard lock poisoned");
        let now = Instant::now();
        let window = Duration::from_secs(u64::from(rate_limit_per_user));
        if let Some(last) = inner.last_send.get(channel_id) {
            let elapsed = now.duration_since(*last);
            if elapsed < window {
                let remaining = window - elapsed;
                return Err(ClientError::Network(format!(
                    "slow mode: channel is in slow mode — retry in {}s",
                    remaining.as_secs() + 1
                )));
            }
        }
        inner.last_send.insert(channel_id.to_string(), now);
        Ok(())
    }

    /// Mark a message as sent on `channel_id` (call after successful HTTP send).
    pub fn record_send(&self, channel_id: &str) {
        let mut inner = self.inner.lock().expect("SlowModeGuard lock poisoned");
        inner.last_send.insert(channel_id.to_string(), Instant::now());
    }
}

// ── D.1 / D.4 — Permission guard ──────────────────────────────────────────

/// Cached permission bitfield for the authenticated user on a specific guild.
///
/// Refreshed by calling `update_permissions` whenever a guild-switch or member
/// event arrives.  The guard is keyed per guild-id.
#[derive(Clone, Default)]
pub struct PermissionGuard {
    inner: Arc<Mutex<PermissionInner>>,
}

#[derive(Default)]
struct PermissionInner {
    /// guild_id → effective_permissions (i64 bitfield, string-encoded by Discord)
    guild_permissions: HashMap<String, i64>,
    /// Set when the local user is a guild owner (implicitly has all permissions).
    owned_guilds: std::collections::HashSet<String>,
}

impl PermissionGuard {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Store the effective permission bitfield for a guild.
    ///
    /// `permissions_str` is the string-encoded `i64` that Discord returns in
    /// the role/member objects (e.g. `"8"` for Administrator).
    pub fn update_permissions(&self, guild_id: &str, permissions_str: &str) {
        let bits: i64 = permissions_str.parse().unwrap_or(0);
        let mut inner = self.inner.lock().expect("PermissionGuard lock poisoned");
        inner.guild_permissions.insert(guild_id.to_string(), bits);
    }

    /// Mark the local user as the owner of `guild_id` (owners bypass all checks).
    pub fn set_owner(&self, guild_id: &str, is_owner: bool) {
        let mut inner = self.inner.lock().expect("PermissionGuard lock poisoned");
        if is_owner {
            inner.owned_guilds.insert(guild_id.to_string());
        } else {
            inner.owned_guilds.remove(guild_id);
        }
    }

    /// Pre-flight check: does the local user have `required_perm` in `guild_id`?
    ///
    /// Returns `Ok(())` when:
    /// - The user owns the guild, OR
    /// - The cached permissions bitfield has the Administrator bit, OR
    /// - The required permission bit is set.
    ///
    /// Returns `Err(ClientError::PermissionDenied)` when:
    /// - No permissions have been cached yet for this guild AND the action is
    ///   a destructive moderation action — fails safe (deny, don't proceed).
    ///
    /// NOTE: Discord's own server-side check is authoritative; this is a
    /// defence-in-depth gate to avoid sending requests that will 403 (every
    /// 403 counts toward the 10k/10min IP ban threshold).
    pub fn check(&self, guild_id: &str, required_perm: i64, action: &str) -> Result<(), ClientError> {
        let inner = self.inner.lock().expect("PermissionGuard lock poisoned");
        if inner.owned_guilds.contains(guild_id) {
            return Ok(());
        }
        match inner.guild_permissions.get(guild_id) {
            None => {
                // No cached permissions — fail safe: deny.
                Err(ClientError::PermissionDenied(format!(
                    "{action}: guild permissions not cached — cannot verify"
                )))
            }
            Some(&bits) => {
                if (bits & PERM_ADMINISTRATOR) != 0 || (bits & required_perm) != 0 {
                    Ok(())
                } else {
                    Err(ClientError::PermissionDenied(format!(
                        "{action}: missing required permission (bit {required_perm})"
                    )))
                }
            }
        }
    }
}

// ── D.1 / D.6 — Typing rate cap ───────────────────────────────────────────

/// Per-channel typing fire-rate cap.
///
/// Discord throttles typing indicators server-side at 10 s, but we impose an
/// 8 s client-side gate to avoid racing the server throttle (which would still
/// count as a request toward the IP ban threshold).
#[derive(Clone, Default)]
pub struct TypingRateCap {
    inner: Arc<Mutex<HashMap<String, Instant>>>,
}

impl TypingRateCap {
    pub const WINDOW: Duration = Duration::from_secs(8);

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if a typing indicator should be sent on `channel_id`.
    ///
    /// Returns `false` when the previous indicator was sent within the 8 s window;
    /// the caller should silently drop the re-trigger.
    pub fn should_send(&self, channel_id: &str) -> bool {
        let mut map = self.inner.lock().expect("TypingRateCap lock poisoned");
        let now = Instant::now();
        let entry = map.entry(channel_id.to_string()).or_insert_with(|| {
            // First call — allow and set to a value old enough to not block.
            now - Self::WINDOW - Duration::from_secs(1)
        });
        let elapsed = now.duration_since(*entry);
        if elapsed >= Self::WINDOW {
            *entry = now;
            true
        } else {
            false
        }
    }
}

// ── D.3 — VoiceManager sketch ─────────────────────────────────────────────

/// Minimal guard that enforces at most one active voice session per account.
///
/// A real `VoiceSession` type lives in `clients/discord/src/voice/`; this
/// sketch holds an `Option<VoiceSessionHandle>` (a simple string token for
/// now) and returns `Err(ClientError::Network("AlreadyConnected"))` on a
/// second concurrent `connect()` call.
#[derive(Clone, Default)]
pub struct VoiceManager {
    inner: Arc<Mutex<Option<VoiceSessionHandle>>>,
}

/// Opaque handle identifying the active voice session.
#[derive(Clone, Debug)]
pub struct VoiceSessionHandle {
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub connected_at: Instant,
}

impl VoiceManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempt to open a new voice session.
    ///
    /// Returns `Ok(())` when no session is active and records the session.
    /// Returns `Err(ClientError::Network("AlreadyConnected"))` when a session
    /// is already open (matches the spec in Phase D table).
    pub fn connect(&self, channel_id: &str, guild_id: Option<&str>) -> Result<(), ClientError> {
        let mut guard = self.inner.lock().expect("VoiceManager lock poisoned");
        if guard.is_some() {
            return Err(ClientError::Network(
                "AlreadyConnected: voice session already active; disconnect first".into(),
            ));
        }
        *guard = Some(VoiceSessionHandle {
            channel_id: channel_id.to_string(),
            guild_id: guild_id.map(str::to_string),
            connected_at: Instant::now(),
        });
        Ok(())
    }

    /// Disconnect and clear the active session.
    pub fn disconnect(&self) {
        let mut guard = self.inner.lock().expect("VoiceManager lock poisoned");
        *guard = None;
    }

    /// Return a snapshot of the active session handle, if any.
    pub fn active_session(&self) -> Option<VoiceSessionHandle> {
        self.inner.lock().expect("VoiceManager lock poisoned").clone()
    }
}

// ── F.1 — GuardrailStats telemetry snapshot ───────────────────────────────

/// Snapshot of all anti-ban telemetry counters (Phase F.1).
///
/// Returned by [`DiscordClient::guardrail_stats`]. The counters correspond to
/// the `discord-anti-ban` grep targets documented in the monitoring runbook
/// (`docs/dev/discord-ban-incident.md`).
///
/// All fields are monotonically increasing since backend init.
#[derive(Clone, Debug, Default)]
pub struct GuardrailStats {
    /// Number of successful (2xx) HTTP responses.
    pub http_2xx: u64,
    /// Number of HTTP 401 Unauthorized responses.
    pub http_401: u64,
    /// Number of HTTP 403 Forbidden responses.
    pub http_403: u64,
    /// Number of HTTP 404 Not Found responses.
    pub http_404: u64,
    /// Number of HTTP 429 Too Many Requests responses.
    pub http_429: u64,
    /// Number of 5xx server-error responses.
    pub http_5xx: u64,
    /// Number of times the build-info scraper failed (fell back to floor or cache).
    pub scrape_fail: u64,
    /// Number of successful gateway IDENTIFY completions (gateway READY received).
    pub gateway_identify_success: u64,
    /// Number of gateway Invalid Session events received.
    pub gateway_invalid_session: u64,
    /// Number of times a rate-guard trip blocked an outbound request.
    pub rate_guard_trips: u64,
    /// Number of times slow-mode blocked a message send.
    pub slow_mode_trips: u64,
    /// Number of times the permission guard blocked a moderator action.
    pub permission_guard_trips: u64,
    /// Number of times the typing rate cap dropped a re-trigger.
    pub typing_cap_drops: u64,
}

/// Shared, thread-safe container for telemetry counters.
///
/// `DiscordHttpClient` and the gateway loop both clone this `Arc` and increment
/// counters in-place via `Arc<Mutex<GuardrailStats>>`.
#[derive(Clone, Default)]
pub struct GuardrailCounters {
    inner: Arc<Mutex<GuardrailStats>>,
}

impl GuardrailCounters {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a 2xx HTTP success.
    pub fn inc_2xx(&self) {
        if let Ok(mut s) = self.inner.lock() {
            s.http_2xx = s.http_2xx.saturating_add(1);
        }
    }

    /// Record a 401 response.
    pub fn inc_401(&self) {
        if let Ok(mut s) = self.inner.lock() {
            s.http_401 = s.http_401.saturating_add(1);
        }
        tracing::warn!(
            target: "discord-anti-ban",
            "discord.http.4xx.401 — Unauthorized (token revoked or session expired)"
        );
    }

    /// Record a 403 response on `route`.
    pub fn inc_403(&self, route: &str) {
        if let Ok(mut s) = self.inner.lock() {
            s.http_403 = s.http_403.saturating_add(1);
        }
        tracing::warn!(
            target: "discord-anti-ban",
            "discord.http.4xx.403 on {route} — permission denied (counts toward 10k/10min IP ban)"
        );
    }

    /// Record a 404 response.
    pub fn inc_404(&self) {
        if let Ok(mut s) = self.inner.lock() {
            s.http_404 = s.http_404.saturating_add(1);
        }
        tracing::info!(
            target: "discord-anti-ban",
            "discord.http.4xx.404 — resource not found"
        );
    }

    /// Record a 429 response on `route` with the `retry_after_secs` backoff.
    pub fn inc_429(&self, route: &str, retry_after_secs: u64) {
        if let Ok(mut s) = self.inner.lock() {
            s.http_429 = s.http_429.saturating_add(1);
        }
        tracing::warn!(
            target: "discord-anti-ban",
            "discord.http.4xx.429 on {route} — throttled by Discord; retry-after={retry_after_secs}s"
        );
    }

    /// Record a 5xx response.
    pub fn inc_5xx(&self, status: u16) {
        if let Ok(mut s) = self.inner.lock() {
            s.http_5xx = s.http_5xx.saturating_add(1);
        }
        tracing::warn!(
            target: "discord-anti-ban",
            "discord.http.5xx — server error HTTP {status}"
        );
    }

    /// Record a scraper failure (fell back to floor constant or stale cache).
    pub fn inc_scrape_fail(&self, reason: &str) {
        if let Ok(mut s) = self.inner.lock() {
            s.scrape_fail = s.scrape_fail.saturating_add(1);
        }
        tracing::warn!(
            target: "discord-anti-ban",
            "discord.http.scrape.fail — build-number scrape failed: {reason}"
        );
    }

    /// Record a successful gateway READY after IDENTIFY.
    pub fn inc_gateway_identify_success(&self, build_number: u32) {
        if let Ok(mut s) = self.inner.lock() {
            s.gateway_identify_success = s.gateway_identify_success.saturating_add(1);
        }
        tracing::info!(
            target: "discord-anti-ban",
            "discord.gateway.identify.success — gateway READY received (build_number={build_number})"
        );
    }

    /// Record an Invalid Session gateway event.
    pub fn inc_gateway_invalid_session(&self) {
        if let Ok(mut s) = self.inner.lock() {
            s.gateway_invalid_session = s.gateway_invalid_session.saturating_add(1);
        }
        tracing::warn!(
            target: "discord-anti-ban",
            "discord.gateway.invalid_session — session invalidated by Discord; will re-identify"
        );
    }

    /// Record a rate-guard trip (outbound request blocked by token bucket).
    pub fn inc_rate_guard_trip(&self) {
        if let Ok(mut s) = self.inner.lock() {
            s.rate_guard_trips = s.rate_guard_trips.saturating_add(1);
        }
        tracing::info!(
            target: "discord-anti-ban",
            "discord.guardrail.rate_guard.trip — outbound request held by token bucket"
        );
    }

    /// Record a slow-mode guard trip.
    pub fn inc_slow_mode_trip(&self, channel_id: &str) {
        if let Ok(mut s) = self.inner.lock() {
            s.slow_mode_trips = s.slow_mode_trips.saturating_add(1);
        }
        tracing::info!(
            target: "discord-anti-ban",
            "discord.guardrail.slow_mode.trip — channel {channel_id} is in slow mode; send blocked"
        );
    }

    /// Record a permission guard trip.
    pub fn inc_permission_trip(&self, action: &str, guild_id: &str) {
        if let Ok(mut s) = self.inner.lock() {
            s.permission_guard_trips = s.permission_guard_trips.saturating_add(1);
        }
        tracing::warn!(
            target: "discord-anti-ban",
            "discord.guardrail.permission.trip — {action} on guild {guild_id} blocked (missing permission)"
        );
    }

    /// Record a typing-cap drop (re-trigger suppressed within 8s window).
    pub fn inc_typing_cap_drop(&self, channel_id: &str) {
        if let Ok(mut s) = self.inner.lock() {
            s.typing_cap_drops = s.typing_cap_drops.saturating_add(1);
        }
        tracing::info!(
            target: "discord-anti-ban",
            "discord.guardrail.typing_cap.drop — typing re-trigger suppressed on {channel_id} (within 8s window)"
        );
    }

    /// Return a point-in-time snapshot of all counters.
    #[must_use]
    pub fn snapshot(&self) -> GuardrailStats {
        self.inner.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

// ── D.8 — DiscordHealth soft-warning surface ───────────────────────────────

/// Soft-warning health surface for the UI (D.8 + F.2).
///
/// Updated by the central response handler and rate-guard. The UI subscribes
/// to a `Signal<DiscordHealth>` and renders a "Backend health" panel (F.2).
#[derive(Clone, Debug, Default)]
pub struct DiscordHealth {
    /// Number of 429 responses seen since backend init.
    pub recent_429_count: u32,
    /// The route that last returned 403, if any.
    pub last_403_route: Option<String>,
    /// When the last 401 was seen (epoch seconds, for display).
    pub last_401_at: Option<u64>,
    /// The Discord build number currently in use (from `build_info.build_number`).
    pub build_number_in_use: u32,
    /// Unix epoch seconds when the build info was last scraped (0 = floor constant).
    pub build_info_scraped_at: u64,
    /// When the last 429 was seen (epoch seconds, for display).
    pub last_429_at: Option<u64>,
    /// Whether the build info is considered stale (scrape failed for > 14 days AND
    /// floor constant is > 30 days old). When `true` the UI shows a yellow warning.
    pub build_stale_warning: bool,
}

impl DiscordHealth {
    /// Update 429 telemetry from a [`RateGuard`] snapshot.
    pub fn update_from_rate_guard(&mut self, guard: &RateGuard) {
        let (count, last) = guard.health_snapshot();
        self.recent_429_count = count;
        // Convert Instant to epoch seconds by computing elapsed from now.
        if let Some(last_instant) = last {
            let elapsed_secs = Instant::now()
                .duration_since(last_instant)
                .as_secs();
            // Approximate epoch: current wall-clock minus elapsed.
            // We don't have access to SystemTime here (WASM-safe), so we store
            // the elapsed-seconds-ago value as a negative offset sentinel.
            // The UI can display "last 429 was N seconds ago" from this.
            // Store as saturating subtraction from a large sentinel so 0 means "none".
            self.last_429_at = Some(elapsed_secs);
        }
    }

    /// Record a 403 response for a route.
    pub fn record_403(&mut self, route: &str) {
        self.last_403_route = Some(route.to_string());
    }

    /// Record a 401 response (epoch seconds from caller).
    pub fn record_401(&mut self, epoch_secs: u64) {
        self.last_401_at = Some(epoch_secs);
    }

    /// Update build-info fields for the F.2 health panel.
    ///
    /// `scraped_at` is the Unix epoch seconds from `BuildInfo::scraped_at`
    /// (0 = synthesised floor constant, never actually scraped).
    /// `stale` should be set by [`check_build_staleness`] in `build_info.rs`.
    pub fn update_build_info(&mut self, build_number: u32, scraped_at: u64, stale: bool) {
        self.build_number_in_use = build_number;
        self.build_info_scraped_at = scraped_at;
        self.build_stale_warning = stale;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    // ── RateGuard tests ───────────────────────────────────────────────────

    #[test]
    fn rate_guard_allows_burst() {
        let g = RateGuard::new();
        for _ in 0..10 {
            assert!(g.check().is_ok(), "burst of 10 should be allowed");
        }
    }

    #[test]
    fn rate_guard_blocks_after_burst() {
        let g = RateGuard::new();
        for _ in 0..10 {
            let _ = g.check();
        }
        // 11th request should fail
        assert!(g.check().is_err(), "11th request should be rate-limited");
    }

    #[test]
    fn rate_guard_429_backoff_doubles_on_repeated() {
        let g = RateGuard::new();
        let d1 = g.record_429("test_bucket", 1);
        let d2 = g.record_429("test_bucket", 1); // second within 60s
        assert!(d2 > d1, "second 429 should have higher backoff");
    }

    #[test]
    fn rate_guard_health_snapshot() {
        let g = RateGuard::new();
        g.record_429("b", 1);
        g.record_429("b", 1);
        let (count, last) = g.health_snapshot();
        assert_eq!(count, 2);
        assert!(last.is_some());
    }

    // ── SlowModeGuard tests ───────────────────────────────────────────────

    #[test]
    fn slow_mode_allows_when_zero() {
        let g = SlowModeGuard::new();
        assert!(g.check("ch1", 0).is_ok());
        assert!(g.check("ch1", 0).is_ok());
    }

    #[test]
    fn slow_mode_blocks_second_send_in_window() {
        let g = SlowModeGuard::new();
        assert!(g.check("ch2", 5).is_ok(), "first send should pass");
        assert!(g.check("ch2", 5).is_err(), "second immediate send should be blocked");
    }

    #[test]
    fn slow_mode_different_channels_independent() {
        let g = SlowModeGuard::new();
        assert!(g.check("ch_a", 5).is_ok());
        assert!(g.check("ch_b", 5).is_ok(), "different channel should not be affected");
    }

    // ── PermissionGuard tests ─────────────────────────────────────────────

    #[test]
    fn permission_guard_allows_admin() {
        let g = PermissionGuard::new();
        g.update_permissions("g1", &PERM_ADMINISTRATOR.to_string());
        assert!(g.check("g1", PERM_KICK_MEMBERS, "kick").is_ok());
    }

    #[test]
    fn permission_guard_allows_specific_perm() {
        let g = PermissionGuard::new();
        g.update_permissions("g1", &PERM_KICK_MEMBERS.to_string());
        assert!(g.check("g1", PERM_KICK_MEMBERS, "kick").is_ok());
    }

    #[test]
    fn permission_guard_denies_missing_perm() {
        let g = PermissionGuard::new();
        g.update_permissions("g1", "0"); // no permissions
        assert!(g.check("g1", PERM_KICK_MEMBERS, "kick").is_err());
    }

    #[test]
    fn permission_guard_denies_unknown_guild() {
        let g = PermissionGuard::new();
        // No permissions stored for "g_unknown"
        assert!(g.check("g_unknown", PERM_BAN_MEMBERS, "ban").is_err());
    }

    #[test]
    fn permission_guard_owner_bypasses_all() {
        let g = PermissionGuard::new();
        g.update_permissions("g1", "0");
        g.set_owner("g1", true);
        assert!(g.check("g1", PERM_MODERATE_MEMBERS, "timeout").is_ok());
    }

    // ── TypingRateCap tests ───────────────────────────────────────────────

    #[test]
    fn typing_cap_allows_first_send() {
        let cap = TypingRateCap::new();
        assert!(cap.should_send("ch1"), "first typing trigger should be allowed");
    }

    #[test]
    fn typing_cap_blocks_within_window() {
        let cap = TypingRateCap::new();
        assert!(cap.should_send("ch1"));
        assert!(!cap.should_send("ch1"), "re-trigger within window should be dropped");
    }

    #[test]
    fn typing_cap_different_channels_independent() {
        let cap = TypingRateCap::new();
        assert!(cap.should_send("ch_x"));
        assert!(cap.should_send("ch_y"), "different channel should be unaffected");
    }

    // ── VoiceManager tests ────────────────────────────────────────────────

    #[test]
    fn voice_manager_allows_first_connect() {
        let vm = VoiceManager::new();
        assert!(vm.connect("ch1", Some("g1")).is_ok());
    }

    #[test]
    fn voice_manager_rejects_second_connect() {
        let vm = VoiceManager::new();
        vm.connect("ch1", Some("g1")).unwrap();
        assert!(vm.connect("ch2", Some("g1")).is_err(), "second connect should fail");
    }

    #[test]
    fn voice_manager_allows_reconnect_after_disconnect() {
        let vm = VoiceManager::new();
        vm.connect("ch1", None).unwrap();
        vm.disconnect();
        assert!(vm.connect("ch2", None).is_ok(), "reconnect after disconnect should work");
    }

    // ── DiscordHealth tests ───────────────────────────────────────────────

    #[test]
    fn discord_health_update_from_rate_guard() {
        let g = RateGuard::new();
        g.record_429("b", 1);
        let mut health = DiscordHealth::default();
        health.update_from_rate_guard(&g);
        assert_eq!(health.recent_429_count, 1);
    }

    #[test]
    fn discord_health_records_403_and_401() {
        let mut h = DiscordHealth::default();
        h.record_403("/api/v10/guilds/123/members/456");
        h.record_401(1_700_000_000);
        assert!(h.last_403_route.is_some());
        assert_eq!(h.last_401_at, Some(1_700_000_000));
    }
}
