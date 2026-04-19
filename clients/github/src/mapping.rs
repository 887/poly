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
    }
}

/// Build a [`ViewDetail`] for a single issue: body as `CustomBlock` plus
/// a comments section describing the thread shape.
#[must_use]
pub fn issue_to_view_detail(issue: &GhIssue, comment_count: u32) -> ViewDetail {
    let body_html = format!(
        "<p>{}</p>",
        html_escape(&issue.body.clone().unwrap_or_default())
    );
    ViewDetail {
        body_block: CustomBlock {
            sanitized_html: body_html,
            stylesheet: None,
            max_height_px: None,
        },
        comments_section: if comment_count > 0 {
            Some(poly_client::TreeSpec {
                root_page_size: comment_count,
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
    let secs = (Utc::now() - dt.with_timezone(&Utc)).num_seconds().max(0);
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::types::{
        GhActor, GhDiscussion, GhDiscussionCategory, GhDiscussionComments, GhReactions, GhUser,
    };

    fn make_issue(number: u64, title: &str, is_pr: bool) -> crate::types::GhIssue {
        crate::types::GhIssue {
            id: number,
            number,
            title: title.to_string(),
            body: Some("test body".to_string()),
            user: GhUser {
                id: 1,
                login: "testuser".to_string(),
                avatar_url: None,
            },
            state: "open".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            html_url: format!("https://github.com/owner/repo/issues/{number}"),
            pull_request: if is_pr { Some(serde_json::json!({})) } else { None },
            comments: 3,
            reactions: GhReactions { total_count: 7 },
        }
    }

    #[test]
    fn test_map_issue_to_viewrow_fields() {
        let issue = make_issue(42, "Fix the bug", false);
        let row = map_issue_to_viewrow(&issue);

        assert_eq!(row.id, "42");
        assert_eq!(row.primary_text, "Fix the bug");
        assert_eq!(row.secondary_text.as_deref(), Some("#42 by testuser"));
        let meta = row.meta_text.unwrap();
        assert!(meta.contains("SCORE:7"), "meta should contain SCORE:7");
        assert!(meta.contains("3 comments"), "meta should contain comment count");
        assert_eq!(row.badge, Some("open".to_string()));
    }

    #[test]
    fn test_map_issue_to_viewrow_pr_state() {
        let issue = make_issue(5, "My PR", true);
        let row = map_issue_to_viewrow(&issue);
        assert_eq!(row.id, "5");
        assert_eq!(row.badge, Some("open".to_string()));
    }

    #[test]
    fn test_humanize_age_recent() {
        // "just now" when timestamp is essentially now
        let now = Utc::now().to_rfc3339();
        let result = humanize_age(&now);
        assert_eq!(result, "just now");
    }

    #[test]
    fn test_humanize_age_minutes() {
        let ts = (Utc::now() - chrono::Duration::minutes(10)).to_rfc3339();
        assert_eq!(humanize_age(&ts), "10m");
    }

    #[test]
    fn test_humanize_age_hours() {
        let ts = (Utc::now() - chrono::Duration::hours(5)).to_rfc3339();
        assert_eq!(humanize_age(&ts), "5h");
    }

    #[test]
    fn test_humanize_age_days() {
        let ts = (Utc::now() - chrono::Duration::days(3)).to_rfc3339();
        assert_eq!(humanize_age(&ts), "3d");
    }

    #[test]
    fn test_humanize_age_invalid() {
        assert_eq!(humanize_age("not-a-date"), "unknown");
    }

    #[test]
    fn test_issue_to_view_detail_with_comments() {
        let issue = make_issue(1, "Title", false);
        let detail = issue_to_view_detail(&issue, 3);
        assert!(detail.body_block.sanitized_html.contains("test body"));
        assert!(detail.comments_section.is_some());
    }

    #[test]
    fn test_issue_to_view_detail_no_comments() {
        let issue = make_issue(2, "Title", false);
        let detail = issue_to_view_detail(&issue, 0);
        assert!(detail.comments_section.is_none());
    }

    #[test]
    fn test_html_escape_in_body() {
        let mut issue = make_issue(10, "Escape test", false);
        issue.body = Some("<script>alert('xss')</script>".to_string());
        let detail = issue_to_view_detail(&issue, 0);
        assert!(
            !detail.body_block.sanitized_html.contains("<script>"),
            "raw <script> must be escaped"
        );
        assert!(
            detail.body_block.sanitized_html.contains("&lt;script&gt;"),
            "must contain HTML-escaped form"
        );
    }

    // -----------------------------------------------------------------------
    // GhDiscussion → ViewRow tests
    // -----------------------------------------------------------------------

    fn make_discussion(
        number: u64,
        title: &str,
        answered: bool,
        closed: bool,
        upvotes: u32,
        comment_count: u32,
    ) -> GhDiscussion {
        GhDiscussion {
            number,
            title: title.to_string(),
            body_text: Some("some body".to_string()),
            url: format!("https://github.com/owner/repo/discussions/{number}"),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
            upvote_count: upvotes,
            comments: GhDiscussionComments { total_count: comment_count },
            author: Some(GhActor {
                login: "alice".to_string(),
                avatar_url: Some("https://github.com/alice.png".to_string()),
            }),
            category: GhDiscussionCategory {
                id: "cat-1".to_string(),
                name: "General".to_string(),
                emoji: Some("💬".to_string()),
            },
            answer_chosen_at: if answered {
                Some("2026-01-03T00:00:00Z".to_string())
            } else {
                None
            },
            closed,
        }
    }

    #[test]
    fn test_map_discussion_basic_fields() {
        let d = make_discussion(99, "Best practice for X?", false, false, 12, 5);
        let row = map_discussion_to_viewrow(&d);

        assert_eq!(row.id, "99");
        assert_eq!(row.primary_text, "Best practice for X?");
        assert_eq!(row.secondary_text.as_deref(), Some("General"));
        let meta = row.meta_text.unwrap();
        assert!(meta.contains("12"), "meta should contain upvote count");
        assert!(meta.contains('5'.to_string().as_str()), "meta should contain comment count");
        assert_eq!(row.icon.as_deref(), Some("💬"));
        assert!(row.badge.is_none(), "open unanswered should have no badge");
    }

    #[test]
    fn test_map_discussion_answered_badge() {
        let d = make_discussion(10, "How to do Y?", true, false, 3, 2);
        let row = map_discussion_to_viewrow(&d);
        assert_eq!(row.badge.as_deref(), Some("answered"));
    }

    #[test]
    fn test_map_discussion_closed_badge() {
        let d = make_discussion(11, "Old topic", false, true, 0, 0);
        let row = map_discussion_to_viewrow(&d);
        assert_eq!(row.badge.as_deref(), Some("closed"));
    }

    #[test]
    fn test_map_discussion_answered_takes_precedence_over_closed() {
        // answered + closed → badge should be "answered"
        let d = make_discussion(12, "Resolved and closed", true, true, 1, 1);
        let row = map_discussion_to_viewrow(&d);
        assert_eq!(row.badge.as_deref(), Some("answered"));
    }

    #[test]
    fn test_map_discussion_no_emoji() {
        let mut d = make_discussion(20, "No emoji cat", false, false, 0, 0);
        d.category.emoji = None;
        let row = map_discussion_to_viewrow(&d);
        assert!(row.icon.is_none());
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
