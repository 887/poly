//! Thin wrapper around the `gh` CLI.
//!
//! All requests go through `gh api <endpoint>` (or `gh api -H ... <endpoint>`).
//! No tokens are extracted from `gh` — credentials live in the user's gh
//! configuration. We only call the CLI directly with `tokio::process::Command`,
//! so there is no shell involved and shell metacharacters in argv are inert.
//!
//! GitHub Enterprise (GHE) is supported by passing `--hostname <host>`.

use std::process::Stdio;

use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::process::Command;

use crate::types::{GhContents, GhIssue, GhIssueComment, GhRepo, GhUser};

/// All errors the gh CLI wrapper can return.
#[derive(Debug, Error)]
pub enum GhError {
    /// Failed to spawn the `gh` binary (not installed / not on PATH).
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
}

impl GhCli {
    /// Wrap the user's gh CLI for github.com.
    #[must_use]
    pub fn dotcom() -> Self {
        Self { hostname: None }
    }

    /// Wrap the user's gh CLI for a GitHub Enterprise hostname.
    #[must_use]
    pub fn enterprise(hostname: impl Into<String>) -> Self {
        Self {
            hostname: Some(hostname.into()),
        }
    }

    /// Display name for the instance (used as `instance_id` and in errors).
    #[must_use]
    pub fn instance_id(&self) -> &str {
        self.hostname.as_deref().unwrap_or("github.com")
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

    /// Run `gh api <endpoint>` and return the raw stdout bytes.
    pub async fn api_raw(&self, endpoint: &str, extra_args: &[&str]) -> Result<Vec<u8>, GhError> {
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
        let endpoint = format!(
            "/repos/{owner}/{repo}/issues?state=all&per_page=50&sort=updated"
        );
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
