//! Thin wrapper around the `gh` CLI.
//!
//! All requests go through `gh api <endpoint>` (or `gh api -H ... <endpoint>`).
//! No tokens are extracted from `gh` — credentials live in the user's gh
//! configuration.
//!
//! GitHub Enterprise (GHE) is supported by passing `--hostname <host>`.
//!
//! ## Native vs WASM
//!
//! On native targets [`GhCli::api_raw`] runs the `gh` binary directly with
//! [`tokio::process::Command`] — no shell, so argv metacharacters stay inert.
//!
//! On wasm32 (the dioxus web build that runs inside the Wry / Electron shells)
//! [`GhCli::api_raw`] cannot spawn processes, so it routes through the
//! [`poly_host_bridge`] client, which POSTs an `exec-command` [`HostCall`]
//! to the native shell's generic `/host` endpoint. The shell runs `gh` on
//! the user's behalf and returns the exit code + stdout/stderr. The same
//! bridge handles every other host-api operation, so this code is not
//! github-specific. Convenience methods built on top (`auth_status_login`,
//! `list_user_repos`, …) are target-agnostic.
//!
//! [`HostCall`]: poly_host_bridge::HostCall

use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::types::{
    GhContents, GhDiscussion, GhDiscussionsData, GhIssue, GhIssueComment, GhRepo, GhUser,
    GraphQlResponse,
};

/// The authenticated user's permission flags for a single GitHub repo,
/// as returned by `GET /repos/{owner}/{repo}` under the `permissions` key.
// Five bools directly mirror the five GitHub API permission levels; a state
// machine or enum would misrepresent the API (they are independent flags).
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
pub struct RepoPermissions {
    /// Full admin access (repo settings, delete, transfer).
    pub admin: bool,
    /// Maintain-level access (push + triage + manage issues/PRs).
    pub maintain: bool,
    /// Write access (push commits).
    pub push: bool,
    /// Triage access (manage issues without push).
    pub triage: bool,
    /// Read access.
    pub pull: bool,
}

/// All errors the gh CLI wrapper can return.
#[derive(Debug, Error)]
pub enum GhError {
    /// Failed to spawn / reach the `gh` transport (native: subprocess; WASM: bridge).
    #[error("failed to spawn gh CLI: {0}")]
    Spawn(String),
    /// `gh` exited with a non-zero status; the message is the captured stderr.
    #[error("gh exited with code {code}: {stderr}")]
    Exit { code: i32, stderr: String },
    /// JSON parse error from `gh` output.
    #[error("failed to parse gh output: {0}")]
    Parse(String),
}

/// Convert a flat JSON object into `gh api` field arguments.
///
/// * strings → `-f key=value` (always sent as a raw string)
/// * booleans / numbers → `-F key=value` (gh parses these as JSON literals)
/// * `null` → omitted (an absent optional field / GraphQL variable)
/// * arrays / nested objects → [`GhError::Spawn`]
///
/// The bridge exec API passes argv only, so a nested payload genuinely cannot
/// be expressed on this transport. Rejecting it is deliberate: the alternative
/// (silently switching to a hard-coded `https://api.github.com` HTTP POST)
/// sends GitHub Enterprise payloads to the public host, unauthenticated.
#[cfg(any(target_arch = "wasm32", test))]
fn json_object_to_gh_fields(body: &serde_json::Value) -> Result<Vec<String>, GhError> {
    let obj = body.as_object().ok_or_else(|| {
        GhError::Spawn("gh CLI transport: request body must be a JSON object".to_string())
    })?;
    let mut out: Vec<String> = Vec::with_capacity(obj.len().saturating_mul(2));
    for (key, value) in obj {
        match *value {
            serde_json::Value::Null => {}
            serde_json::Value::String(ref s) => {
                out.push("-f".to_string());
                out.push(format!("{key}={s}"));
            }
            serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
                out.push("-F".to_string());
                out.push(format!("{key}={value}"));
            }
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                return Err(GhError::Spawn(format!(
                    "gh CLI transport cannot express nested field '{key}' as an argument; \
                     this request needs a stdin-capable transport"
                )));
            }
        }
    }
    Ok(out)
}

