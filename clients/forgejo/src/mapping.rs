//! Convert Forgejo API types into Poly client types.
//!
//! Channel ID conventions used inside the forgejo backend:
//!
//! | Channel kind            | ID format                           |
//! |-------------------------|--------------------------------------|
//! | Issues forum (per repo) | `fj-issues-{owner}-{repo}`          |
//! | Pull requests forum     | `fj-pulls-{owner}-{repo}`           |
//! | Code explorer           | `fj-code-{owner}-{repo}`            |
//! | Single issue thread     | `fj-issue-{owner}-{repo}-{number}`  |
//!
//! Server IDs are the Forgejo numeric repo ID prefixed with `fj-` so they
//! are stable across renames.

use chrono::{DateTime, Utc};
use poly_client::{
    BackendType, Category, Channel, ChannelType, Message, MessageContent, PresenceStatus, Server,
    User,
};

use crate::types::{ForgejoComment, ForgejoIssue, ForgejoRepo, ForgejoUser};

/// Backend slug used in `BackendType` and routes.
pub const BACKEND_SLUG: &str = "forgejo";

/// Build a stable server ID for a repo.
#[must_use]
pub fn server_id_for_repo(repo: &ForgejoRepo) -> String {
    format!("fj-{}", repo.id)
}

/// Channel ID for the per-repo issues forum.
#[must_use]
pub fn issues_channel_id(owner: &str, repo: &str) -> String {
    format!("fj-issues-{owner}-{repo}")
}

/// Channel ID for the per-repo pull requests forum.
#[must_use]
pub fn pulls_channel_id(owner: &str, repo: &str) -> String {
    format!("fj-pulls-{owner}-{repo}")
}

/// Channel ID for the per-repo code explorer.
#[must_use]
pub fn code_channel_id(owner: &str, repo: &str) -> String {
    format!("fj-code-{owner}-{repo}")
}

/// Channel ID for a single issue/PR comment thread.
#[must_use]
#[allow(dead_code)]
pub fn issue_thread_channel_id(owner: &str, repo: &str, number: u64) -> String {
    format!("fj-issue-{owner}-{repo}-{number}")
}

/// Try to parse `(owner, repo)` out of a code-channel ID (`fj-code-{owner}-{repo}`).
#[must_use]
pub fn parse_code_channel(channel_id: &str) -> Option<(String, String)> {
    let rest = channel_id.strip_prefix("fj-code-")?;
    let (owner, repo) = rest.split_once('-')?;
    Some((owner.to_string(), repo.to_string()))
}

/// Convert a [`ForgejoUser`] into a Poly [`User`].
#[must_use]
pub fn user_from_fj(u: &ForgejoUser) -> User {
    User {
        id: u.login.clone(),
        display_name: u.full_name.clone().unwrap_or_else(|| u.login.clone()),
        avatar_url: u.avatar_url.clone(),
        presence: PresenceStatus::Offline,
        backend: BackendType::from(BACKEND_SLUG),
    }
}

/// Convert a [`ForgejoRepo`] into a Poly [`Server`].
#[must_use]
pub fn server_from_repo(repo: &ForgejoRepo, account_id: &str, account_display_name: &str) -> Server {
    let (owner, name) = split_full_name(&repo.full_name);
    let issues_id = issues_channel_id(&owner, &name);
    let pulls_id = pulls_channel_id(&owner, &name);
    let code_id = code_channel_id(&owner, &name);
    let server_id = server_id_for_repo(repo);

    Server {
        id: server_id.clone(),
        name: repo.full_name.clone(),
        icon_url: repo.owner.avatar_url.clone(),
        banner_url: None,
        categories: vec![
            Category {
                id: format!("{server_id}-discussion"),
                name: "Discussion".to_string(),
                channel_ids: vec![issues_id, pulls_id],
            },
            Category {
                id: format!("{server_id}-source"),
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
pub fn channels_for_repo(repo: &ForgejoRepo) -> Vec<Channel> {
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
#[must_use]
pub fn issue_to_message(issue: &ForgejoIssue) -> Message {
    let kind = if issue.is_pull_request() { "PR" } else { "Issue" };
    let body = issue.body.clone().unwrap_or_default();
    let text = format!(
        "**[{} #{}] {}** ({})\n\n{}\n\n{}",
        kind, issue.number, issue.title, issue.state, body, issue.html_url
    );
    Message {
        id: format!("fj-issue-msg-{}", issue.id),
        author: user_from_fj(&issue.user),
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
pub fn comment_to_message(c: &ForgejoComment) -> Message {
    Message {
        id: format!("fj-comment-{}", c.id),
        author: user_from_fj(&c.user),
        content: MessageContent::Text(c.body.clone().unwrap_or_default()),
        timestamp: parse_ts(&c.created_at),
        attachments: Vec::new(),
        reactions: Vec::new(),
        reply_to: None,
        edited: false,
    }
}

/// Split `"owner/name"` into its two halves.
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

/// Filter a slice of repos to those with `updated_at` within the last 2 years
/// and not archived.
#[must_use]
pub fn filter_active_repos(repos: Vec<ForgejoRepo>) -> Vec<ForgejoRepo> {
    let cutoff = Utc::now() - chrono::Duration::days(365 * 2);
    repos
        .into_iter()
        .filter(|r| {
            if r.archived {
                return false;
            }
            match &r.updated_at {
                None => true,
                Some(s) => DateTime::parse_from_rfc3339(s)
                    .map(|dt| dt.with_timezone(&Utc) >= cutoff)
                    .unwrap_or(true),
            }
        })
        .collect()
}
