//! Forgejo / Gitea REST API v1 HTTP client.
//!
//! All requests go through [`poly_host_bridge::http::HttpClient`], which on
//! native targets uses `reqwest` and on wasm32 routes through the host bridge
//! that the native shell exposes.

use poly_client::{ClientError, ClientResult};
use std::sync::{Arc, Mutex};

/// Default User-Agent for Forgejo API requests.
pub const DEFAULT_CLIENT_VERSION: &str = "poly-forgejo/0.0.0";
use poly_host_bridge::http::HttpClient;
use serde::de::DeserializeOwned;

use crate::types::{
    ForgejoComment, ForgejoContentEntry, ForgejoIssue, ForgejoRepo, ForgejoRepoResponse,
    ForgejoUser,
};

/// Map a non-2xx HTTP status onto the [`ClientError`] the host can act on.
///
/// Every request path in this client funnels through here so the contract is
/// identical whatever the verb: an expired token is always `PermissionDenied`
/// (so the host can prompt for a new one), a missing resource is always
/// `NotFound`, a throttle is always `RateLimited` (so the host can back off),
/// and only genuinely unclassified statuses stay `Network`.
///
/// `retry_after` is the raw `Retry-After` header value when present (seconds,
/// per RFC 9110); a missing or unparseable value falls back to 60s.
fn map_status(context: &str, code: u16, retry_after: Option<&str>) -> ClientError {
    match code {
        401 | 403 => ClientError::PermissionDenied(format!(
            "{context} returned HTTP {code} — the access token is missing, \
             expired, or lacks the required scope"
        )),
        404 => ClientError::NotFound(format!("{context} returned HTTP 404")),
        429 => ClientError::RateLimited {
            retry_after_ms: retry_after
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(60)
                .saturating_mul(1_000),
        },
        other => ClientError::Network(format!("{context} returned HTTP {other}")),
    }
}

