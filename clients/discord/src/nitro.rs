//! Discord Nitro tier detection and feature gating (Phase E).
//!
//! Provides:
//!
//! - [`NitroTier`] — derived from `DiscordUser.premium_type` (E.2).
//! - [`NitroGate`] — helpers that gate Nitro-only features at both the UI
//!   affordance layer and the HTTP client layer (E.4).
//!
//! **Policy:** We intentionally refuse to send Nitro-gated requests even when
//! the Discord API would technically accept them.  The goal is to give Discord
//! zero anomalous signal: a user without Nitro sending a 50 MB file is a
//! self-bot heuristic trigger, not a policy we want to exercise.
//! See `docs/dev/discord-nitro.md` for the full rationale.

use poly_client::ClientError;

// ── E.2 — NitroTier ───────────────────────────────────────────────────────

/// Discord Nitro subscription tier, derived from `premium_type` on the
/// `DiscordUser` object (`GET /users/@me`).
///
/// | `premium_type` | Tier              |
/// |----------------|-------------------|
/// | 0 or absent    | None              |
/// | 1              | Nitro Classic     |
/// | 2              | Nitro             |
/// | 3              | Nitro Basic       |
///
/// Source: discord-api-types v10 `UserPremiumType` enum.
// PartialOrd/Ord are MANUALLY implemented (not derived) because Discord's
// `premium_type` discriminants don't reflect capability hierarchy:
// None=0, Classic=1, Full=2, Basic=3. A derived Ord would sort by
// discriminant — making `Basic >= Classic` return true, which is wrong
// (Basic is the WEAKEST paid tier — only animated-emoji privileges,
// strictly less than Classic). We keep the on-wire discriminants and map
// to a capability rank in the Ord impl below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NitroTier {
    #[default]
    None = 0,
    /// Nitro Classic (legacy) — animated avatars, server stickers, custom emojis,
    /// profile banners.  No server boosts or 50 MB upload.
    Classic = 1,
    /// Nitro Full — all Classic features, 2× server boosts, 50 MB upload,
    /// super-reactions, GIF avatars, animated profile banners.
    Full = 2,
    /// Nitro Basic — animated emoji use only; no upload bump, no boosts.
    Basic = 3,
}

impl NitroTier {
    /// Capability rank used by Ord — higher == more privileges.
    /// Distinct from Discord's `premium_type` discriminant on the wire.
    #[must_use]
    pub fn capability_rank(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Basic => 1,
            Self::Classic => 2,
            Self::Full => 3,
        }
    }
}

impl PartialOrd for NitroTier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NitroTier {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.capability_rank().cmp(&other.capability_rank())
    }
}

impl From<u8> for NitroTier {
    fn from(v: u8) -> Self {
        match v {
            1 => Self::Classic,
            2 => Self::Full,
            3 => Self::Basic,
            _ => Self::None,
        }
    }
}

impl NitroTier {
    /// Return the raw `premium_type` integer value.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Convenience: does the user have ANY active Nitro subscription?
    #[must_use]
    pub fn has_any_nitro(self) -> bool {
        !matches!(self, Self::None)
    }
}

// ── E.4 — NitroGate feature helpers ───────────────────────────────────────

/// Feature-gate helpers.  Each `can_*` method returns `true` when the given
/// tier is sufficient.  The corresponding `check_*` methods return
/// `Err(ClientError::PermissionDenied("Nitro required"))` for use in HTTP
/// client-layer defence-in-depth.
///
/// **Status:** speculative Phase E.4/E.5 infrastructure.  No call site in
/// `clients/discord/src/` exercises these helpers today — the Discord client
/// has no file-upload, avatar-set, or sticker/emoji-send paths that would
/// trigger them.  Tests cover the API contract so wiring them when those
/// paths land is a one-line change.
// lint-allow-unused: Phase E.4/E.5 defence-in-depth; no upload/avatar/emoji path wired.
#[allow(dead_code)]
pub struct NitroGate;

// lint-allow-unused: Phase E.4/E.5 defence-in-depth; no upload/avatar/emoji path wired.
#[allow(dead_code)]
impl NitroGate {
    // ── E.4 helpers ────────────────────────────────────────────────────────

    /// Server stickers from other servers require at least Nitro Classic.
    #[must_use]
    pub fn can_use_cross_server_stickers(tier: NitroTier) -> bool {
        tier >= NitroTier::Classic
    }

    /// Animated cross-server emoji use requires at least Nitro Classic.
    #[must_use]
    pub fn can_use_animated_emoji(tier: NitroTier) -> bool {
        tier >= NitroTier::Classic
    }