/// Build the argv for `gh api -X POST … <endpoint>`.
#[cfg(any(target_arch = "wasm32", test))]
fn post_argv(
    hostname: Option<&str>,
    endpoint: &str,
    body: &serde_json::Value,
) -> Result<Vec<String>, GhError> {
    let mut args: Vec<String> = vec!["api".to_string(), "-X".to_string(), "POST".to_string()];
    if let Some(host) = hostname {
        args.push("--hostname".to_string());
        args.push(host.to_string());
    }
    args.extend(json_object_to_gh_fields(body)?);
    args.push(endpoint.to_string());
    Ok(args)
}

/// Build the argv for `gh api graphql …` from a `{ query, variables }` body.
///
/// gh treats every field other than `query` as a GraphQL variable.
#[cfg(any(target_arch = "wasm32", test))]
fn graphql_argv(hostname: Option<&str>, body: &serde_json::Value) -> Result<Vec<String>, GhError> {
    let query = body.get("query").and_then(serde_json::Value::as_str).ok_or_else(|| {
        GhError::Spawn("gh CLI transport: GraphQL body must carry a string 'query'".to_string())
    })?;
    let mut args: Vec<String> = vec!["api".to_string(), "graphql".to_string()];
    if let Some(host) = hostname {
        args.push("--hostname".to_string());
        args.push(host.to_string());
    }
    args.push("-f".to_string());
    args.push(format!("query={query}"));
    if let Some(variables) = body.get("variables") {
        args.extend(json_object_to_gh_fields(variables)?);
    }
    Ok(args)
}

/// Map a host-bridge transport failure onto [`GhError`].
#[cfg(target_arch = "wasm32")]
fn map_bridge_error(err: poly_host_bridge::BridgeError) -> GhError {
    use poly_host_bridge::BridgeError;

    match err {
        BridgeError::Unreachable { url, source } => GhError::Spawn(format!(
            "host bridge unreachable at {url}: {source} — this build of Poly \
             needs a native shell (apps/desktop-web, apps/desktop-electron-web, …) \
             to forward gh CLI calls."
        )),
        other @ (BridgeError::Transport(_)
        | BridgeError::ParseResponse(_)
        | BridgeError::Host(_)
        | BridgeError::VariantMismatch { .. }) => GhError::Spawn(other.to_string()),
    }
}

/// Wrapper that runs `gh` subprocesses for a single GitHub instance
/// (either github.com or a GHE hostname).
#[derive(Debug, Clone)]
pub struct GhCli {
    /// Optional GHE hostname (e.g. `"github.example.com"`).
    /// `None` means github.com.
    hostname: Option<String>,
    /// Optional HTTP base URL for testing — when set, uses direct HTTP
    /// instead of spawning the `gh` CLI binary.
    http_base_url: Option<String>,
    /// Optional auth token for HTTP mode.
    http_token: Option<String>,
}

impl GhCli {
    /// Wrap the user's gh CLI for github.com.
    #[must_use]
    pub const fn dotcom() -> Self {
        Self {
            hostname: None,
            http_base_url: None,
            http_token: None,
        }
    }

    /// Wrap the user's gh CLI for a GitHub Enterprise hostname.
    #[must_use]
    pub fn enterprise(hostname: impl Into<String>) -> Self {
        Self {
            hostname: Some(hostname.into()),
            http_base_url: None,
            http_token: None,
        }
    }

    /// Create a client that uses direct HTTP instead of the gh CLI.
    /// Used for testing against mock servers.
    #[must_use]
    pub fn with_http(base_url: impl Into<String>) -> Self {
        Self {
            hostname: None,
            http_base_url: Some(base_url.into()),
            http_token: None,
        }
    }

    /// Set the auth token for HTTP mode.
    pub fn set_token(&mut self, token: String) {
        self.http_token = Some(token);
    }

    /// Clear the auth token.
    pub fn clear_token(&mut self) {
        self.http_token = None;
    }

    /// Display name for the instance (used as `instance_id` and in errors).
    #[must_use]
    pub fn instance_id(&self) -> &str {
        self.http_base_url.as_ref().map_or_else(
            || self.hostname.as_deref().unwrap_or("github.com"),
            |url| url.trim_start_matches("http://")
                .trim_start_matches("https://")
                .split('/')
                .next()
                .unwrap_or("localhost"),
        )
    }