/// Read the `Retry-After` header off a response, if present and printable.
fn retry_after_header(resp: &poly_host_bridge::http::Response) -> Option<String> {
    resp.headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

/// Low-level Forgejo REST API v1 client.
pub struct ForgejoApi {
    /// Base URL including `/api/v1` (no trailing slash).
    base_url: String,
    http: HttpClient,
    token: Option<String>,
    /// Interior-mutable User-Agent so `set_user_agent` works via `&self`.
    user_agent: Arc<Mutex<String>>,
}

impl ForgejoApi {
    /// Create a new client pointing at `instance_url` (e.g. `https://codeberg.org`).
    ///
    /// The constructor strips a trailing slash and appends `/api/v1`.
    #[must_use] 
    pub fn new(instance_url: &str) -> Self {
        let mut url = instance_url.trim_end_matches('/').to_string();
        url.push_str("/api/v1");
        Self {
            base_url: url,
            http: HttpClient::new(),
            token: None,
            user_agent: Arc::new(Mutex::new(DEFAULT_CLIENT_VERSION.to_string())),
        }
    }

    /// Update the User-Agent string (interior-mutable — callable via `&self`).
    pub fn set_user_agent(&self, ua: String) {
        if let Ok(mut lock) = self.user_agent.lock() {
            *lock = ua;
        }
    }

    /// The current User-Agent string.
    #[must_use] 
    pub fn user_agent(&self) -> String {
        self.user_agent
            .lock()
            .ok().map_or_else(|| DEFAULT_CLIENT_VERSION.to_string(), |g| g.clone())
    }

    fn ua(&self) -> String {
        self.user_agent()
    }

    /// Store a personal access token for authenticated requests.
    pub fn set_token(&mut self, token: String) {
        self.token = Some(token);
    }

    /// Clear any stored token (called on logout).
    pub fn clear_token(&mut self) {
        self.token = None;
    }

    /// The configured base URL (no trailing slash).
    #[must_use] 
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Build the full URL for an API path (e.g. `/user`).
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Start a GET request with the User-Agent and Authorization headers that
    /// EVERY request must carry.
    ///
    /// Building a request by hand instead of going through here is how
    /// `is_starred` silently stopped honouring the client-version override.
    fn get_request(&self, path: &str) -> poly_host_bridge::http::RequestBuilder {
        let mut req = self.http.get(self.url(path)).header("User-Agent", self.ua());
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("token {token}"));
        }
        req
    }

    /// Same as [`Self::get_request`] for DELETE.
    fn delete_request(&self, path: &str) -> poly_host_bridge::http::RequestBuilder {
        let mut req = self
            .http
            .delete(self.url(path))
            .header("User-Agent", self.ua());
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("token {token}"));
        }
        req
    }

    /// Send an authenticated GET request and deserialize the JSON body as `T`.
    async fn get<T: DeserializeOwned>(&self, path: &str) -> ClientResult<T> {
        let resp = self
            .get_request(path)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let retry_after = retry_after_header(&resp);
            return Err(map_status(
                &format!("GET {path}"),
                status,
                retry_after.as_deref(),
            ));
        }

        resp.json::<T>()
            .await
            .map_err(|e| ClientError::Internal(format!("JSON parse error for {path}: {e}")))
    }

    /// `GET /user` — fetch the authenticated user.
    pub async fn get_authenticated_user(&self) -> ClientResult<ForgejoUser> {
        self.get("/user").await
    }

    /// `GET /user/repos?limit=50&sort=updated` — list repos for the authenticated user.
    pub async fn list_user_repos(&self) -> ClientResult<Vec<ForgejoRepo>> {
        self.get("/user/repos?limit=50&sort=updated").await
    }

    /// `GET /repos/{owner}/{repo}/issues?state=all&limit=50&sort=updated&type=issues`
    pub async fn list_repo_issues(
        &self,
        owner: &str,
        repo: &str,
    ) -> ClientResult<Vec<ForgejoIssue>> {
        let path = format!(
            "/repos/{owner}/{repo}/issues?state=all&limit=50&sort=updated&type=issues"
        );
        self.get(&path).await
    }

    /// `GET /repos/{owner}/{repo}/issues?state=all&limit=50&sort=updated&type=pulls`
    pub async fn list_repo_pulls(
        &self,
        owner: &str,
        repo: &str,
    ) -> ClientResult<Vec<ForgejoIssue>> {
        let path = format!(
            "/repos/{owner}/{repo}/issues?state=all&limit=50&sort=updated&type=pulls"
        );
        self.get(&path).await
    }

    /// Paged issues/PRs endpoint — used by `get_view_rows`.
    ///
    /// `state` is `"open"`, `"closed"`, or `"all"`.
    /// `issue_type` is `"issues"` or `"pulls"`.
    /// `page` is 1-based.
    pub async fn list_repo_issues_paged(
        &self,
        owner: &str,
        repo: &str,
        state: &str,
        issue_type: &str,
        page: u32,
    ) -> ClientResult<Vec<ForgejoIssue>> {
        let path = format!(
            "/repos/{owner}/{repo}/issues?state={state}&type={issue_type}&page={page}&limit=30&sort=updated"
        );
        self.get(&path).await
    }

    /// `GET /repos/{owner}/{repo}/issues/{index}` — single issue or PR.
    pub async fn get_issue(
        &self,
        owner: &str,
        repo: &str,
        index: u64,
    ) -> ClientResult<ForgejoIssue> {
        let path = format!("/repos/{owner}/{repo}/issues/{index}");
        self.get(&path).await
    }

    /// `GET /user/starred/{owner}/{repo}` — 204 if starred, 404 if not.
    ///
    /// Returns `Ok(true)` on 204, `Ok(false)` on 404, `Err` on other errors.
    pub async fn is_starred(&self, owner: &str, repo: &str) -> ClientResult<bool> {
        let path = format!("/user/starred/{owner}/{repo}");
        let resp = self
            .get_request(&path)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        // 404 is the documented "not starred" answer here, so it must NOT go
        // through `map_status` (which maps 404 → NotFound).
        match resp.status().as_u16() {
            204 => Ok(true),
            404 => Ok(false),
            code => {
                let retry_after = retry_after_header(&resp);
                Err(map_status(
                    &format!("GET {path}"),
                    code,
                    retry_after.as_deref(),
                ))
            }
        }
    }

    /// `GET /repos/{owner}/{repo}/issues/{number}/comments`
    pub async fn list_issue_comments(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> ClientResult<Vec<ForgejoComment>> {
        let path = format!("/repos/{owner}/{repo}/issues/{number}/comments");
        self.get(&path).await
    }

    /// `GET /repos/{owner}/{repo}/contents/{path}` — directory listing.
    pub async fn get_contents(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
    ) -> ClientResult<Vec<ForgejoContentEntry>> {
        let api_path = if path.is_empty() {
            format!("/repos/{owner}/{repo}/contents")
        } else {
            format!("/repos/{owner}/{repo}/contents/{path}")
        };
        self.get(&api_path).await
    }

    /// `GET /repos/{owner}/{repo}/contents/{path}` — single file.
    pub async fn get_file_content(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
    ) -> ClientResult<ForgejoContentEntry> {
        let api_path = format!("/repos/{owner}/{repo}/contents/{path}");
        self.get(&api_path).await
    }

    /// `GET /repos/{owner}/{repo}` — fetch repo-level permissions for the caller.
    pub async fn get_repo_permissions(
        &self,
        owner: &str,
        repo: &str,
    ) -> ClientResult<ForgejoRepoResponse> {
        let path = format!("/repos/{owner}/{repo}");
        self.get(&path).await
    }

    /// `DELETE /repos/{owner}/{repo}/issues/comments/{id}` — delete one issue comment.
    pub async fn delete_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        comment_id: u64,
    ) -> ClientResult<()> {
        let path = format!("/repos/{owner}/{repo}/issues/comments/{comment_id}");
        let resp = self
            .delete_request(&path)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        match resp.status().as_u16() {
            204 | 200 => Ok(()),
            code => {
                let retry_after = retry_after_header(&resp);
                Err(map_status(
                    &format!("DELETE {path}"),
                    code,
                    retry_after.as_deref(),
                ))
            }
        }
    }
}