    /// Profile banners require at least Nitro Classic.
    #[must_use]
    pub fn can_set_profile_banner(tier: NitroTier) -> bool {
        tier >= NitroTier::Classic
    }

    /// GIF avatars require full Nitro.
    #[must_use]
    pub fn can_use_gif_avatar(tier: NitroTier) -> bool {
        tier >= NitroTier::Full
    }

    /// Super-reactions require full Nitro.
    #[must_use]
    pub fn can_use_super_reactions(tier: NitroTier) -> bool {
        tier >= NitroTier::Full
    }

    // ── E.5 — Upload boundary ──────────────────────────────────────────────

    /// Maximum upload size in bytes for the given tier and channel boost level.
    ///
    /// | Tier / Boost | Limit  |
    /// |--------------|--------|
    /// | None, Basic  | 8 MB   |
    /// | Classic      | 50 MB  |
    /// | Full         | 50 MB  |
    /// | Boost Tier 2+ (any tier) | 50 MB |
    /// | Boost Tier 3 (any tier)  | 100 MB |
    ///
    /// Note: server boost tier overrides the base user-tier limit.
    #[must_use]
    pub fn max_upload_bytes(tier: NitroTier, guild_boost_level: u8) -> u64 {
        const MB: u64 = 1024 * 1024;
        match guild_boost_level {
            3 => 100 * MB,
            2 => 50 * MB,
            _ => match tier {
                NitroTier::Full | NitroTier::Classic => 50 * MB,
                _ => 8 * MB,
            },
        }
    }

    /// Defence-in-depth check before `send_message_with_attachments`.
    ///
    /// Returns `Err(ClientError::PermissionDenied)` when the attachment byte
    /// count exceeds `max_upload_bytes(tier, boost_level)`.
    pub fn check_upload_size(
        tier: NitroTier,
        guild_boost_level: u8,
        total_bytes: u64,
    ) -> Result<(), ClientError> {
        let limit = Self::max_upload_bytes(tier, guild_boost_level);
        if total_bytes > limit {
            Err(ClientError::PermissionDenied(format!(
                "attachment too large: {} bytes exceeds the {}-byte limit for your Nitro tier \
                 (Nitro required for larger uploads)",
                total_bytes, limit
            )))
        } else {
            Ok(())
        }
    }

    /// Defence-in-depth check: reject GIF avatars without Nitro Full.
    pub fn check_gif_avatar(tier: NitroTier) -> Result<(), ClientError> {
        if !Self::can_use_gif_avatar(tier) {
            Err(ClientError::PermissionDenied(
                "GIF avatars require Nitro (not Nitro Classic or Basic)".into(),
            ))
        } else {
            Ok(())
        }
    }

    /// Defence-in-depth check: reject animated cross-server emoji without Nitro.
    pub fn check_animated_emoji(tier: NitroTier) -> Result<(), ClientError> {
        if !Self::can_use_animated_emoji(tier) {
            Err(ClientError::PermissionDenied(
                "animated emoji from other servers requires Nitro Classic or higher".into(),
            ))
        } else {
            Ok(())
        }
    }
}

// ── E.3 — Account info cache (thin struct, owned by DiscordClient) ─────────

/// Cached Nitro tier for the authenticated account.
///
/// Populated on `get_me()` at backend init; refreshed on app focus.
/// The `DiscordClient` owns a `Mutex<DiscordAccountInfo>`.
#[derive(Debug, Clone, Default)]
pub struct DiscordAccountInfo {
    /// Nitro tier from `premium_type`, or `None` before `get_me()` is called.
    pub nitro_tier: Option<NitroTier>,
}

impl DiscordAccountInfo {
    /// Update from the `premium_type` field returned by `GET /users/@me`.
    pub fn update_nitro_tier(&mut self, premium_type: Option<u8>) {
        self.nitro_tier = Some(premium_type.map_or(NitroTier::None, NitroTier::from));
    }

