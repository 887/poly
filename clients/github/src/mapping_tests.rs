//! Test fixtures and unit tests for [`crate::mapping`].
//!
//! Split out of `mapping.rs` to keep production mapping logic free of
//! test-only fixture builders. The tests still reach into `crate::types`
//! (which is `pub(crate)`) and the public `mapping` surface; nothing in
//! production code depends on this module.

// lint-allow-unused: test module — assertion macros and fixture builders use unwrap/expect/panic idiomatically to fail fast on bad inputs
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use chrono::Utc;

use crate::mapping::{
    humanize_age, issue_to_view_detail, map_discussion_to_viewrow, map_issue_to_viewrow,
};
use crate::types::{
    GhActor, GhDiscussion, GhDiscussionCategory, GhDiscussionComments, GhIssue, GhIssueComment,
    GhReactions, GhUser,
};

fn make_issue(number: u64, title: &str, is_pr: bool) -> GhIssue {
    GhIssue {
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
    let comment = GhIssueComment {
        id: 10,
        user: GhUser { id: 2, login: "commenter".to_string(), avatar_url: None },
        body: Some("great issue".to_string()),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        html_url: "https://github.com/owner/repo/issues/1#issuecomment-10".to_string(),
    };
    let detail = issue_to_view_detail(&issue, &[comment]);
    assert!(detail.body_block.sanitized_html.contains("test body"));
    assert!(detail.body_block.sanitized_html.contains("great issue"));
    assert!(detail.body_block.sanitized_html.contains("commenter"));
    assert!(detail.comments_section.is_some());
}

#[test]
fn test_issue_to_view_detail_no_comments() {
    let issue = make_issue(2, "Title", false);
    let detail = issue_to_view_detail(&issue, &[]);
    assert!(detail.comments_section.is_none());
}

#[test]
fn test_html_escape_in_body() {
    let mut issue = make_issue(10, "Escape test", false);
    issue.body = Some("<script>alert('xss')</script>".to_string());
    let detail = issue_to_view_detail(&issue, &[]);
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
