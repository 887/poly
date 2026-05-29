//! # poly-reddit
//!
//! Reddit client for Poly. Scrapes `old.reddit.com` HTML rather than using
//! Reddit's REST/OAuth API — Reddit killed third-party API access mid-2023
//! and the remaining tiers are throttled or enterprise-priced. `old.reddit.com`
//! is server-rendered, structurally stable since 2018, and explicitly
//! maintained by Reddit (the user-prefs toggle keeps it as the default UI).
//!
//! ## Build modes
//!
//! - **Native** (`--features native`): implements `ClientBackend` directly
//!   using `reqwest` + `scraper`. (Trait impl shipped in later phases — see
//!   `docs/plans/plan-reddit-stub.md` Phase D-E. Phase A scaffolds the crate
//!   only.)
//!
//! ## Gating
//!
//! Not in poly-core's default features. Opt-in via `--features reddit` —
//! same model as Discord and Teams. The TOS gray area around scraping is
//! the explicit reason for keeping it out of release builds.
//!
//! ## Module layout (SOLID-audit-reddit B.5)
//!
//! - [`client`] — [`RedditClient`] HTTP transport split per domain:
//!   - `client/mod.rs` — struct + constructors + session / fetch helpers
//!   - `client/auth.rs` — login / session probe
//!   - `client/read.rs` — subreddit / post / user / search / inbox reads
//!   - `client/write.rs` — DM / vote / submit / edit / delete writes
//! - [`backend`] — `ClientBackend` impl, ID mapping, parser → poly_client conversions
//! - [`parser`] — old.reddit HTML scrapers (subreddit, post, user, inbox)
//! - [`signup`] — account-creation flow

/// The backend slug used in all [`poly_client::BackendType`] constructions for this crate.
pub const SLUG: &str = "reddit";

#[cfg(feature = "native")]
pub mod backend;

#[cfg(feature = "native")]
mod client;

#[cfg(feature = "native")]
pub mod parser;

#[cfg(feature = "native")]
pub mod signup;

#[cfg(feature = "native")]
pub use client::RedditClient;

#[cfg(feature = "native")]
use parser::ParseError;

/// Return Fluent translations for the given locale.
#[must_use]
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

/// Errors returned by the read-flow methods on [`RedditClient`].
#[cfg(feature = "native")]
#[derive(Debug, thiserror::Error)]
pub enum RedditError {
    /// Network or transport-level failure.
    #[error("HTTP error: {0}")]
    Http(String),
    /// HTML parser rejected the response (selector miss, malformed
    /// timestamp, etc.).
    #[error("parse error: {0}")]
    Parse(#[from] ParseError),
    /// Response was the login page — caller's session cookie is missing
    /// or expired (or the requested resource requires auth).
    #[error("response is logged out (cookie missing or expired)")]
    LoggedOut,
    /// Reddit returned a non-success status that is not a redirect.
    /// Includes the HTTP status code.
    #[error("HTTP {0}")]
    Status(u16),
}

#[cfg(feature = "native")]
impl From<reqwest::Error> for RedditError {
    fn from(e: reqwest::Error) -> Self {
        Self::Http(e.to_string())
    }
}

/// Display-relevant info about a subscribed subreddit.
#[cfg(feature = "native")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubredditInfo {
    /// `display_name` (without the `r/` prefix).
    pub name: String,
    /// Optional resolved icon URL (community_icon preferred, icon_img
    /// fallback, HTML entities decoded).
    pub icon_url: Option<String>,
}

/// Sort options for subreddit listings.
#[cfg(feature = "native")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKind {
    /// Default ordering — engagement + recency.
    Hot,
    /// Newest first.
    New,
    /// Highest scoring all-time (or per Reddit's default top window).
    Top,
    /// Posts gaining traction quickly.
    Rising,
    /// High-engagement, mixed-vote posts.
    Controversial,
    /// Top posts from the past hour.
    TopHour,
    /// Top posts from the past day.
    TopDay,
    /// Top posts from the past week.
    TopWeek,
    /// Top posts from the past month.
    TopMonth,
    /// Top posts from the past year.
    TopYear,
    /// Top posts of all time.
    TopAll,
}

#[cfg(feature = "native")]
impl SortKind {
    /// URL path segment for this sort.
    #[must_use]
    pub const fn as_path(self) -> &'static str {
        match self {
            Self::Hot => "hot",
            Self::New => "new",
            Self::Top | Self::TopHour | Self::TopDay | Self::TopWeek
            | Self::TopMonth | Self::TopYear | Self::TopAll => "top",
            Self::Rising => "rising",
            Self::Controversial => "controversial",
        }
    }

    /// Optional `?t=` query parameter for time-windowed top sorts.
    ///
    /// Returns `None` for sorts that don't need a time filter.
    #[must_use]
    pub const fn time_filter(self) -> Option<&'static str> {
        match self {
            Self::TopHour => Some("hour"),
            Self::TopDay => Some("day"),
            Self::TopWeek => Some("week"),
            Self::TopMonth => Some("month"),
            Self::TopYear => Some("year"),
            Self::TopAll => Some("all"),
            Self::Hot | Self::New | Self::Rising | Self::Controversial | Self::Top => None,
        }
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn sort_kind_path_segments() {
        assert_eq!(SortKind::Hot.as_path(), "hot");
        assert_eq!(SortKind::New.as_path(), "new");
        assert_eq!(SortKind::Top.as_path(), "top");
        assert_eq!(SortKind::Rising.as_path(), "rising");
        assert_eq!(SortKind::Controversial.as_path(), "controversial");
        // Sub-period Top variants all map to "top" path.
        assert_eq!(SortKind::TopHour.as_path(), "top");
        assert_eq!(SortKind::TopDay.as_path(), "top");
        assert_eq!(SortKind::TopWeek.as_path(), "top");
        assert_eq!(SortKind::TopMonth.as_path(), "top");
        assert_eq!(SortKind::TopYear.as_path(), "top");
        assert_eq!(SortKind::TopAll.as_path(), "top");
    }

    #[test]
    fn sort_kind_time_filter() {
        assert_eq!(SortKind::Hot.time_filter(), None);
        assert_eq!(SortKind::Top.time_filter(), None);
        assert_eq!(SortKind::TopHour.time_filter(), Some("hour"));
        assert_eq!(SortKind::TopDay.time_filter(), Some("day"));
        assert_eq!(SortKind::TopWeek.time_filter(), Some("week"));
        assert_eq!(SortKind::TopMonth.time_filter(), Some("month"));
        assert_eq!(SortKind::TopYear.time_filter(), Some("year"));
        assert_eq!(SortKind::TopAll.time_filter(), Some("all"));
    }
}
