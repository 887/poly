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
    pub fn user_agent(&self) -> String {
        self.user_agent
            .lock()
            .ok()
            .map(|g| g.clone())
            .unwrap_or_else(|| DEFAULT_CLIENT_VERSION.to_string())
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
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Build the full URL for an API path (e.g. `/user`).
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Send an authenticated GET request and deserialize the JSON body as `T`.
    async fn get<T: DeserializeOwned>(&self, path: &str) -> ClientResult<T> {
        let url = self.url(path);
        let mut req = self.http.get(url).header("User-Agent", self.ua());
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("token {token}"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ClientError::Network(format!(
                "GET {path} returned HTTP {}",
                resp.status()
            )));
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
        let url = self.url(&path);
        let mut req = self.http.get(url);
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("token {token}"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            204 => Ok(true),
            404 => Ok(false),
            code => Err(ClientError::Network(format!(
                "GET /user/starred returned HTTP {code}"
            ))),
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
        let url = self.url(&format!(
            "/repos/{owner}/{repo}/issues/comments/{comment_id}"
        ));
        let mut req = self.http.delete(url);
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("token {token}"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        match resp.status().as_u16() {
            204 | 200 => Ok(()),
            401 | 403 => Err(ClientError::PermissionDenied(
                "delete_issue_comment: not authorized".to_string(),
            )),
            404 => Err(ClientError::NotFound(format!(
                "comment {comment_id} not found"
            ))),
            code => Err(ClientError::Network(format!(
                "DELETE /repos/{owner}/{repo}/issues/comments/{comment_id} returned HTTP {code}"
            ))),
        }
    }
}
