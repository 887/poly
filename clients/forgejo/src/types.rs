//! Forgejo / Gitea REST API v1 response types.
//!
//! These mirror the Forgejo REST API v1 response structure for the
//! endpoints we hit. Only the fields the mapping layer reads are kept;
//! everything else is allowed to deserialize and be discarded.

use serde::Deserialize;

/// One repository as returned by `GET /api/v1/user/repos`.
#[derive(Debug, Clone, Deserialize)]
pub struct ForgejoRepo {
    /// Numeric repo ID, stable across renames.
    pub id: u64,
    /// `"owner/name"` format slug.
    pub full_name: String,
    /// Repo display name (the `name` half of `full_name`).
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Owner subobject.
    pub owner: ForgejoUser,
    /// Whether the repo is private.
    #[serde(default)]
    pub private: bool,
    /// Whether the repo is archived.
    #[serde(default)]
    pub archived: bool,
    /// Last update timestamp (ISO 8601).
    #[serde(default)]
    pub updated_at: Option<String>,
    /// Default branch name (e.g. `"main"`).
    #[serde(default)]
    pub default_branch: Option<String>,
    /// Web URL of the repo.
    pub html_url: String,
    /// Number of stars the repo has received.
    #[serde(default)]
    pub stars_count: u64,
    /// Number of forks.
    #[serde(default)]
    pub forks_count: u64,
    /// Number of open issues (issues only, not PRs).
    #[serde(default)]
    pub open_issues_count: u64,
}

/// A Forgejo user / org as embedded inside a repo or issue.
#[derive(Debug, Clone, Deserialize)]
pub struct ForgejoUser {
    pub id: u64,
    pub login: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub full_name: Option<String>,
}

/// One issue or pull request as returned by `GET /api/v1/repos/{owner}/{repo}/issues`.
///
/// Both issues and PRs are returned by the same endpoint; the `pull_request`
/// field is present when the item is a PR.
#[derive(Debug, Clone, Deserialize)]
pub struct ForgejoIssue {
    pub id: u64,
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    pub user: ForgejoUser,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    pub html_url: String,
    #[serde(default)]
    pub comments: u32,
    /// Present iff this issue is actually a PR.
    #[serde(default)]
    pub pull_request: Option<serde_json::Value>,
}

impl ForgejoIssue {
    /// Whether this issue is actually a pull request.
    pub fn is_pull_request(&self) -> bool {
        self.pull_request.is_some()
    }
}

/// A comment on an issue or PR.
#[derive(Debug, Clone, Deserialize)]
pub struct ForgejoComment {
    pub id: u64,
    pub user: ForgejoUser,
    #[serde(default)]
    pub body: Option<String>,
    pub created_at: String,
    pub html_url: String,
}

/// Repo-level permissions for the authenticated user, as returned by
/// `GET /api/v1/repos/{owner}/{repo}` inside the `permissions` object.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ForgejoRepoPermissions {
    /// User has admin access to the repo.
    #[serde(default)]
    pub admin: bool,
    /// User has push (write) access.
    #[serde(default)]
    pub push: bool,
    /// User has pull (read) access.
    #[serde(default)]
    pub pull: bool,
}

/// Minimal repo response from `GET /api/v1/repos/{owner}/{repo}` — only the
/// `permissions` block is needed for `get_my_permissions`.
#[derive(Debug, Clone, Deserialize)]
pub struct ForgejoRepoResponse {
    #[serde(default)]
    pub permissions: ForgejoRepoPermissions,
}

/// One file/dir entry from the contents API
/// (`GET /api/v1/repos/{owner}/{repo}/contents/{path}`).
#[derive(Debug, Clone, Deserialize)]
pub struct ForgejoContentEntry {
    pub name: String,
    pub path: String,
    /// `"file"`, `"dir"`, `"symlink"`, or `"submodule"`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub size: u64,
    /// Base64-encoded file content (only present for single file fetch).
    #[serde(default)]
    pub content: Option<String>,
    /// `"base64"` when present.
    #[serde(default)]
    pub encoding: Option<String>,
}