    /// Run `gh api <endpoint>` and parse the JSON output as `T`.
    ///
    /// Extra args are appended after the endpoint (e.g. pagination flags).
    pub async fn api_get<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        extra_args: &[&str],
    ) -> Result<T, GhError> {
        let bytes = self.api_raw(endpoint, extra_args).await?;
        serde_json::from_slice(&bytes).map_err(|e| GhError::Parse(e.to_string()))
    }

    /// Native: run `gh api <endpoint>` as a subprocess and return raw stdout.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn api_raw(&self, endpoint: &str, extra_args: &[&str]) -> Result<Vec<u8>, GhError> {
        use std::process::Stdio;
        use tokio::process::Command;

        // If HTTP mode is configured, use direct HTTP instead of gh CLI
        if let Some(base_url) = &self.http_base_url {
            return self.api_raw_http(base_url, endpoint).await;
        }

        let mut cmd = Command::new("gh");
        cmd.arg("api");
        if let Some(host) = &self.hostname {
            cmd.arg("--hostname").arg(host);
        }
        cmd.arg(endpoint);
        for a in extra_args {
            cmd.arg(a);
        }
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd
            .output()
            .await
            .map_err(|e| GhError::Spawn(e.to_string()))?;

        if !output.status.success() {
            return Err(GhError::Exit {
                code: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        Ok(output.stdout)
    }

    /// WASM: route the `gh` invocation through the abstract host bridge that
    /// the native shell (Wry / Electron / future iOS / Android) exposes at
    /// `/host`. We send a single [`HostCall::ExecCommand`] and the shell runs
    /// the binary on our behalf. No GitHub-specific protocol shape — every
    /// host-api operation goes through the same endpoint.
    #[cfg(target_arch = "wasm32")]
    pub async fn api_raw(&self, endpoint: &str, extra_args: &[&str]) -> Result<Vec<u8>, GhError> {
        // If HTTP mode is configured, use direct HTTP instead of the host bridge
        if let Some(base_url) = &self.http_base_url {
            return self.api_raw_http(base_url, endpoint).await;
        }

        use poly_host_bridge::{BridgeError, Client};

        let mut args: Vec<String> = Vec::with_capacity(4_usize.saturating_add(extra_args.len()));
        args.push("api".to_string());
        if let Some(host) = &self.hostname {
            args.push("--hostname".to_string());
            args.push(host.clone());
        }
        args.push(endpoint.to_string());
        for a in extra_args {
            args.push((*a).to_string());
        }

        let client = Client::new();
        let (exit_code, stdout, stderr) = client.exec("gh", args).await.map_err(|e| match e {
            BridgeError::Unreachable { url, source } => GhError::Spawn(format!(
                "host bridge unreachable at {url}: {source} — this build of Poly \
                 needs a native shell (apps/desktop-web, apps/desktop-electron-web, …) \
                 to forward gh CLI calls."
            )),
            other @ (BridgeError::Transport(_)
            | BridgeError::ParseResponse(_)
            | BridgeError::Host(_)
            | BridgeError::VariantMismatch { .. }) => GhError::Spawn(other.to_string()),
        })?;

        if exit_code != 0_i32 {
            return Err(GhError::Exit {
                code: exit_code,
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
            });
        }
        Ok(stdout)
    }

    /// HTTP-based transport for testing. Sends GET requests directly to the mock server.
    async fn api_raw_http(&self, base_url: &str, endpoint: &str) -> Result<Vec<u8>, GhError> {
        use poly_host_bridge::http::HttpClient;

        let url = format!("{base_url}{endpoint}");
        let http = HttpClient::new();
        let mut req = http.get(&url);
        if let Some(token) = &self.http_token {
            req = req.header("Authorization", format!("token {token}"));
        }
        let response = req
            .send()
            .await
            .map_err(|e| GhError::Spawn(format!("HTTP request failed: {e}")))?;

        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| GhError::Parse(format!("failed to read response body: {e}")))?;

        if !status.is_success() {
            return Err(GhError::Exit {
                code: i32::from(status.as_u16()),
                stderr: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }

        Ok(bytes.to_vec())
    }

    /// Check whether the user is authenticated against this instance.
    /// Returns the authenticated login on success.
    pub async fn auth_status_login(&self) -> Result<String, GhError> {
        // `gh api user` is the simplest auth check — it returns the
        // authenticated user as JSON, or fails if not logged in.
        let user: GhUser = self.api_get("/user", &[]).await?;
        Ok(user.login)
    }

    /// List repos the authenticated user owns or collaborates on.
    ///
    /// `affiliation=owner,collaborator` filters out org repos the user only
    /// has read access to via team membership; the caller can additionally
    /// filter on `pushed_at` to drop stale repos.
    pub async fn list_user_repos(&self) -> Result<Vec<GhRepo>, GhError> {
        // 100 per page is the API max; the gh CLI handles auth headers.
        // For users with >100 repos this would need pagination — left as
        // a follow-up since most accounts fit in one page.
        self.api_get(
            "/user/repos?affiliation=owner,collaborator&per_page=100&sort=pushed",
            &[],
        )
        .await
    }

    /// List issues + PRs in a repo (open + recent closed).
    pub async fn list_repo_issues(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<GhIssue>, GhError> {
        let endpoint = format!("/repos/{owner}/{repo}/issues?state=all&per_page=50&sort=updated");
        self.api_get(&endpoint, &[]).await
    }

    /// List comments on a single issue / PR.
    pub async fn list_issue_comments(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Vec<GhIssueComment>, GhError> {
        let endpoint = format!("/repos/{owner}/{repo}/issues/{number}/comments?per_page=100");
        self.api_get(&endpoint, &[]).await
    }

    /// Fetch a single issue or PR by number.
    pub async fn get_issue(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<GhIssue, GhError> {
        let endpoint = format!("/repos/{owner}/{repo}/issues/{number}");
        self.api_get(&endpoint, &[]).await
    }

    /// Create a comment on an issue or PR by number.
    ///
    /// Uses `POST /repos/{owner}/{repo}/issues/{number}/comments` with body
    /// `{ "body": text }`. Returns the created comment.
    pub async fn create_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        text: &str,
    ) -> Result<GhIssueComment, GhError> {
        let endpoint = format!("/repos/{owner}/{repo}/issues/{number}/comments");
        let body = serde_json::json!({ "body": text });
        self.api_post(&endpoint, body).await
    }

    /// Check whether the authenticated user has starred a repo.
    ///
    /// Returns `Ok(true)` on 204, `Ok(false)` on 404.
    /// Any other error is propagated.
    pub async fn is_starred(&self, owner: &str, repo: &str) -> Result<bool, GhError> {
        let endpoint = format!("/user/starred/{owner}/{repo}");
        match self.api_raw(&endpoint, &[]).await {
            Ok(_) => Ok(true),
            Err(GhError::Exit { code: 404_i32, .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Fetch the contents of a path in a repo at HEAD of the default branch.
    /// Returns either a directory listing or a single file payload.
    pub async fn get_contents(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
    ) -> Result<GhContents, GhError> {
        let endpoint = if path.is_empty() {
            format!("/repos/{owner}/{repo}/contents")
        } else {
            format!("/repos/{owner}/{repo}/contents/{path}")
        };
        self.api_get(&endpoint, &[]).await
    }

    /// Execute a GraphQL query against `POST /graphql` (github.com or GHE).
    ///
    /// Body: `{ "query": query, "variables": variables }`.
    /// Response shape: `{ "data": T, "errors": [...] }`.
    ///
    /// If the response contains one or more `errors`, the first message is
    /// returned as [`GhError::Exit`] (code = 0 to distinguish from HTTP errors).
    /// If `data` is absent and there are no errors, an internal parse error is
    /// returned.
    pub async fn graphql_query<T: DeserializeOwned>(
        &self,
        query: &str,
        variables: serde_json::Value,
    ) -> Result<T, GhError> {
        let body = serde_json::json!({ "query": query, "variables": variables });
        let bytes = self.graphql_raw(&body).await?;
        let envelope: GraphQlResponse<T> =
            serde_json::from_slice(&bytes).map_err(|e| GhError::Parse(e.to_string()))?;
        if !envelope.errors.is_empty() {
            let msg = envelope
                .errors
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(GhError::Exit {
                code: 0,
                stderr: format!("GraphQL errors: {msg}"),
            });
        }
        envelope.data.ok_or_else(|| {
            GhError::Parse("GraphQL response missing 'data' field".to_string())
        })
    }

    /// Native: run `gh api graphql --method POST --input -` with the JSON body
    /// piped to stdin, or use direct HTTP when `http_base_url` is set.
    #[cfg(not(target_arch = "wasm32"))]
    async fn graphql_raw(&self, body: &serde_json::Value) -> Result<Vec<u8>, GhError> {
        use std::process::Stdio;
        use tokio::io::AsyncWriteExt as _;
        use tokio::process::Command;

        if let Some(base_url) = &self.http_base_url {
            return self.graphql_raw_http(base_url, body).await;
        }

        let body_bytes =
            serde_json::to_vec(body).map_err(|e| GhError::Parse(e.to_string()))?;

        let mut cmd = Command::new("gh");
        cmd.arg("api").arg("graphql").arg("--method").arg("POST").arg("--input").arg("-");
        if let Some(host) = &self.hostname {
            cmd.args(["--hostname", host]);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| GhError::Spawn(e.to_string()))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&body_bytes)
                .await
                .map_err(|e| GhError::Spawn(format!("failed to write stdin: {e}")))?;
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| GhError::Spawn(e.to_string()))?;

        if !output.status.success() {
            return Err(GhError::Exit {
                code: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        Ok(output.stdout)
    }

    /// WASM: route the GraphQL call through the host bridge exec transport so
    /// `--hostname` (GitHub Enterprise) and gh's own stored credentials are
    /// honoured. Only the explicit test-mode `http_base_url` uses direct HTTP —
    /// we never silently retarget a GHE request at github.com.
    #[cfg(target_arch = "wasm32")]
    async fn graphql_raw(&self, body: &serde_json::Value) -> Result<Vec<u8>, GhError> {
        if let Some(base_url) = &self.http_base_url {
            return self.graphql_raw_http(base_url, body).await;
        }
        let args = graphql_argv(self.hostname.as_deref(), body)?;
        self.exec_gh(args).await
    }

    /// Shared HTTP POST path for GraphQL (test mode on native, default on WASM).
    async fn graphql_raw_http(
        &self,
        base_url: &str,
        body: &serde_json::Value,
    ) -> Result<Vec<u8>, GhError> {
        use poly_host_bridge::http::HttpClient;

        let url = format!("{}/graphql", base_url.trim_end_matches('/'));
        let http = HttpClient::new();
        let mut req = http
            .post(&url)
            .header("Accept", "application/vnd.github+json")
            .json(body);
        if let Some(token) = &self.http_token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        let response = req
            .send()
            .await
            .map_err(|e| GhError::Spawn(format!("HTTP request failed: {e}")))?;

        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| GhError::Parse(format!("failed to read response body: {e}")))?;

        if !status.is_success() {
            return Err(GhError::Exit {
                code: i32::from(status.as_u16()),
                stderr: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }

        Ok(bytes.to_vec())
    }

    /// POST a JSON body to `endpoint` and parse the response as `T`.
    ///
    /// Used for creating issue comments:
    /// `POST /repos/{owner}/{repo}/issues/{number}/comments`
    /// with body `{ "body": "..." }`.
    ///
    /// On native: runs `gh api -X POST -f body=<text> <endpoint>`.
    /// On WASM: routes through the host bridge exec transport.
    /// In HTTP test mode: sends a direct HTTP POST.
    pub async fn api_post<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        body: serde_json::Value,
    ) -> Result<T, GhError> {
        let bytes = self.api_post_raw(endpoint, body).await?;
        serde_json::from_slice(&bytes).map_err(|e| GhError::Parse(e.to_string()))
    }

    /// Native: run `gh api -X POST --input - <endpoint>` with JSON body on stdin.
    #[cfg(not(target_arch = "wasm32"))]
    async fn api_post_raw(
        &self,
        endpoint: &str,
        body: serde_json::Value,
    ) -> Result<Vec<u8>, GhError> {
        use std::process::Stdio;
        use tokio::io::AsyncWriteExt as _;
        use tokio::process::Command;

        if let Some(base_url) = &self.http_base_url {
            return self.api_post_raw_http(base_url, endpoint, body).await;
        }

        let body_bytes = serde_json::to_vec(&body).map_err(|e| GhError::Parse(e.to_string()))?;

        let mut cmd = Command::new("gh");
        cmd.arg("api").arg("-X").arg("POST").arg("--input").arg("-");
        if let Some(host) = &self.hostname {
            cmd.arg("--hostname").arg(host);
        }
        cmd.arg(endpoint);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| GhError::Spawn(e.to_string()))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&body_bytes)
                .await
                .map_err(|e| GhError::Spawn(format!("failed to write stdin: {e}")))?;
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| GhError::Spawn(e.to_string()))?;
        if !output.status.success() {
            return Err(GhError::Exit {
                code: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(output.stdout)
    }

    /// WASM: route through the host bridge exec transport, same pattern as `api_raw`.
    ///
    /// The bridge exec API passes argv only (no stdin), so the JSON body is
    /// expressed as `gh api` field arguments — see [`json_object_to_gh_fields`].
    /// This keeps `--hostname` and gh's own credentials in play; the previous
    /// implementation dropped both and POSTed unauthenticated to
    /// `api.github.com`, which leaked GitHub Enterprise payloads to github.com.
    #[cfg(target_arch = "wasm32")]
    async fn api_post_raw(
        &self,
        endpoint: &str,
        body: serde_json::Value,
    ) -> Result<Vec<u8>, GhError> {
        if let Some(base_url) = &self.http_base_url {
            return self.api_post_raw_http(base_url, endpoint, body).await;
        }

        let args = post_argv(self.hostname.as_deref(), endpoint, &body)?;
        self.exec_gh(args).await
    }

    /// Run `gh <args>` through the host bridge and return stdout.
    #[cfg(target_arch = "wasm32")]
    async fn exec_gh(&self, args: Vec<String>) -> Result<Vec<u8>, GhError> {
        use poly_host_bridge::Client;

        let client = Client::new();
        let (exit_code, stdout, stderr) =
            client.exec("gh", args).await.map_err(map_bridge_error)?;
        if exit_code != 0_i32 {
            return Err(GhError::Exit {
                code: exit_code,
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
            });
        }
        Ok(stdout)
    }

    /// HTTP POST transport (test mode on native, default on WASM for POST).
    async fn api_post_raw_http(
        &self,
        base_url: &str,
        endpoint: &str,
        body: serde_json::Value,
    ) -> Result<Vec<u8>, GhError> {
        use poly_host_bridge::http::HttpClient;

        let url = format!("{base_url}{endpoint}");
        let http = HttpClient::new();
        let mut req = http
            .post(&url)
            .header("Accept", "application/vnd.github+json")
            .json(&body);
        if let Some(token) = &self.http_token {
            req = req.header("Authorization", format!("token {token}"));
        }
        let response = req
            .send()
            .await
            .map_err(|e| GhError::Spawn(format!("HTTP POST failed: {e}")))?;

        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| GhError::Parse(format!("failed to read response body: {e}")))?;

        if !status.is_success() {
            return Err(GhError::Exit {
                code: i32::from(status.as_u16()),
                stderr: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        Ok(bytes.to_vec())
    }

    /// Send an HTTP DELETE to `endpoint` (no response body expected).
    ///
    /// Returns `Ok(())` on 204 No Content or any 2xx status.
    /// Used for deleting issue comments and PR review comments.
    pub async fn api_delete(&self, endpoint: &str) -> Result<(), GhError> {
        // In HTTP mode route through the HTTP transport.
        if let Some(base_url) = &self.http_base_url {
            return self.api_delete_http(base_url, endpoint).await;
        }
        // In CLI mode we cannot use `api_raw` (which calls `gh api` without a
        // method flag) — pass `-X DELETE` explicitly.
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::process::Stdio;
            use tokio::process::Command;

            let mut cmd = Command::new("gh");
            cmd.arg("api").arg("-X").arg("DELETE");
            if let Some(host) = &self.hostname {
                cmd.arg("--hostname").arg(host);
            }
            cmd.arg(endpoint);
            cmd.stdin(Stdio::null());
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());

            let output = cmd.output().await.map_err(|e| GhError::Spawn(e.to_string()))?;
            if !output.status.success() {
                return Err(GhError::Exit {
                    code: output.status.code().unwrap_or(-1),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                });
            }
            Ok(())
        }
        #[cfg(target_arch = "wasm32")]
        {
            use poly_host_bridge::{BridgeError, Client};
            let mut args: Vec<String> = vec!["api".into(), "-X".into(), "DELETE".into()];
            if let Some(host) = &self.hostname {
                args.push("--hostname".into());
                args.push(host.clone());
            }
            args.push(endpoint.to_string());
            let client = Client::new();
            let (exit_code, _stdout, stderr) =
                client.exec("gh", args).await.map_err(|e| match e {
                    BridgeError::Unreachable { url, source } => GhError::Spawn(format!(
                        "host bridge unreachable at {url}: {source}"
                    )),
                    other @ (BridgeError::Transport(_)
                    | BridgeError::ParseResponse(_)
                    | BridgeError::Host(_)
                    | BridgeError::VariantMismatch { .. }) => GhError::Spawn(other.to_string()),
                })?;
            if exit_code != 0_i32 {
                return Err(GhError::Exit {
                    code: exit_code,
                    stderr: String::from_utf8_lossy(&stderr).into_owned(),
                });
            }
            Ok(())
        }
    }

    /// HTTP DELETE transport used in test mode.
    async fn api_delete_http(&self, base_url: &str, endpoint: &str) -> Result<(), GhError> {
        use poly_host_bridge::http::HttpClient;

        let url = format!("{base_url}{endpoint}");
        let http = HttpClient::new();
        let mut req = http.delete(&url);
        if let Some(token) = &self.http_token {
            req = req.header("Authorization", format!("token {token}"));
        }
        let response = req
            .send()
            .await
            .map_err(|e| GhError::Spawn(format!("HTTP DELETE failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let bytes = response
                .bytes()
                .await
                .unwrap_or_default();
            return Err(GhError::Exit {
                code: i32::from(status.as_u16()),
                stderr: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        Ok(())
    }

    /// Fetch the authenticated user's permission flags for a repo.
    ///
    /// Calls `GET /repos/{owner}/{repo}` and returns the `permissions` sub-object.
    /// Returns a tuple `(admin, push, pull, maintain, triage)`.
    pub async fn get_repo_permissions(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<RepoPermissions, GhError> {
        let endpoint = format!("/repos/{owner}/{repo}");
        let v: serde_json::Value = self.api_get(&endpoint, &[]).await?;
        let p = v.get("permissions").and_then(|p| p.as_object());
        let bool_field = |obj: Option<&serde_json::Map<String, serde_json::Value>>, key: &str| {
            obj.and_then(|o| o.get(key))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        };
        Ok(RepoPermissions {
            admin: bool_field(p, "admin"),
            maintain: bool_field(p, "maintain"),
            push: bool_field(p, "push"),
            triage: bool_field(p, "triage"),
            pull: bool_field(p, "pull"),
        })
    }

    /// List GitHub Discussions for a repo, ordered by last-updated descending.
    ///
    /// `first` controls page size (max 100 per GitHub's GraphQL limits).
    /// `after` is the pagination cursor from the previous page's `pageInfo.endCursor`.
    ///
    /// Returns `(discussions, next_cursor)` where `next_cursor` is `Some` when
    /// `pageInfo.hasNextPage` is true.
    pub async fn list_discussions(
        &self,
        owner: &str,
        repo: &str,
        first: u32,
        after: Option<&str>,
    ) -> Result<(Vec<GhDiscussion>, Option<String>), GhError> {
        const QUERY: &str = r"
query($owner: String!, $name: String!, $first: Int!, $after: String) {
  repository(owner: $owner, name: $name) {
    discussions(first: $first, after: $after, orderBy: {field: UPDATED_AT, direction: DESC}) {
      pageInfo { endCursor hasNextPage }
      nodes {
        number
        title
        bodyText
        url
        createdAt
        updatedAt
        upvoteCount
        comments(first: 0) { totalCount }
        author { login avatarUrl }
        category { id name emoji }
        answerChosenAt
        closed
      }
    }
  }
}
";
        let variables = serde_json::json!({
            "owner": owner,
            "name": repo,
            "first": first,
            "after": after,
        });
        let data: GhDiscussionsData = self.graphql_query(QUERY, variables).await?;
        let conn = data.repository.discussions;
        let next_cursor = if conn.page_info.has_next_page {
            conn.page_info.end_cursor
        } else {
            None
        };
        let discussions = conn.nodes.into_iter().flatten().collect();
        Ok((discussions, next_cursor))
    }
}

#[cfg(test)]
mod transport_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::{graphql_argv, json_object_to_gh_fields, post_argv, GhError};

    /// The GHE hostname MUST reach the argv — the old wasm POST path dropped it
    /// and hard-coded `https://api.github.com`, leaking enterprise payloads.
    #[test]
    fn post_argv_threads_enterprise_hostname_and_body_field() {
        let body = serde_json::json!({ "body": "internal issue text" });
        let args = post_argv(
            Some("github.acme-internal.com"),
            "/repos/o/r/issues/7/comments",
            &body,
        )
        .expect("a flat string body is expressible as gh fields");

        assert_eq!(
            args,
            vec![
                "api".to_string(),
                "-X".to_string(),
                "POST".to_string(),
                "--hostname".to_string(),
                "github.acme-internal.com".to_string(),
                "-f".to_string(),
                "body=internal issue text".to_string(),
                "/repos/o/r/issues/7/comments".to_string(),
            ]
        );
    }

    #[test]
    fn post_argv_without_hostname_omits_the_flag() {
        let body = serde_json::json!({ "body": "hi" });
        let args = post_argv(None, "/repos/o/r/issues/1/comments", &body).unwrap();
        assert!(
            !args.iter().any(|a| a == "--hostname"),
            "dotcom must not pass --hostname: {args:?}"
        );
        assert_eq!(args.last().map(String::as_str), Some("/repos/o/r/issues/1/comments"));
    }

    /// A payload the argv transport cannot express must fail loudly rather than
    /// be silently retargeted at a different host.
    #[test]
    fn nested_body_is_rejected_not_retargeted() {
        let body = serde_json::json!({ "labels": ["bug", "p1"] });
        let err = post_argv(None, "/repos/o/r/issues/1/labels", &body).unwrap_err();
        assert!(
            matches!(err, GhError::Spawn(_)),
            "nested payloads must be rejected: {err:?}"
        );
    }

    #[test]
    fn graphql_argv_threads_hostname_types_variables_and_omits_nulls() {
        let body = serde_json::json!({
            "query": "query($owner: String!) { x }",
            "variables": {
                "owner": "acme",
                "first": 25_u32,
                "after": serde_json::Value::Null,
            },
        });
        let args = graphql_argv(Some("github.acme-internal.com"), &body).unwrap();

        let pos = args
            .iter()
            .position(|a| a == "--hostname")
            .expect("GHE hostname must be threaded into the graphql argv");
        assert_eq!(
            args.get(pos.saturating_add(1)).map(String::as_str),
            Some("github.acme-internal.com")
        );
        assert!(args.contains(&"query=query($owner: String!) { x }".to_string()));
        assert!(args.contains(&"owner=acme".to_string()));
        assert!(
            args.contains(&"first=25".to_string()),
            "numeric variables use -F so gh sends them as JSON numbers: {args:?}"
        );
        assert!(
            !args.iter().any(|a| a.starts_with("after=")),
            "null variables must be omitted, not sent as the string \"null\": {args:?}"
        );
    }

    #[test]
    fn fields_use_raw_string_flag_for_strings_and_typed_flag_for_scalars() {
        let fields = json_object_to_gh_fields(&serde_json::json!({ "a": "1" })).unwrap();
        assert_eq!(fields, vec!["-f".to_string(), "a=1".to_string()]);

        let fields = json_object_to_gh_fields(&serde_json::json!({ "a": true })).unwrap();
        assert_eq!(fields, vec!["-F".to_string(), "a=true".to_string()]);
    }
}
