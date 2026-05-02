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
pub mod backend;

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

    /// POST a form-encoded body. Workspace reqwest disables
    /// default-features (drops `.form()`), so we encode + set the
    /// Content-Type manually.
    async fn post_form(
        &self,
        url: &str,
        fields: &[(&str, &str)],
    ) -> Result<reqwest::Response, RedditError> {
        let body = serde_urlencoded::to_string(fields)
            .map_err(|e| RedditError::Http(e.to_string()))?;
        Ok(self
            .http
            .post(url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .send()
            .await?)
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

    // ── Phase C: cookie auth ────────────────────────────────────────────

    /// Log in by username + password (test backend only).
    ///
    /// Real `old.reddit.com` returns 403 on `/api/login/<user>` since
    /// 2024+ — this method is for tests against `poly-test-reddit`. For
    /// production use, see [`Self::login_with_session_cookie`].
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for non-2xx HTTP, `RedditError::LoggedOut`
    /// if Reddit replies with a wrong-password JSON error.
    pub async fn login_with_password(
        &self,
        username: &str,
        password: &str,
    ) -> Result<(), RedditError> {
        let path = format!("/api/login/{username}");
        let url = self.resolve_url(&path);
        let resp = self
            .post_form(
                &url,
                &[
                    ("user", username),
                    ("passwd", password),
                    ("api_type", "json"),
                ],
            )
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(RedditError::Status(status.as_u16()));
        }
        // The cookie jar on `self.http` auto-stores any Set-Cookie headers
        // — including the `reddit_session` we want.
        let body: serde_json::Value = resp.json().await?;
        let errors = body
            .get("json")
            .and_then(|j| j.get("errors"))
            .and_then(|e| e.as_array());
        if let Some(errs) = errors
            && !errs.is_empty()
        {
            return Err(RedditError::LoggedOut);
        }
        Ok(())
    }

    /// Bring-your-own-cookie auth path. The user pastes their
    /// `reddit_session` cookie value (from a logged-in browser), and we
    /// pre-seed the jar so subsequent requests authenticate as them.
    ///
    /// This is the only viable production auth path for real reddit
    /// (the password endpoint returns 403). Intended UI: a settings
    /// field where the user pastes the cookie value.
    ///
    /// # Errors
    ///
    /// `RedditError::Http` if the cookie can't be parsed into the jar.
    pub fn login_with_session_cookie(
        &mut self,
        cookie_value: &str,
    ) -> Result<(), RedditError> {
        // reqwest's default Client builder doesn't expose a way to
        // re-seed cookies after construction. Rebuild the client with a
        // fresh jar pre-populated.
        let jar = std::sync::Arc::new(reqwest::cookie::Jar::default());
        let url = self
            .base_url
            .parse::<reqwest::Url>()
            .map_err(|e| RedditError::Http(e.to_string()))?;
        jar.add_cookie_str(
            &format!("reddit_session={cookie_value}; Path=/"),
            &url,
        );
        self.http = reqwest::Client::builder()
            .cookie_provider(jar)
            .user_agent(Self::DEFAULT_UA)
            .build()?;
        Ok(())
    }

    /// Probe whether the current session is authenticated.
    ///
    /// `GET /api/me.json` — anonymous responses have empty `data.name`,
    /// authenticated ones have the user fields populated.
    ///
    /// # Errors
    ///
    /// `RedditError::Http` / `Status` for transport-level failures.
    pub async fn is_logged_in(&self) -> Result<Option<String>, RedditError> {
        let url = self.resolve_url("/api/me.json");
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        let body: serde_json::Value = resp.json().await?;
        Ok(body
            .get("data")
            .and_then(|d| d.get("name"))
            .and_then(|n| n.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned))
    }

    // ── Phase D (auth-gated): inbox + subscribed list ───────────────────

    /// Fetch the authenticated user's DM inbox.
    ///
    /// # Errors
    ///
    /// `RedditError::LoggedOut` if the session cookie is missing or
    /// expired (response will be the login page).
    pub async fn inbox(&self) -> Result<Vec<parser::RawDm>, RedditError> {
        let url = self.resolve_url("/message/inbox/");
        let html = self.fetch_text(&url).await?;
        Ok(parser::inbox::parse_inbox(&html)?)
    }

    // ── Phase E: write flows ────────────────────────────────────────────

    /// Send a private message (DM).
    ///
    /// **Note:** real `old.reddit.com` `/api/compose` returns 404 as of
    /// 2026-05-02. This works against `poly-test-reddit` (which
    /// implements the legacy form-POST endpoint); against live Reddit a
    /// future update will need to switch to the shreddit GraphQL or
    /// OAuth-bearer path. See plan-reddit-stub.md F.2 findings.
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx, `RedditError::LoggedOut`
    /// for a wrong-recipient or unauthenticated reply.
    pub async fn compose_dm(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), RedditError> {
        let url = self.resolve_url("/api/compose");
        let resp = self
            .post_form(
                &url,
                &[
                    ("to", to),
                    ("subject", subject),
                    ("text", body),
                    ("api_type", "json"),
                ],
            )
            .await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        let body: serde_json::Value = resp.json().await?;
        if let Some(errs) = body
            .get("json")
            .and_then(|j| j.get("errors"))
            .and_then(|e| e.as_array())
            && !errs.is_empty()
        {
            return Err(RedditError::LoggedOut);
        }
        Ok(())
    }

    /// Subscribe (or unsubscribe) to a subreddit. `sr_fullname` is the
    /// `t5_<id>` form. `action` is `"sub"` or `"unsub"`.
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx.
    pub async fn subscribe(
        &self,
        sr_fullname: &str,
        action: &str,
    ) -> Result<(), RedditError> {
        let url = self.resolve_url("/api/subscribe");
        let resp = self
            .post_form(
                &url,
                &[
                    ("action", action),
                    ("sr", sr_fullname),
                    ("api_type", "json"),
                ],
            )
            .await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        Ok(())
    }

    /// Reply with a comment under a parent (`t1_` for comment-on-comment,
    /// `t3_` for comment-on-post).
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx.
    pub async fn reply_comment(
        &self,
        parent_fullname: &str,
        text: &str,
    ) -> Result<(), RedditError> {
        let url = self.resolve_url("/api/comment");
        let resp = self
            .post_form(
                &url,
                &[
                    ("thing_id", parent_fullname),
                    ("text", text),
                    ("api_type", "json"),
                ],
            )
            .await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        Ok(())
    }

    /// Vote on a thing. `dir` is `1` (upvote), `0` (clear), or `-1`
    /// (downvote). `id` is the `t3_` or `t1_` fullname.
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx.
    pub async fn vote(&self, fullname: &str, dir: i8) -> Result<(), RedditError> {
        let url = self.resolve_url("/api/vote");
        let dir_str = dir.to_string();
        let resp = self
            .post_form(&url, &[("id", fullname), ("dir", dir_str.as_str())])
            .await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        Ok(())
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