    /// Returns the current tier, defaulting to `NitroTier::None` if not yet fetched.
    #[must_use]
    pub fn tier(&self) -> NitroTier {
        self.nitro_tier.unwrap_or(NitroTier::None)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    // ── NitroTier conversion ───────────────────────────────────────────────

    #[test]
    fn nitro_tier_from_u8() {
        assert_eq!(NitroTier::from(0), NitroTier::None);
        assert_eq!(NitroTier::from(1), NitroTier::Classic);
        assert_eq!(NitroTier::from(2), NitroTier::Full);
        assert_eq!(NitroTier::from(3), NitroTier::Basic);
        assert_eq!(NitroTier::from(99), NitroTier::None);
    }

    #[test]
    fn nitro_tier_has_any_nitro() {
        assert!(!NitroTier::None.has_any_nitro());
        assert!(NitroTier::Classic.has_any_nitro());
        assert!(NitroTier::Full.has_any_nitro());
        assert!(NitroTier::Basic.has_any_nitro());
    }

    #[test]
    fn nitro_tier_ordering() {
        assert!(NitroTier::Full > NitroTier::Classic);
        assert!(NitroTier::Classic > NitroTier::None);
        // Basic is the raw int 3 but logically "less than" Full (int 2) by the
        // PartialOrd derive which compares discriminant values numerically.
        // The as_u8() is load-bearing for deserialization; ordering is only
        // used in can_* helpers where Basic < Classic < Full is the intent.
        // The enum definition mirrors Discord's premium_type integer values.
    }

    // ── NitroGate feature checks ──────────────────────────────────────────

    #[test]
    fn stickers_require_classic() {
        assert!(!NitroGate::can_use_cross_server_stickers(NitroTier::None));
        assert!(!NitroGate::can_use_cross_server_stickers(NitroTier::Basic));
        assert!(NitroGate::can_use_cross_server_stickers(NitroTier::Classic));
        assert!(NitroGate::can_use_cross_server_stickers(NitroTier::Full));
    }

    #[test]
    fn gif_avatar_requires_full() {
        assert!(!NitroGate::can_use_gif_avatar(NitroTier::None));
        assert!(!NitroGate::can_use_gif_avatar(NitroTier::Classic));
        assert!(!NitroGate::can_use_gif_avatar(NitroTier::Basic));
        assert!(NitroGate::can_use_gif_avatar(NitroTier::Full));
    }

    #[test]
    fn super_reactions_require_full() {
        assert!(!NitroGate::can_use_super_reactions(NitroTier::None));
        assert!(NitroGate::can_use_super_reactions(NitroTier::Full));
    }

    // ── Upload boundary ───────────────────────────────────────────────────

    #[test]
    fn upload_limit_no_nitro() {
        let limit = NitroGate::max_upload_bytes(NitroTier::None, 0);
        assert_eq!(limit, 8 * 1024 * 1024, "non-Nitro: 8 MB");
    }

    #[test]
    fn upload_limit_nitro_classic() {
        let limit = NitroGate::max_upload_bytes(NitroTier::Classic, 0);
        assert_eq!(limit, 50 * 1024 * 1024, "Nitro Classic: 50 MB");
    }

    #[test]
    fn upload_limit_nitro_full() {
        let limit = NitroGate::max_upload_bytes(NitroTier::Full, 0);
        assert_eq!(limit, 50 * 1024 * 1024, "Nitro Full: 50 MB");
    }

    #[test]
    fn upload_limit_boost_tier_2() {
        let limit = NitroGate::max_upload_bytes(NitroTier::None, 2);
        assert_eq!(limit, 50 * 1024 * 1024, "Boost Tier 2: 50 MB regardless of Nitro");
    }

    #[test]
    fn upload_limit_boost_tier_3() {
        let limit = NitroGate::max_upload_bytes(NitroTier::None, 3);
        assert_eq!(limit, 100 * 1024 * 1024, "Boost Tier 3: 100 MB");
    }

    #[test]
    fn check_upload_size_blocks_over_limit() {
        let result =
            NitroGate::check_upload_size(NitroTier::None, 0, 10 * 1024 * 1024);
        assert!(result.is_err(), "10 MB should be blocked for non-Nitro user");
    }

    #[test]
    fn check_upload_size_allows_under_limit() {
        let result =
            NitroGate::check_upload_size(NitroTier::None, 0, 4 * 1024 * 1024);
        assert!(result.is_ok(), "4 MB should be allowed for non-Nitro user");
    }

    // ── DiscordAccountInfo ────────────────────────────────────────────────

    #[test]
    fn account_info_update_nitro_tier() {
        let mut info = DiscordAccountInfo::default();
        assert_eq!(info.tier(), NitroTier::None, "default is None");
        info.update_nitro_tier(Some(2));
        assert_eq!(info.tier(), NitroTier::Full);
    }

    #[test]
    fn account_info_none_premium_type() {
        let mut info = DiscordAccountInfo::default();
        info.update_nitro_tier(None);
        assert_eq!(info.tier(), NitroTier::None);
    }
}
