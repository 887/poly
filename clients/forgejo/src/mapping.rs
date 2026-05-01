//! Convert Forgejo API types into Poly client types.
//!
//! Channel ID conventions used inside the forgejo backend:
//!
//! | Channel kind            | ID format                            |
//! |-------------------------|---------------------------------------|
//! | Issues forum (per repo) | `fj-issues-{owner}~{repo}`           |
//! | Pull requests forum     | `fj-pulls-{owner}~{repo}`            |
//! | Code explorer           | `fj-code-{owner}~{repo}`             |
//! | Single issue thread     | `fj-issue-{owner}~{repo}-{number}`   |
//!
//! The `{owner}/{repo}` portion preserves the Forgejo slash separator so that
//! owner and repo names containing hyphens round-trip unambiguously.
//!
//! Server IDs are the Forgejo numeric repo ID prefixed with `fj-` so they
//! are stable across renames.

use chrono::{DateTime, Utc};
use poly_client::{
    BackendType, Category, Channel, ChannelType, CustomBlock, MenuTargetKind, Message,
    MessageContent, PresenceStatus, Server, TreeSpec, User, ViewDetail, ViewRow,
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
///
/// Uses `owner/repo` as the separator so that hyphenated owner or repo
/// names round-trip unambiguously.
#[must_use]
pub fn issues_channel_id(owner: &str, repo: &str) -> String {
    format!("fj-issues-{owner}~{repo}")
}

/// Channel ID for the per-repo pull requests forum.
#[must_use]
pub fn pulls_channel_id(owner: &str, repo: &str) -> String {
    format!("fj-pulls-{owner}~{repo}")
}

/// Channel ID for the per-repo code explorer.
#[must_use]
pub fn code_channel_id(owner: &str, repo: &str) -> String {
    format!("fj-code-{owner}~{repo}")
}

/// Channel ID for a single issue/PR comment thread.
#[must_use]
pub fn issue_thread_channel_id(owner: &str, repo: &str, number: u64) -> String {
    format!("fj-issue-{owner}~{repo}-{number}")
}

/// Try to parse `(owner, repo)` out of a code-channel ID (`fj-code-{owner}~{repo}`).
#[must_use]
pub fn parse_code_channel(channel_id: &str) -> Option<(String, String)> {
    let rest = channel_id.strip_prefix("fj-code-")?;
    let (owner, repo) = rest.split_once('~')?;
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
        default_channel_id: None,
        description: None,
        star_count: None,
        language: None,
        forks_count: None,
        open_issues_count: None,
    }
}

/// Channel ID for the per-repo discussions forum.
#[must_use]
pub fn discussions_channel_id(owner: &str, repo: &str) -> String {
    format!("fj-discussions-{owner}~{repo}")
}

/// Build the full channel list for a repo:
/// issues forum, PR forum, discussions forum, code explorer.
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
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
        },
        Channel {
            id: pulls_channel_id(&owner, &name),
            name: "pull-requests".to_string(),
            channel_type: ChannelType::Forum,
            server_id: server_id.clone(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
        },
        Channel {
            id: discussions_channel_id(&owner, &name),
            name: "discussions".to_string(),
            channel_type: ChannelType::Forum,
            server_id: server_id.clone(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
        },
        Channel {
            id: code_channel_id(&owner, &name),
            name: "code".to_string(),
            channel_type: ChannelType::Code,
            server_id,
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
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
        thread: None,
        preview_image_url: None,
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
        thread: None,
        preview_image_url: None,
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

// ---------------------------------------------------------------------------
// ViewRow / ViewDetail mappers (Pack E.4)
// ---------------------------------------------------------------------------

/// Map a [`ForgejoIssue`] into a [`ViewRow`] for the list pane.
///
/// Pure function — suitable for unit testing without a running client.
#[must_use]
pub fn map_issue_to_viewrow(issue: &ForgejoIssue) -> ViewRow {
    ViewRow {
        id: issue.number.to_string(),
        primary_text: issue.title.clone(),
        secondary_text: Some(format!("#{} by {}", issue.number, issue.user.login)),
        meta_text: Some(format!(
            "SCORE:0 · {} comments · {}",
            issue.comments,
            humanize_age(&issue.created_at)
        )),
        icon: None,
        badge: Some(issue.state.clone()),
        context_menu_target_kind: MenuTargetKind::Channel,
        preview_image_url: None,
    }
}

/// Build a [`ViewDetail`] for a single issue: body as `CustomBlock` with
/// comments rendered inline beneath the body.
///
/// Accepts the fetched comments list so the caller can show the complete
/// thread in the split-pane detail panel without a second round-trip.
#[must_use]
pub fn issue_to_view_detail(issue: &ForgejoIssue, comments: &[ForgejoComment]) -> ViewDetail {
    let mut html = format!(
        "<p class=\"issue-body\">{}</p>",
        html_escape(&issue.body.clone().unwrap_or_default())
    );

    if !comments.is_empty() {
        html.push_str(&format!(
            "<hr><p class=\"comments-heading\"><strong>{} comment{}</strong></p>",
            comments.len(),
            if comments.len() == 1 { "" } else { "s" }
        ));
        for c in comments {
            html.push_str(&format!(
                "<div class=\"issue-comment\"><p class=\"comment-author\"><strong>{}</strong> · {}</p><p class=\"comment-body\">{}</p></div>",
                html_escape(&c.user.login),
                html_escape(&humanize_age(&c.created_at)),
                html_escape(&c.body.clone().unwrap_or_default()),
            ));
        }
    }

    ViewDetail {
        body_block: CustomBlock {
            sanitized_html: html,
            stylesheet: None,
            max_height_px: None,
        },
        comments_section: if !comments.is_empty() {
            Some(TreeSpec {
                root_page_size: u32::try_from(comments.len()).unwrap_or(u32::MAX),
                max_depth: 1,
            })
        } else {
            None
        },
    }
}

/// Return a human-readable age string from an RFC3339 timestamp.
///
/// Examples: "just now", "5m", "3h", "2d", "4mo", "1y"
#[must_use]
pub fn humanize_age(created_at: &str) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(created_at) else {
        return "unknown".to_string();
    };
    let secs = Utc::now()
        .signed_duration_since(dt.with_timezone(&Utc))
        .num_seconds()
        .max(0);
    // lint-allow-unused: time-bucket boundaries; truncation is the desired display semantic
    #[allow(clippy::integer_division)]
    match secs {
        s if s < 60 => "just now".to_string(),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86400 => format!("{}h", s / 3600),
        s if s < 86400 * 30 => format!("{}d", s / 86400),
        s if s < 86400 * 365 => format!("{}mo", s / (86400 * 30)),
        s => format!("{}y", s / (86400 * 365)),
    }
}

/// Minimal HTML escape for issue body text.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc))
}

/// Filter a slice of repos to those with `updated_at` within the last 2 years
/// and not archived.
#[must_use]
pub fn filter_active_repos(repos: Vec<ForgejoRepo>) -> Vec<ForgejoRepo> {
    let cutoff = Utc::now()
        .checked_sub_signed(chrono::Duration::days(365 * 2))
        .unwrap_or(chrono::DateTime::<Utc>::MIN_UTC);
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