#[cfg(test)]
mod status_mapping_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::map_status;
    use poly_client::ClientError;

    /// An expired / revoked token must be distinguishable from a transient
    /// network fault — otherwise the host retries forever instead of asking
    /// the user for a new token.
    #[test]
    fn auth_failures_map_to_permission_denied() {
        assert!(matches!(
            map_status("GET /user/repos", 401, None),
            ClientError::PermissionDenied(_)
        ));
        assert!(matches!(
            map_status("GET /user/repos", 403, None),
            ClientError::PermissionDenied(_)
        ));
    }

    #[test]
    fn missing_resource_maps_to_not_found() {
        assert!(matches!(
            map_status("GET /repos/o/r", 404, None),
            ClientError::NotFound(_)
        ));
    }

    /// The retry budget carried by a `RateLimited`, or `None` for any other
    /// variant (spelled out rather than wildcarded so a new `ClientError`
    /// variant forces a decision here).
    fn retry_ms(err: &ClientError) -> Option<u64> {
        match *err {
            ClientError::RateLimited { retry_after_ms } => Some(retry_after_ms),
            ClientError::AuthFailed(_)
            | ClientError::Network(_)
            | ClientError::NotFound(_)
            | ClientError::PermissionDenied(_)
            | ClientError::Internal(_)
            | ClientError::NotSupported(_) => None,
        }
    }

    #[test]
    fn throttling_maps_to_rate_limited_and_honours_retry_after() {
        assert_eq!(
            retry_ms(&map_status("GET /user/repos", 429, Some("30"))),
            Some(30_000),
            "429 must map to RateLimited with the Retry-After budget"
        );
        // Missing / unparseable Retry-After falls back to 60s.
        assert_eq!(retry_ms(&map_status("GET /user/repos", 429, None)), Some(60_000));
        assert_eq!(
            retry_ms(&map_status(
                "GET /user/repos",
                429,
                Some("Wed, 21 Oct 2026 07:28:00 GMT")
            )),
            Some(60_000),
            "an HTTP-date Retry-After we cannot parse falls back to the default"
        );
    }

    #[test]
    fn unclassified_statuses_stay_network() {
        assert!(matches!(
            map_status("GET /user/repos", 500, None),
            ClientError::Network(_)
        ));
        assert!(matches!(
            map_status("GET /user/repos", 418, None),
            ClientError::Network(_)
        ));
    }
}
