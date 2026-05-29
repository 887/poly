//! Low-level Lemmy HTTP client: connection state, auth/session storage,
//! and shared header/URL helpers.
//!
//! Endpoint methods live in `endpoints.rs`. This module owns the struct
//! definition + the small `impl` block of private/protocol helpers.

use poly_client::{ClientError, ClientResult};
use poly_host_bridge::http::HttpClient;
use std::sync::{Arc, RwLock};

use super::types::DEFAULT_CLIENT_VERSION;

/// Stored session state for the Lemmy HTTP client.
#[derive(Debug, Clone)]
pub struct LemmySession {
    /// Bearer JWT.
    pub jwt: String,
    /// Authenticated user's integer ID (from `/api/v3/site`).
    pub user_id: i64,
    /// Authenticated user's display name.
    pub user_display_name: String,
    /// Authenticated user's avatar URL.
    pub user_avatar_url: Option<String>,
}

/// Low-level Lemmy REST API client.
pub struct LemmyHttpClient {
    base_url: String,
    http: HttpClient,
    session: Arc<RwLock<Option<LemmySession>>>,
    user_agent: Arc<RwLock<String>>,
}

impl LemmyHttpClient {
    /// Create a new client pointing at `base_url` (e.g. `https://lemmy.ml`).
    pub fn new(base_url: impl Into<String>) -> Self {
        let mut url = base_url.into();
        // Strip trailing slash so we can always append `/api/v3/...`
        if url.ends_with('/') {
            url.pop();
        }
        Self {
            base_url: url,
            http: HttpClient::new(),
            session: Arc::new(RwLock::new(None)),
            user_agent: Arc::new(RwLock::new(DEFAULT_CLIENT_VERSION.to_string())),
        }
    }

    /// The configured base URL (no trailing slash).
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Whether a session JWT is currently stored.
    pub fn is_authenticated(&self) -> bool {
        self.session
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    /// Retrieve the stored session, if any.
    pub fn session(&self) -> Option<LemmySession> {
        self.session
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Store a session JWT after successful login.
    pub fn set_session(&self, session: LemmySession) {
        *self.session.write().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(session);
    }

    /// Clear the stored session.
    pub fn clear_session(&self) {
        *self.session.write().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }

    /// Update the User-Agent string.
    pub fn set_user_agent(&self, ua: String) {
        if let Ok(mut guard) = self.user_agent.write() {
            *guard = ua;
        }
    }

    /// Borrow the underlying HTTP client — used by `endpoints.rs`.
    pub(super) const fn raw_http(&self) -> &HttpClient {
        &self.http
    }

    /// Current User-Agent string (cloned for header injection).
    pub(super) fn ua(&self) -> String {
        self.user_agent
            .read()
            .ok().map_or_else(|| DEFAULT_CLIENT_VERSION.to_string(), |g| g.clone())
    }

    /// Build an absolute URL for an API path (e.g. `/api/v3/site`).
    pub(super) fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Return the current JWT or an `AuthFailed` error.
    pub(super) fn jwt(&self) -> ClientResult<String> {
        self.session()
            .map(|s| s.jwt)
            .ok_or_else(|| ClientError::AuthFailed("Lemmy client is not authenticated".to_string()))
    }

    /// POST with UA header injected.
    ///
    /// Currently unused — kept for upcoming UA-aware routes.
    // lint-allow-unused: helper kept for upcoming UA-aware routes
    #[allow(dead_code)]
    pub(super) async fn http_post<B: serde::Serialize + Sync, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        auth: Option<&str>,
    ) -> ClientResult<T> {
        let mut req = self
            .http
            .post(self.url(path))
            .header("User-Agent", self.ua())
            .json(body);
        if let Some(jwt) = auth {
            req = req.header("Authorization", format!("Bearer {jwt}"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            return Err(ClientError::Network(format!("{path} returned HTTP {status}")));
        }
        resp.json::<T>().await.map_err(|e| ClientError::Internal(e.to_string()))
    }

    /// GET with UA header injected.
    ///
    /// Currently unused — kept for upcoming UA-aware routes.
    // lint-allow-unused: helper kept for upcoming UA-aware routes
    #[allow(dead_code)]
    pub(super) async fn http_get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        auth: Option<&str>,
    ) -> ClientResult<T> {
        let mut req = self
            .http
            .get(self.url(path))
            .header("User-Agent", self.ua());
        if let Some(jwt) = auth {
            req = req.header("Authorization", format!("Bearer {jwt}"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            return Err(ClientError::Network(format!("{path} returned HTTP {status}")));
        }
        resp.json::<T>().await.map_err(|e| ClientError::Internal(e.to_string()))
    }
}
