//! Convert GitHub JSON shapes into Poly client types.
//!
//! Channel ID conventions used inside the github backend:
//!
//! | Channel kind            | ID format                          |
//! |-------------------------|-------------------------------------|
//! | Issues forum (per repo) | `gh-issues-{owner}-{repo}`         |
//! | Pull requests forum     | `gh-pulls-{owner}-{repo}`          |
//! | Code explorer           | `gh-code-{owner}-{repo}`           |
//! | Single issue thread     | `gh-issue-{owner}-{repo}-{number}` |
//!
//! Server IDs are the GitHub numeric repo ID prefixed with `gh-` so they
//! are stable across renames.

use chrono::{DateTime, Utc};
use poly_client::{
    Category, Channel, ChannelType, Message, MessageContent, PresenceStatus, Server, User,
    BackendType,
};

use crate::types::{GhIssue, GhIssueComment, GhRepo, GhUser};

/// Backend slug used in `BackendType` and routes.
pub const BACKEND_SLUG: &str = "github";

/// Build a stable server ID for a repo.
#[must_use]
pub fn server_id_for_repo(repo: &GhRepo) -> String {
    format!("gh-{}", repo.id)
}

/// Channel ID for the per-repo issues forum.
#[must_use]
pub fn issues_channel_id(owner: &str, repo: &str) -> String {
    format!("gh-issues-{owner}-{repo}")
}

/// Channel ID for the per-repo pull requests forum.
#[must_use]
pub fn pulls_channel_id(owner: &str, repo: &str) -> String {
    format!("gh-pulls-{owner}-{repo}")
}

/// Channel ID for the per-repo code explorer.
#[must_use]
pub fn code_channel_id(owner: &str, repo: &str) -> String {
    format!("gh-code-{owner}-{repo}")
}

/// Channel ID for a single issue/PR comment thread.
///
/// Currently constructed inline by the message-fetch path; kept exported so
/// the future thread-open routing can use it.
#[must_use]
pub fn issue_thread_channel_id(owner: &str, repo: &str, number: u64) -> String {
    format!("gh-issue-{owner}-{repo}-{number}")
}

/// Try to parse `(owner, repo)` out of a code-channel ID.
#[must_use]
pub fn parse_code_channel(channel_id: &str) -> Option<(String, String)> {
    let rest = channel_id.strip_prefix("gh-code-")?;
    let (owner, repo) = rest.split_once('-')?;
    Some((owner.to_string(), repo.to_string()))
}

/// Convert a [`GhUser`] into a Poly [`User`].
#[must_use]
pub fn user_from_gh(u: &GhUser) -> User {
    User {
        id: u.login.clone(),
        display_name: u.login.clone(),
        avatar_url: u.avatar_url.clone(),
        presence: PresenceStatus::Offline,
        backend: BackendType::from(BACKEND_SLUG),
    }
}

/// Convert a [`GhRepo`] into a Poly [`Server`].
///
/// `account_id` and `account_display_name` come from the authenticated session.
#[must_use]
pub fn server_from_repo(repo: &GhRepo, account_id: &str, account_display_name: &str) -> Server {
    let (owner, name) = split_full_name(&repo.full_name);
    let issues_id = issues_channel_id(&owner, &name);
    let pulls_id = pulls_channel_id(&owner, &name);
    let code_id = code_channel_id(&owner, &name);

    Server {
        id: server_id_for_repo(repo),
        name: repo.full_name.clone(),
        icon_url: repo.owner.avatar_url.clone(),
        banner_url: None,
        categories: vec![
            Category {
                id: format!("{}-discussion", server_id_for_repo(repo)),
                name: "Discussion".to_string(),
                channel_ids: vec![issues_id, pulls_id],
            },
            Category {
                id: format!("{}-source", server_id_for_repo(repo)),
                name: "Source".to_string(),
                channel_ids: vec![code_id],
            },
        ],
        backend: BackendType::from(BACKEND_SLUG),
        unread_count: 0,
        mention_count: 0,
        account_id: account_id.to_string(),
        account_display_name: account_display_name.to_string(),
    }
}

/// Build the full channel list for a repo (issues forum, PR forum, code explorer).
#[must_use]
pub fn channels_for_repo(repo: &GhRepo) -> Vec<Channel> {
    let (owner, name) = split_full_name(&repo.full_name);
    let server_id = server_id_for_repo(repo);
    vec![
        Channel {
            id: issues_channel_id(&owner, &name),
            name: "issues".to_string(),
            channel_type: ChannelType::Forum,
            server_id: server_id.clone(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
        },
        Channel {
            id: pulls_channel_id(&owner, &name),
            name: "pull-requests".to_string(),
            channel_type: ChannelType::Forum,
            server_id: server_id.clone(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
        },
        Channel {
            id: code_channel_id(&owner, &name),
            name: "code".to_string(),
            channel_type: ChannelType::Code,
            server_id,
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
        },
    ]
}

/// Convert one issue/PR into a Forum-style top-level [`Message`].
///
/// The body is rendered as plain text; the URL is appended so the UI can
/// link out to GitHub for the full markdown rendering and reactions.
#[must_use]
pub fn issue_to_message(issue: &GhIssue) -> Message {
    let kind = if issue.is_pull_request() { "PR" } else { "Issue" };
    let body = issue.body.clone().unwrap_or_default();
    let text = format!(
        "**[{} #{}] {}** ({})\n\n{}\n\n{}",
        kind, issue.number, issue.title, issue.state, body, issue.html_url
    );
    Message {
        id: format!("gh-issue-msg-{}", issue.id),
        author: user_from_gh(&issue.user),
        content: MessageContent::Text(text),
        timestamp: parse_ts(&issue.created_at),
        attachments: Vec::new(),
        reactions: Vec::new(),
        reply_to: None,
        edited: false,
    }
}

/// Convert an issue/PR comment into a [`Message`] inside the thread channel.
#[must_use]
pub fn comment_to_message(c: &GhIssueComment) -> Message {
    Message {
        id: format!("gh-comment-{}", c.id),
        author: user_from_gh(&c.user),
        content: MessageContent::Text(c.body.clone().unwrap_or_default()),
        timestamp: parse_ts(&c.created_at),
        attachments: Vec::new(),
        reactions: Vec::new(),
        reply_to: None,
        edited: false,
    }
}

/// Split `"owner/name"` into its two halves; falls back to `(slug, slug)`
/// if the slash is missing (defensive — the API guarantees the format).
#[must_use]
pub fn split_full_name(full_name: &str) -> (String, String) {
    if let Some((o, n)) = full_name.split_once('/') {
        (o.to_string(), n.to_string())
    } else {
        (full_name.to_string(), full_name.to_string())
    }
}

fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// Filter a slice of repos to those with `pushed_at` within the last `years`.
///
/// Repos with no `pushed_at` (rare) are kept.
#[must_use]
pub fn filter_active_repos(repos: Vec<GhRepo>, years: i64) -> Vec<GhRepo> {
    let cutoff = Utc::now() - chrono::Duration::days(365 * years);
    repos
        .into_iter()
        .filter(|r| match &r.pushed_at {
            None => true,
            Some(s) => DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc) >= cutoff)
                .unwrap_or(true),
        })
        .filter(|r| !r.archived)
        .collect()
}
