//! Authentication endpoints for [`super::RedditClient`].
//!
//! Carved out in SOLID-audit-reddit B.5. See the parent module for the
//! struct and shared transport helpers.

use super::RedditClient;
use crate::RedditError;

impl RedditClient {
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
}
