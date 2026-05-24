//! Convert GitHub JSON shapes into Poly client types.
//!
//! Channel ID conventions used inside the github backend:
//!
//! | Channel kind            | ID format                            |
//! |-------------------------|---------------------------------------|
//! | Issues forum (per repo) | `gh-issues-{owner}~{repo}`           |
//! | Pull requests forum     | `gh-pulls-{owner}~{repo}`            |
//! | Discussions forum       | `gh-discussions-{owner}~{repo}`      |
//! | Code explorer           | `gh-code-{owner}~{repo}`             |
//! | Single issue thread     | `gh-issue-{owner}~{repo}-{number}`   |
//!
//! The `{owner}/{repo}` portion uses `/` as the separator so that
//! owner and repo names containing hyphens round-trip unambiguously.
//! (GitHub enforces that neither owner nor repo names contain `/`.)
//!
//! Server IDs are the GitHub numeric repo ID prefixed with `gh-` so they
//! are stable across renames.

use chrono::{DateTime, Utc};
use poly_client::{
    Category, Channel, ChannelType, CustomBlock, MenuTargetKind, Message, MessageContent,
    PresenceStatus, Server, User, ViewDetail, ViewRow, BackendType,
};

use crate::types::{GhDiscussion, GhIssue, GhIssueComment, GhRepo, GhUser};

/// Backend slug used in `BackendType` and routes.
pub const BACKEND_SLUG: &str = "github";

/// Build a stable server ID for a repo.
#[must_use]
pub fn server_id_for_repo(repo: &GhRepo) -> String {
    format!("gh-{}", repo.id)
}

/// Channel ID for the per-repo issues forum.
///
/// Uses `owner/repo` as the separator so that hyphenated owner or repo
/// names round-trip unambiguously.
#[must_use]
pub fn issues_channel_id(owner: &str, repo: &str) -> String {
    format!("gh-issues-{owner}~{repo}")
}

/// Channel ID for the per-repo pull requests forum.
#[must_use]
pub fn pulls_channel_id(owner: &str, repo: &str) -> String {
    format!("gh-pulls-{owner}~{repo}")
}

/// Channel ID for the per-repo code explorer.
#[must_use]
pub fn code_channel_id(owner: &str, repo: &str) -> String {
    format!("gh-code-{owner}~{repo}")
}

/// Channel ID for a single issue/PR comment thread.
///
/// Currently constructed inline by the message-fetch path; kept exported so
/// the future thread-open routing can use it.
#[must_use]
pub fn issue_thread_channel_id(owner: &str, repo: &str, number: u64) -> String {
    format!("gh-issue-{owner}~{repo}-{number}")
}

/// Try to parse `(owner, repo)` out of a code-channel ID (`gh-code-{owner}~{repo}`).
#[must_use]
pub fn parse_code_channel(channel_id: &str) -> Option<(String, String)> {
    let rest = channel_id.strip_prefix("gh-code-")?;
    let (owner, repo) = rest.split_once('~')?;
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
        default_channel_id: None,
        description: repo.description.clone(),
        star_count: Some(repo.stargazers_count),
        language: repo.language.clone(),
        forks_count: Some(repo.forks_count),
        open_issues_count: Some(repo.open_issues_count),
    }
}

/// Build the full channel list for a repo:
/// issues forum, PR forum, discussions forum, code explorer.
///
/// Discussions used to live as a third tab inside the Issues channel
/// view, but the user asked for it to be a top-level sidebar entry like
/// Issues / Pull Requests so the toolbar tab row could be eliminated.
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

/// Channel ID for the Discussions forum on a repo.
#[must_use]
pub fn discussions_channel_id(owner: &str, repo: &str) -> String {
    format!("gh-discussions-{owner}~{repo}")
}

// ---------------------------------------------------------------------------
// ViewRow / ViewDetail mappers (Pack E.3)
// ---------------------------------------------------------------------------

/// Map a [`GhIssue`] into a [`ViewRow`] for the list pane.
///
/// Pure function — suitable for unit testing without a running client.
#[must_use]
pub fn map_issue_to_viewrow(issue: &GhIssue) -> ViewRow {
    ViewRow {
        id: issue.number.to_string(),
        primary_text: issue.title.clone(),
        secondary_text: Some(format!("#{} by {}", issue.number, issue.user.login)),
        meta_text: Some(format!(
            "SCORE:{} · {} comments · {}",
            issue.reactions.total_count,
            issue.comments,
            humanize_age(&issue.created_at)
        )),
        icon: None,
        badge: Some(issue.state.clone()),
        context_menu_target_kind: MenuTargetKind::Channel,
        preview_image_url: None,
        is_video: false,
    }
}

