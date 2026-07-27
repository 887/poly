//! [`RedditClient`] — HTTP transport core for the Reddit scraper.
//!
//! Domain-specific endpoint methods live in sibling modules
//! ([`auth`], [`read`], [`write`]) and attach to the same struct via
//! additional `impl` blocks. This module keeps only the struct
//! definition, constructors, and the session-cookie / fetch helpers
//! they all share.
//!
//! Split layout introduced in SOLID-audit-reddit B.5.

use crate::RedditError;

mod auth;
mod read;
mod write;

/// Reddit HTML-scraping client.
///
/// Holds the HTTP client and the manually-tracked `reddit_session` cookie.
/// We track the cookie manually instead of using `reqwest`'s `cookies`
/// feature because that feature pulls in `cookie_store` → `getrandom 0.3`
/// which can't compile cleanly on `wasm32-unknown-unknown` even with the
/// `wasm_js` feature unification dance.
pub struct RedditClient {
    pub(super) http: reqwest::Client,
    /// Scraping base — production: `https://old.reddit.com`.
    /// Test backend (Phase F) overrides via `REDDIT_BASE_URL` env to
    /// `http://127.0.0.1:9108`.
    pub(super) base_url: String,
    /// `reddit_session` cookie value, set by login. Sent as a Cookie
    /// header on every request.
    pub(super) session_cookie: std::sync::Mutex<Option<String>>,
}

impl RedditClient {
    /// Default User-Agent. Reddit aggressively rate-limits the bare
    /// `curl/` and `python-requests/` UAs; spoofing a real browser is
    /// the standard mitigation. Live capture confirmed this exact UA
    /// works against `old.reddit.com` (2026-05-02).
    pub(super) const DEFAULT_UA: &'static str =
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
    pub(super) fn with_session_cookie(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
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
    pub(super) fn capture_session_cookie(&self, resp: &reqwest::Response) {
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
    pub const fn http(&self) -> &reqwest::Client {
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
    pub(super) fn resolve_url(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{base}/{path}")
    }

    /// Fetch a page and return its body, mapping non-2xx → `RedditError`.
    pub(super) async fn fetch_text(&self, url: &str) -> Result<String, RedditError> {
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
    pub(super) async fn post_form(
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
}

/// Percent-encode a URL query-parameter VALUE.
///
/// RFC 3986 unreserved set (`A-Za-z0-9-._~`) passes through; every other byte
/// is escaped as `%XX`. Escaping the whole complement rather than a hand-picked
/// list is load-bearing: the previous version left `+` and `=` untouched, so a
/// query like `c++` decoded server-side as `c  ` (two spaces) and a
/// base64-ish `after` cursor containing `+`/`=` advanced to the wrong page.
///
/// No dependency on a percent-encoding crate — this must build on WASM.
#[must_use]
pub fn urlencoding_simple(s: &str) -> String {
    use std::fmt::Write as _;

    // Worst case every byte expands to three characters.
    let mut out = String::with_capacity(s.len().saturating_mul(3));
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push('%');
            write!(out, "{b:02X}").ok();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn resolve_url_joins_cleanly() {
        let c = RedditClient::with_base_url("https://old.reddit.com/".to_string()).unwrap();
        assert_eq!(c.resolve_url("/r/rust/hot/"), "https://old.reddit.com/r/rust/hot/");
        assert_eq!(c.resolve_url("r/rust/hot/"), "https://old.reddit.com/r/rust/hot/");

        let c2 = RedditClient::with_base_url("http://127.0.0.1:9108".to_string()).unwrap();
        assert_eq!(c2.resolve_url("/api/me.json"), "http://127.0.0.1:9108/api/me.json");
    }

    /// `+` and `=` MUST be escaped: a raw `+` decodes server-side as a space
    /// (so a search for `c++` became `c  `), and a raw `=` can be misread as a
    /// parameter boundary in a base64-ish `after` cursor.
    #[test]
    fn urlencoding_escapes_plus_and_equals() {
        assert_eq!(urlencoding_simple("c++"), "c%2B%2B");
        assert_eq!(urlencoding_simple("dGVzdA=="), "dGVzdA%3D%3D");
        assert_eq!(urlencoding_simple("a+b=c"), "a%2Bb%3Dc");
    }

    #[test]
    fn urlencoding_keeps_the_unreserved_set_and_escapes_everything_else() {
        assert_eq!(
            urlencoding_simple("aZ09-._~"),
            "aZ09-._~",
            "the RFC 3986 unreserved set must pass through untouched"
        );
        assert_eq!(urlencoding_simple("a b"), "a%20b");
        assert_eq!(urlencoding_simple("a&b?c#d%e"), "a%26b%3Fc%23d%25e");
        assert_eq!(urlencoding_simple("t3_abc123"), "t3_abc123", "cursors are unchanged");
        // Non-ASCII is percent-encoded per UTF-8 byte.
        assert_eq!(urlencoding_simple("ü"), "%C3%BC");
    }

    #[test]
    fn default_ua_is_a_real_browser_string() {
        // Sanity — if we ever swap to "poly-reddit/x.y" by accident,
        // Reddit will rate-limit. This guards the regression.
        assert!(RedditClient::DEFAULT_UA.contains("Firefox"));
        assert!(RedditClient::DEFAULT_UA.contains("Mozilla"));
    }
}
