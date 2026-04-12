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

use crate::types::{GhContents, GhIssue, GhIssueComment, GhRepo, GhUser};

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
    pub fn dotcom() -> Self {
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
        if let Some(url) = &self.http_base_url {
            url.trim_start_matches("http://")
                .trim_start_matches("https://")
                .split('/')
                .next()
                .unwrap_or("localhost")
        } else {
            self.hostname.as_deref().unwrap_or("github.com")
        }
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
        // If HTTP mode is configured, use direct HTTP instead of gh CLI
        if let Some(base_url) = &self.http_base_url {
            return self.api_raw_http(base_url, endpoint).await;
        }

        use std::process::Stdio;
        use tokio::process::Command;

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

        let mut args: Vec<String> = Vec::with_capacity(4 + extra_args.len());
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
            other => GhError::Spawn(other.to_string()),
        })?;

        if exit_code != 0 {
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

        let url = format!("{}{}", base_url, endpoint);
        let http = HttpClient::new();
        let mut req = http.get(&url);
        if let Some(token) = &self.http_token {
            req = req.header("Authorization", format!("token {}", token));
        }
        let response = req
            .send()
            .await
            .map_err(|e| GhError::Spawn(format!("HTTP request failed: {}", e)))?;

        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| GhError::Parse(format!("failed to read response body: {}", e)))?;

        if status != 200 {
            return Err(GhError::Exit {
                code: status.as_u16() as i32,
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
}
