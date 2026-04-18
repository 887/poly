//! Internal GitHub JSON shapes returned by the `gh api` CLI.
//!
//! These mirror the GitHub REST API v3 response structure for the
//! endpoints we hit. Only the fields the mapping layer reads are kept;
//! everything else is allowed to deserialize and be discarded.

use serde::Deserialize;

/// One repository as returned by `gh api /user/repos` / `gh api /user/repos?type=...`.
#[derive(Debug, Clone, Deserialize)]
pub struct GhRepo {
    /// Numeric repo ID, used as the Poly server `id` for stability across renames.
    pub id: u64,
    /// `"owner/name"` format slug.
    pub full_name: String,
    /// Repo display name (the `name` half of `full_name`).
    pub name: String,
    /// Markdown / plain-text description, may be missing.
    #[serde(default)]
    pub description: Option<String>,
    /// Owner subobject (login + avatar URL).
    pub owner: GhUser,
    /// Whether the repo is private — informational only, used for UI hints.
    #[serde(default)]
    pub private: bool,
    /// Whether the repo is archived.
    #[serde(default)]
    pub archived: bool,
    /// `pushed_at` timestamp from the API — used to filter inactive repos.
    #[serde(default)]
    pub pushed_at: Option<String>,
    /// Default branch name (e.g. `"main"`); needed for the code explorer.
    #[serde(default)]
    pub default_branch: Option<String>,
    /// Web URL of the repo (used for the external code-search link).
    pub html_url: String,
}

/// A GitHub user / org as embedded inside a repo or issue.
#[derive(Debug, Clone, Deserialize)]
pub struct GhUser {
    pub id: u64,
    pub login: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

/// Reaction counts embedded in an issue response.
///
/// GitHub returns `reactions` only when the API request accepts
/// `application/vnd.github+json`. We deserialise the sub-object
/// optionally so that mock responses without it still round-trip.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GhReactions {
    #[serde(rename = "total_count", default)]
    pub total_count: i64,
}

/// One issue or pull request as returned by `gh api /repos/{owner}/{repo}/issues`.
///
/// PRs are issues in the GitHub data model — they appear in the same listing
/// and are distinguished by the presence of `pull_request`.
#[derive(Debug, Clone, Deserialize)]
pub struct GhIssue {
    pub id: u64,
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    pub user: GhUser,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    pub html_url: String,
    /// Present iff this issue is actually a PR.
    #[serde(default)]
    pub pull_request: Option<serde_json::Value>,
    #[serde(default)]
    pub comments: u32,
    /// Reaction counts (optional — absent in some API contexts).
    #[serde(default)]
    pub reactions: GhReactions,
}

impl GhIssue {
    /// Whether this issue is actually a pull request.
    pub fn is_pull_request(&self) -> bool {
        self.pull_request.is_some()
    }
}

/// A comment on an issue or PR.
#[derive(Debug, Clone, Deserialize)]
pub struct GhIssueComment {
    pub id: u64,
    pub user: GhUser,
    #[serde(default)]
    pub body: Option<String>,
    pub created_at: String,
    pub html_url: String,
}

/// One file/dir entry from the contents API
/// (`gh api /repos/{owner}/{repo}/contents/{path}`).
#[derive(Debug, Clone, Deserialize)]
pub struct GhContentEntry {
    pub name: String,
    pub path: String,
    /// `"file"`, `"dir"`, `"symlink"`, or `"submodule"`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub size: u64,
    /// Base64-encoded file content (only present for `type == "file"` when
    /// fetched as a single entry, not a directory listing).
    #[serde(default)]
    pub content: Option<String>,
    /// `"base64"` when present.
    #[serde(default)]
    pub encoding: Option<String>,
}

/// Either a directory listing or a single file response from the contents API.
///
/// `gh api /repos/{owner}/{repo}/contents/{path}` returns a JSON array for
/// directories and a single object for files. We deserialize untagged.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum GhContents {
    Dir(Vec<GhContentEntry>),
    File(GhContentEntry),
}