/// Map a [`GhDiscussion`] into a [`ViewRow`] for the discussions list pane.
///
/// Pure function — suitable for unit testing without a running client.
#[must_use]
pub fn map_discussion_to_viewrow(d: &GhDiscussion) -> ViewRow {
    let badge = if d.answer_chosen_at.is_some() {
        Some("answered".to_string())
    } else if d.closed {
        Some("closed".to_string())
    } else {
        None
    };

    ViewRow {
        id: d.number.to_string(),
        primary_text: d.title.clone(),
        secondary_text: Some(d.category.name.clone()),
        meta_text: Some(format!(
            "👍 {} · 💬 {}",
            d.upvote_count, d.comments.total_count
        )),
        icon: d.category.emoji.clone(),
        badge,
        context_menu_target_kind: MenuTargetKind::Channel,
        preview_image_url: None,
        is_video: false,
    }
}

/// Build a [`ViewDetail`] for a single issue: body as `CustomBlock` with
/// comments rendered inline beneath the body.
///
/// Accepts the fetched comments list so the caller can show the complete
/// thread in the split-pane detail panel without a second round-trip.
#[must_use]
pub fn issue_to_view_detail(issue: &GhIssue, comments: &[GhIssueComment]) -> ViewDetail {
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
        comments_section: if comments.is_empty() {
            None
        } else {
            Some(poly_client::TreeSpec {
                root_page_size: u32::try_from(comments.len()).unwrap_or(u32::MAX),
                max_depth: 1,
            })
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
    // lint-allow-unused: chrono Duration sub of (now - past) — bounded by i64 secs range
    #[allow(clippy::arithmetic_side_effects)]
    let secs = (Utc::now() - dt.with_timezone(&Utc)).num_seconds().max(0);
    // lint-allow-unused: human-readable age — integer truncation is the intent
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
        thread: None,
        preview_image_url: None,
    }
}

/// Convert a [`GhDiscussion`] into a Forum-style top-level [`Message`].
///
/// GitHub Discussions are read-only via the REST API available through
/// the `gh` CLI — the GraphQL mutation that creates discussion comments
/// requires a token scope (`write:discussion`) that `gh` does not expose
/// in the same way. We therefore map each discussion as a read-only
/// message and link out to the web URL for interaction.
#[must_use]
pub fn discussion_to_message(d: &GhDiscussion) -> Message {
    let author_login = d
        .author
        .as_ref()
        .map(|a| a.login.as_str())
        .unwrap_or("[deleted]");
    let author_avatar = d.author.as_ref().and_then(|a| a.avatar_url.clone());
    let body = d.body_text.clone().unwrap_or_default();
    let status = if d.answer_chosen_at.is_some() {
        "answered"
    } else if d.closed {
        "closed"
    } else {
        "open"
    };
    let text = format!(
        "**[Discussion #{}] {}** ({}) — {}\n\n{}\n\n{}",
        d.number, d.title, d.category.name, status, body, d.url
    );
    Message {
        id: format!("gh-discussion-msg-{}", d.number),
        author: User {
            id: author_login.to_string(),
            display_name: author_login.to_string(),
            avatar_url: author_avatar,
            presence: PresenceStatus::Offline,
            backend: BackendType::from(super::SLUG),
        },
        content: MessageContent::Text(text),
        timestamp: parse_ts(&d.created_at),
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
        thread: None,
        preview_image_url: None,
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
    DateTime::parse_from_rfc3339(s).map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc))
}

/// Filter a slice of repos to those with `pushed_at` within the last `years`.
///
/// Repos with no `pushed_at` (rare) are kept.
#[must_use]
pub fn filter_active_repos(repos: Vec<GhRepo>, years: i64) -> Vec<GhRepo> {
    // lint-allow-unused: chrono::Duration sub from now — bounded by caller-supplied years (i64)
    #[allow(clippy::arithmetic_side_effects)]
    let cutoff = Utc::now() - chrono::Duration::days(365_i64.saturating_mul(years));
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
