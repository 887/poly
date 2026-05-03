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
    pub fn as_path(self) -> &'static str {
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
    pub fn time_filter(self) -> Option<&'static str> {
        match self {
            Self::TopHour => Some("hour"),
            Self::TopDay => Some("day"),
            Self::TopWeek => Some("week"),
            Self::TopMonth => Some("month"),
            Self::TopYear => Some("year"),
            Self::TopAll => Some("all"),
            _ => None,
        }
    }
}

/// Reddit HTML-scraping client.
///
/// Holds the HTTP client and the manually-tracked `reddit_session` cookie.
/// We track the cookie manually instead of using `reqwest`'s `cookies`
/// feature because that feature pulls in `cookie_store` → `getrandom 0.3`
/// which can't compile cleanly on `wasm32-unknown-unknown` even with the
/// `wasm_js` feature unification dance.
#[cfg(feature = "native")]
pub struct RedditClient {
    http: reqwest::Client,
    /// Scraping base — production: `https://old.reddit.com`.
    /// Test backend (Phase F) overrides via `REDDIT_BASE_URL` env to
    /// `http://127.0.0.1:9108`.
    base_url: String,
    /// `reddit_session` cookie value, set by login. Sent as a Cookie
    /// header on every request.
    session_cookie: std::sync::Mutex<Option<String>>,
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
            .user_agent(Self::DEFAULT_UA)
            .build()?;
        Ok(Self {
            http,
            base_url,
            session_cookie: std::sync::Mutex::new(None),
        })
    }

    /// Apply the stored `reddit_session` cookie (if any) as a Cookie
    /// header on a request builder.
    fn with_session_cookie(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let cookie = self
            .session_cookie
            .lock()
            .ok()
            .and_then(|g| g.clone());
        match cookie {
            // Use a custom header instead of Cookie so the browser doesn't
            // strip it on WASM cross-origin fetch (Cookie is in the
            // forbidden-header list for fetch). The test server reads
            // either header. Real old.reddit.com only honours the cookie,
            // but production-mode poly-reddit can be flipped back to
            // Cookie via a build-time cfg if needed.
            Some(value) => req
                .header("X-Mock-Session", value.as_str())
                .header(reqwest::header::COOKIE, format!("reddit_session={value}")),
            None => req,
        }
    }

    /// Extract the `reddit_session` value from a response's Set-Cookie
    /// headers, store it in the session field if present.
    fn capture_session_cookie(&self, resp: &reqwest::Response) {
        for raw in resp.headers().get_all(reqwest::header::SET_COOKIE) {
            if let Ok(s) = raw.to_str() {
                for pair in s.split(';') {
                    let pair = pair.trim();
                    if let Some(value) = pair.strip_prefix("reddit_session=") {
                        if let Ok(mut g) = self.session_cookie.lock() {
                            *g = Some(value.to_string());
                        }
                        return;
                    }
                }
            }
        }
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

    /// The current session cookie value, if logged in. Used by the
    /// `RedditBackend` to populate `Session.token` so that
    /// `restore_account → authenticate(Token(...))` round-trips the real
    /// session value (rather than the bare username, which the test
    /// server doesn't recognise as a session).
    #[must_use]
    pub fn session_cookie_value(&self) -> Option<String> {
        self.session_cookie.lock().ok().and_then(|g| g.clone())
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
        let resp = self.with_session_cookie(self.http.get(url)).send().await?;
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
        let req = self
            .http
            .post(url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body);
        let resp = self.with_session_cookie(req).send().await?;
        self.capture_session_cookie(&resp);
        Ok(resp)
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
        let base = self.resolve_url(&path);
        let url = match sort.time_filter() {
            Some(t) => format!("{base}?t={t}"),
            None => base,
        };
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

    /// Fetch the gallery image URLs for a multi-image post.
    ///
    /// Reddit's gallery layout is delivered via `/comments/<id>/.json`,
    /// not the HTML page — `gallery_data.items` ordered list keyed into
    /// `media_metadata.<media_id>.s.u` (the source URL). HTML-entity
    /// encoded URLs in the JSON are decoded.
    ///
    /// Returns `Ok(Vec::new())` for non-gallery posts; the caller can
    /// check `RawPost.is_gallery` first to avoid the round-trip.
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx, `RedditError::Http` for
    /// transport-level failures, `RedditError::Parse` if the JSON shape
    /// is unrecognisable.
    pub async fn get_gallery_urls(&self, post_id: &str) -> Result<Vec<String>, RedditError> {
        let path = format!("/comments/{post_id}/.json");
        let url = self.resolve_url(&path);
        let resp = self.with_session_cookie(self.http.get(&url)).send().await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        let json: serde_json::Value = resp.json().await?;
        Ok(parser::post::parse_gallery_metadata(&json)?)
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

    /// Fetch the subscribed subreddit slugs for the authenticated user.
    ///
    /// Uses the JSON endpoint (`/subreddits/mine/subscriber/.json`) instead
    /// of the HTML page because Reddit hides subs from new accounts in
    /// HTML for anti-spam reasons (see plan-reddit-stub.md F.2 findings).
    /// JSON shows them immediately.
    ///
    /// Sends the session cookie (manual `X-Mock-Session` + `Cookie`
    /// headers) — anonymous callers get an empty list, not an error.
    ///
    /// # Errors
    ///
    /// `RedditError::Status` on 5xx; `RedditError::Http` on transport.
    pub async fn list_subscribed_subreddits(&self) -> Result<Vec<SubredditInfo>, RedditError> {
        let url = self.resolve_url("/subreddits/mine/subscriber/.json");
        let resp = self.with_session_cookie(self.http.get(&url)).send().await?;
        if !resp.status().is_success() {
            // Anonymous users get 401/403 — return empty list rather than error.
            return Ok(Vec::new());
        }
        let body: serde_json::Value = resp.json().await?;
        let children = body
            .get("data")
            .and_then(|d| d.get("children"))
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(children
            .iter()
            .filter_map(|c| {
                let data = c.get("data")?;
                let name = data.get("display_name").and_then(|n| n.as_str())?;
                // Reddit emits two icon fields and either may be empty:
                //   `community_icon` is the new-style large round icon
                //   `icon_img` is the legacy uploaded image
                // Prefer community_icon when populated, fall back to icon_img.
                // Reddit also HTML-entity-encodes `&` in the URLs (`&amp;`)
                // — decode so reqwest / browser fetch parses the URL.
                let icon_url = ["community_icon", "icon_img"]
                    .iter()
                    .find_map(|field| {
                        data.get(*field)
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.replace("&amp;", "&"))
                    });
                Some(SubredditInfo {
                    name: name.to_string(),
                    icon_url,
                })
            })
            .collect())
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
        // Set-Cookie is captured into self.session_cookie by
        // `capture_session_cookie` (called from post_form) — works on
        // native. WASM browser fetch hides Set-Cookie from JS though, so
        // also pull the session out of the JSON body, which the test
        // server includes at `json.data.cookie`.
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
        if let Some(cookie_value) = body
            .get("json")
            .and_then(|j| j.get("data"))
            .and_then(|d| d.get("cookie"))
            .and_then(|c| c.as_str())
            && let Ok(mut g) = self.session_cookie.lock()
        {
            *g = Some(cookie_value.to_string());
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
    pub fn login_with_session_cookie(&self, cookie_value: &str) -> Result<(), RedditError> {
        if let Ok(mut g) = self.session_cookie.lock() {
            *g = Some(cookie_value.to_string());
        }
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
        let resp = self
            .with_session_cookie(self.http.get(&url))
            .send()
            .await?;
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

    // ── Phase E: community search ───────────────────────────────────────

    /// Search subreddits by keyword.
    ///
    /// Returns a page of matching [`SubredditInfo`] items and an optional
    /// `after` cursor for the next page (Reddit's standard pagination token).
    ///
    /// # Errors
    ///
    /// `RedditError::Status` for HTTP non-2xx, `RedditError::Http` for
    /// transport errors, `RedditError::Json` for unparseable responses.
    pub async fn search_subreddits(
        &self,
        query: &str,
        after: Option<&str>,
    ) -> Result<(Vec<SubredditInfo>, Option<String>), RedditError> {
        let encoded_q = urlencoding_simple(query);
        let mut path = format!("/subreddits/search.json?q={encoded_q}&limit=25");
        if let Some(cursor) = after {
            let encoded_after = urlencoding_simple(cursor);
            path.push_str(&format!("&after={encoded_after}"));
        }
        let url = self.resolve_url(&path);
        let resp = self.with_session_cookie(self.http.get(&url)).send().await?;
        if !resp.status().is_success() {
            return Err(RedditError::Status(resp.status().as_u16()));
        }
        let body: serde_json::Value = resp.json().await?;
        let children = body
            .get("data")
            .and_then(|d| d.get("children"))
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();
        let next_after = body
            .get("data")
            .and_then(|d| d.get("after"))
            .and_then(|a| a.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let subs = children
            .iter()
            .filter_map(|c| {
                let data = c.get("data")?;
                let name = data.get("display_name").and_then(|n| n.as_str())?;
                let icon_url = ["community_icon", "icon_img"]
                    .iter()
                    .find_map(|field| {
                        data.get(*field)
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.replace("&amp;", "&"))
                    });
                Some(SubredditInfo {
                    name: name.to_string(),
                    icon_url,
                })
            })
            .collect();
        Ok((subs, next_after))
    }
}

/// Percent-encode characters that would break URL query parameter values.
///
/// Only encodes `&`, `?`, `#`, `%`, and space — enough for query strings
/// without a full-blown percent-encoding library dependency on WASM.
fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            ' ' => out.push('+'),
            '&' => out.push_str("%26"),
            '?' => out.push_str("%3F"),
            '#' => out.push_str("%23"),
            '%' => out.push_str("%25"),
            c => out.push(c),
        }
    }
    out
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
