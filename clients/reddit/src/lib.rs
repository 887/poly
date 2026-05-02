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

#[cfg(feature = "native")]
pub mod parser;

#[cfg(feature = "native")]
pub mod signup;

#[cfg(feature = "native")]
use parser::{ParseError, RawComment, RawPost, UserProfile};

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
}

#[cfg(feature = "native")]
impl SortKind {
    /// URL path segment for this sort.
    #[must_use]
    pub fn as_path(self) -> &'static str {
        match self {
            Self::Hot => "hot",
            Self::New => "new",
            Self::Top => "top",
            Self::Rising => "rising",
            Self::Controversial => "controversial",
        }
    }
}

/// Reddit HTML-scraping client.
///
/// Holds the HTTP client (with cookie jar, populated by Phase C login)
/// and the scraping base URL. Phase D-anonymous adds the read-only
/// methods; Phase C+E will add cookie auth and write flows.
#[cfg(feature = "native")]
pub struct RedditClient {
    /// HTTP client with cookie jar — login persists `reddit_session` here.
    http: reqwest::Client,
    /// Scraping base — production: `https://old.reddit.com`.
    /// Test backend (Phase F) overrides via `REDDIT_BASE_URL` env to
    /// `http://127.0.0.1:9108`.
    base_url: String,
}

#[cfg(feature = "native")]
impl RedditClient {
    /// Default User-Agent. Reddit aggressively rate-limits the bare
    /// `curl/` and `python-requests/` UAs; spoofing a real browser is
    /// the standard mitigation. Live capture confirmed this exact UA
    /// works against `old.reddit.com` (2026-05-02).
    const DEFAULT_UA: &'static str =
        "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0";

    /// Create a new Reddit client pointed at the default `old.reddit.com`.
    ///
    /// # Errors
    ///
    /// Returns an error if `reqwest::Client` construction fails (extremely
    /// rare — only when the system TLS backend is unavailable).
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::with_base_url("https://old.reddit.com".to_string())
    }

    /// Create a new Reddit client pointed at `base_url`. Used by integration
    /// tests against `servers/test-reddit/` (port 9108) and by the
    /// `REDDIT_BASE_URL` env override.
    ///
    /// # Errors
    ///
    /// Returns an error if `reqwest::Client` construction fails.
    pub fn with_base_url(base_url: String) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .cookie_store(true)
            .user_agent(Self::DEFAULT_UA)
            .build()?;
        Ok(Self { http, base_url })
    }

    /// The configured base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The underlying HTTP client. Used by parser modules in Phase B.
    #[must_use]
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Build a full URL by joining the base + path. Centralised so the
    /// `REDDIT_BASE_URL` test override works uniformly.
    fn resolve_url(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{base}/{path}")
    }

    /// Fetch a page and return its body, mapping non-2xx → `RedditError`.
    async fn fetch_text(&self, url: &str) -> Result<String, RedditError> {
        let resp = self.http.get(url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(RedditError::Status(status.as_u16()));
        }
        Ok(resp.text().await?)
    }

    /// List the posts in a subreddit at the given sort.
    ///
    /// Anonymous — works without auth.
    ///
    /// # Errors
    ///
    /// - `RedditError::Status` for HTTP non-2xx.
    /// - `RedditError::Parse` if the HTML structure is unexpected.
    /// - `RedditError::LoggedOut` if Reddit redirects to the login page
    ///   (can happen for quarantined or NSFW-gated subs).
    pub async fn list_subreddit(
        &self,
        subreddit: &str,
        sort: SortKind,
    ) -> Result<Vec<RawPost>, RedditError> {
        let path = format!("/r/{subreddit}/{}/", sort.as_path());
        let url = self.resolve_url(&path);
        let html = self.fetch_text(&url).await?;
        Ok(parser::subreddit::parse_listing(&html)?)
    }

    /// Fetch a single post + its full nested comment tree by post ID.
    ///
    /// The bare `/comments/<id>/` URL works — Reddit 301-redirects to
    /// add the canonical slug, and `reqwest` follows redirects by
    /// default.
    ///
    /// Anonymous — works without auth.
    ///
    /// # Errors
    ///
    /// Same as [`Self::list_subreddit`].
    pub async fn get_post(
        &self,
        post_id: &str,
    ) -> Result<(RawPost, Vec<RawComment>), RedditError> {
        let path = format!("/comments/{post_id}/");
        let url = self.resolve_url(&path);
        let html = self.fetch_text(&url).await?;
        Ok(parser::post::parse_post_page(&html)?)
    }

    /// Fetch a user's overview page.
    ///
    /// Anonymous — works without auth.
    ///
    /// # Errors
    ///
    /// Same as [`Self::list_subreddit`]. `RedditError::Status(404)` if
    /// the user doesn't exist or has been suspended.
    pub async fn get_user(&self, username: &str) -> Result<UserProfile, RedditError> {
        let path = format!("/user/{username}/");
        let url = self.resolve_url(&path);
        let html = self.fetch_text(&url).await?;
        Ok(parser::user::parse_user_overview(&html)?)
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
    }

    #[test]
    fn resolve_url_joins_cleanly() {
        let c = RedditClient::with_base_url("https://old.reddit.com/".to_string()).unwrap();
        assert_eq!(c.resolve_url("/r/rust/hot/"), "https://old.reddit.com/r/rust/hot/");
        assert_eq!(c.resolve_url("r/rust/hot/"), "https://old.reddit.com/r/rust/hot/");

        let c2 = RedditClient::with_base_url("http://127.0.0.1:9108".to_string()).unwrap();
        assert_eq!(c2.resolve_url("/api/me.json"), "http://127.0.0.1:9108/api/me.json");
    }

    #[test]
    fn default_ua_is_a_real_browser_string() {
        // Sanity — if we ever swap to "poly-reddit/x.y" by accident,
        // Reddit will rate-limit. This guards the regression.
        assert!(RedditClient::DEFAULT_UA.contains("Firefox"));
        assert!(RedditClient::DEFAULT_UA.contains("Mozilla"));
    }
}
