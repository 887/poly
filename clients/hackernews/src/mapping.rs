//! Convert HN types to Poly shared types.

use chrono::{DateTime, TimeZone, Utc};
use poly_client::{
    Attachment, BackendType, Category, Channel, ChannelType, MenuTargetKind, Message,
    MessageContent, MessageReplyPreview, Reaction, Server, User, ViewRow, PresenceStatus,
};

use crate::types::{HnFeed, HnItem, HnItemType, HnUser};

pub(crate) const SERVER_ID: &str = "hn";

/// Build the static "Hacker News" virtual server.
///
/// `account_id` must be the real session id (e.g. "hn-anonymous" or
/// "hn-{username}") so that route URLs and backend lookups stay in sync.
pub fn build_server(account_id: &str) -> Server {
    Server {
        id: SERVER_ID.to_string(),
        name: "Hacker News".to_string(),
        icon_url: Some("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 40 40'%3E%3Crect width='40' height='40' rx='8' fill='%23ff6600'/%3E%3Ctext x='20' y='27' font-family='sans-serif' font-size='15' font-weight='bold' text-anchor='middle' fill='white'%3EHN%3C/text%3E%3C/svg%3E".to_string()),
        banner_url: None,
        categories: vec![
            Category {
                id: "hn-stories".to_string(),
                name: "Stories".to_string(),
                channel_ids: vec![
                    HnFeed::Top.channel_id().to_string(),
                    HnFeed::New.channel_id().to_string(),
                    HnFeed::Best.channel_id().to_string(),
                ],
            },
            Category {
                id: "hn-askshow".to_string(),
                name: "Ask & Show".to_string(),
                channel_ids: vec![
                    HnFeed::Ask.channel_id().to_string(),
                    HnFeed::Show.channel_id().to_string(),
                ],
            },
            Category {
                id: "hn-jobs".to_string(),
                name: "Jobs".to_string(),
                channel_ids: vec![HnFeed::Jobs.channel_id().to_string()],
            },
        ],
        backend: BackendType::from("hackernews"),
        unread_count: 0,
        mention_count: 0,
        account_id: account_id.to_string(),
        account_display_name: "Hacker News".to_string(),
        default_channel_id: None,
    }
}

/// Build the 6 story feed channels.
pub fn build_channels() -> Vec<Channel> {
    let feeds = [
        HnFeed::Top,
        HnFeed::New,
        HnFeed::Best,
        HnFeed::Ask,
        HnFeed::Show,
        HnFeed::Jobs,
    ];

    feeds
        .iter()
        .map(|&feed| Channel {
            id: feed.channel_id().to_string(),
            name: feed.display_name().to_string(),
            channel_type: ChannelType::HackerNews,
            server_id: SERVER_ID.to_string(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
        })
        .collect()
}

/// Strip HTML tags from a string, replacing common block tags with newlines.
pub fn strip_html(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut chars = html.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '<' => {
                in_tag = true;
                // Check for block-level tags that should become newlines
                let rest: String = chars.clone().take(5).collect();
                let tag_lower = rest.to_lowercase();
                if tag_lower.starts_with("p>")
                    || tag_lower.starts_with("/p>")
                    || tag_lower.starts_with("br>")
                    || tag_lower.starts_with("br ")
                    || tag_lower.starts_with("br/")
                {
                    result.push('\n');
                }
            }
            '>' => {
                in_tag = false;
            }
            '&' if !in_tag => {
                // Decode common HTML entities
                let entity: String = chars.clone().take(6).collect();
                if entity.starts_with("amp;") {
                    result.push('&');
                    for _ in 0..4 {
                        chars.next();
                    }
                } else if entity.starts_with("lt;") {
                    result.push('<');
                    for _ in 0..3 {
                        chars.next();
                    }
                } else if entity.starts_with("gt;") {
                    result.push('>');
                    for _ in 0..3 {
                        chars.next();
                    }
                } else if entity.starts_with("quot;") {
                    result.push('"');
                    for _ in 0..5 {
                        chars.next();
                    }
                } else if entity.starts_with("#39;") {
                    result.push('\'');
                    for _ in 0..4 {
                        chars.next();
                    }
                } else if entity.starts_with("#x27;") {
                    result.push('\'');
                    for _ in 0..5 {
                        chars.next();
                    }
                } else if entity.starts_with("#x2F;") || entity.starts_with("#x2f;") {
                    result.push('/');
                    for _ in 0..5 {
                        chars.next();
                    }
                } else if entity.starts_with("apos;") {
                    result.push('\'');
                    for _ in 0..5 {
                        chars.next();
                    }
                } else {
                    result.push('&');
                }
            }
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    // Collapse multiple newlines into at most two
    let mut cleaned = String::with_capacity(result.len());
    let mut prev_newline = false;
    for ch in result.chars() {
        if ch == '\n' {
            if !prev_newline {
                cleaned.push('\n');
            }
            prev_newline = true;
        } else {
            prev_newline = false;
            cleaned.push(ch);
        }
    }

    cleaned.trim().to_string()
}

fn timestamp_from_unix(unix: Option<u64>) -> DateTime<Utc> {
    unix.and_then(|t| Utc.timestamp_opt(t as i64, 0).single())
        .unwrap_or_else(Utc::now)
}

fn anonymous_user() -> User {
    User {
        id: "anonymous".to_string(),
        display_name: "anonymous".to_string(),
        avatar_url: None,
        presence: PresenceStatus::Offline,
        backend: BackendType::from("hackernews"),
    }
}

fn hn_author_to_user(by: Option<&str>) -> User {
    match by {
        Some(username) => User {
            id: username.to_string(),
            display_name: username.to_string(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: BackendType::from("hackernews"),
        },
        None => anonymous_user(),
    }
}

/// Format a story item as readable text content.
pub fn format_story_text(item: &HnItem) -> String {
    let title = item.title.as_deref().unwrap_or("(no title)");
    let by = item.by.as_deref().unwrap_or("unknown");

    match item.item_type {
        HnItemType::Job => {
            let mut lines = vec![title.to_string()];
            if let Some(url) = &item.url {
                lines.push(url.clone());
            }
            lines.join("\n")
        }
        _ => {
            let score = item.score.unwrap_or(0);
            let comments = item.descendants.unwrap_or(0);

            if let Some(url) = &item.url {
                format!("{title}\n{url}\n\n{score} points | {comments} comments | by {by}")
            } else if let Some(text) = &item.text {
                let body = strip_html(text);
                format!("{title}\n\n{body}\n\n{score} points | {comments} comments | by {by}")
            } else {
                format!("{title}\n\n{score} points | {comments} comments | by {by}")
            }
        }
    }
}

/// Convert a HN story item to a Poly Message.
pub fn hn_item_to_message(item: &HnItem) -> Message {
    let content_text = format_story_text(item);
    let author = hn_author_to_user(item.by.as_deref());
    let timestamp = timestamp_from_unix(item.time);

    let mut reactions = Vec::new();
    if let Some(score) = item.score {
        reactions.push(Reaction {
            emoji: "🔥".to_string(),
            count: score,
            me: false,
        });
    }
    if let Some(descendants) = item.descendants {
        reactions.push(Reaction {
            emoji: "💬".to_string(),
            count: descendants,
            me: false,
        });
    }

    let attachments = if let Some(url) = &item.url {
        vec![Attachment::remote(
            format!("url-{}", item.id),
            item.title
                .as_deref()
                .unwrap_or("Link")
                .to_string(),
            "text/html".to_string(),
            url.clone(),
            0,
        )]
    } else {
        Vec::new()
    };

    Message {
        id: item.id.to_string(),
        author,
        content: MessageContent::Text(content_text),
        timestamp,
        attachments,
        reactions,
        reply_to: None,
        edited: false,
        thread: None,
    }
}

/// Convert a HN comment item to a Poly Message.
/// Convert a HN comment item to a Poly `Message`.
///
/// `parent_id` is the numeric ID of the parent comment (or story for top-level
/// comments). Pass `None` for top-level comments so `reply_to` stays `None`
/// and `build_comment_tree` correctly places them at the root.
pub fn hn_comment_to_message(item: &HnItem, parent_id: Option<u64>, story_id: u64) -> Message {
    let text = if item.deleted.unwrap_or(false) {
        "[deleted]".to_string()
    } else if item.dead.unwrap_or(false) {
        "[flagged]".to_string()
    } else {
        item.text
            .as_deref()
            .map(strip_html)
            .unwrap_or_default()
    };

    let author = hn_author_to_user(item.by.as_deref());
    let timestamp = timestamp_from_unix(item.time);

    // Only set reply_to for non-top-level comments (parent ≠ story).
    let reply_to = parent_id
        .filter(|&pid| pid != story_id)
        .map(|pid| MessageReplyPreview {
            message_id: pid.to_string(),
            author_id: String::new(),
            author_display_name: String::new(),
            author_avatar_url: None,
            snippet: String::new(),
        });

    Message {
        id: item.id.to_string(),
        author,
        content: MessageContent::Text(text),
        timestamp,
        attachments: Vec::new(),
        reactions: Vec::new(),
        reply_to,
        edited: false,
        thread: None,
    }
}

/// Convert a HN user profile to a Poly User.
pub fn hn_user_to_user(user: &HnUser) -> User {
    User {
        id: user.id.clone(),
        display_name: user.id.clone(),
        avatar_url: None,
        presence: PresenceStatus::Offline,
        backend: BackendType::from("hackernews"),
    }
}

/// Check if a channel ID refers to a post's comment thread.
/// Post channels use the convention `hn-post-{item_id}`.
pub fn post_id_from_channel(channel_id: &str) -> Option<u64> {
    channel_id
        .strip_prefix("hn-post-")
        .and_then(|id_str| id_str.parse().ok())
}

/// Return a human-readable age string for a Unix timestamp, e.g. "3h", "2d".
pub fn humanize_age(unix_secs: Option<u64>) -> String {
    let Some(t) = unix_secs else {
        return "?".to_string();
    };
    let now = Utc::now().timestamp() as u64;
    let secs = now.saturating_sub(t);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3_600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3_600)
    } else if secs < 86_400 * 30 {
        format!("{}d", secs / 86_400)
    } else if secs < 86_400 * 365 {
        format!("{}mo", secs / (86_400 * 30))
    } else {
        format!("{}y", secs / (86_400 * 365))
    }
}

/// Map a HN story item to a `ViewRow`.
pub fn hn_item_to_view_row(item: &HnItem) -> ViewRow {
    let title = item.title.clone().unwrap_or_default();

    let secondary_text = item
        .url
        .clone()
        .or_else(|| item.by.as_ref().map(|by| format!("by {by}")));

    let meta_text = Some(format!(
        "{}pt · {} comments · {}",
        item.score.unwrap_or(0),
        item.descendants.unwrap_or(0),
        humanize_age(item.time),
    ));

    ViewRow {
        id: item.id.to_string(),
        primary_text: title,
        secondary_text,
        meta_text,
        icon: None,
        badge: None,
        context_menu_target_kind: MenuTargetKind::Message,
    }
}

// ---------------------------------------------------------------------------
// Unit tests (Pack E.2 — §1.2 layer a)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::types::{HnFeed, HnItem, HnItemType};

    fn story(id: u64, title: &str, url: Option<&str>, by: &str, score: u32, descendants: u32, time: u64) -> HnItem {
        HnItem {
            id,
            item_type: HnItemType::Story,
            by: Some(by.to_string()),
            time: Some(time),
            text: None,
            url: url.map(str::to_string),
            title: Some(title.to_string()),
            score: Some(score),
            descendants: Some(descendants),
            kids: None,
            parent: None,
            dead: None,
            deleted: None,
        }
    }

    fn ask_hn_story(id: u64, title: &str, text: &str, by: &str, score: u32, descendants: u32, time: u64) -> HnItem {
        HnItem {
            id,
            item_type: HnItemType::Story,
            by: Some(by.to_string()),
            time: Some(time),
            text: Some(text.to_string()),
            url: None,
            title: Some(title.to_string()),
            score: Some(score),
            descendants: Some(descendants),
            kids: None,
            parent: None,
            dead: None,
            deleted: None,
        }
    }

    // -- Feed ID mapping tests --

    #[test]
    fn feed_channel_ids_roundtrip() {
        let pairs = [
            (HnFeed::Top, "hn-top"),
            (HnFeed::New, "hn-new"),
            (HnFeed::Best, "hn-best"),
            (HnFeed::Ask, "hn-ask"),
            (HnFeed::Show, "hn-show"),
            (HnFeed::Jobs, "hn-jobs-ch"),
        ];
        for (feed, expected_id) in &pairs {
            assert_eq!(feed.channel_id(), *expected_id, "channel_id mismatch for {feed:?}");
            let parsed = HnFeed::from_channel_id(expected_id);
            assert_eq!(parsed, Some(*feed), "from_channel_id failed for {expected_id}");
        }
    }

    #[test]
    fn feed_unknown_channel_id_returns_none() {
        assert!(HnFeed::from_channel_id("hn-forum-unknown").is_none());
    }

    // -- ViewRow mapping tests --

    #[test]
    fn map_story_with_url_to_viewrow() {
        let item = story(42, "Rust 2.0 Released", Some("https://example.com"), "pg", 500, 200, 1_700_000_000);
        let row = hn_item_to_view_row(&item);

        assert_eq!(row.id, "42");
        assert_eq!(row.primary_text, "Rust 2.0 Released");
        assert_eq!(row.secondary_text.as_deref(), Some("https://example.com"));
        let meta = row.meta_text.expect("meta_text must be Some");
        assert!(meta.contains("500pt"), "meta should contain score: {meta}");
        assert!(meta.contains("200 comments"), "meta should contain descendants: {meta}");
    }

    #[test]
    fn map_ask_hn_story_secondary_is_by() {
        // Ask HN stories have no URL — secondary_text should be "by <author>"
        let item = ask_hn_story(99, "Ask HN: Best Rust books?", "Post body here", "tptacek", 200, 80, 1_700_000_000);
        let row = hn_item_to_view_row(&item);

        assert_eq!(row.id, "99");
        assert_eq!(row.primary_text, "Ask HN: Best Rust books?");
        assert_eq!(row.secondary_text.as_deref(), Some("by tptacek"));
    }

    #[test]
    fn map_story_id_is_string() {
        let item = story(1234567, "Test", None, "anon", 1, 0, 1_700_000_000);
        let row = hn_item_to_view_row(&item);
        assert_eq!(row.id, "1234567");
    }

    // -- humanize_age tests --

    #[test]
    fn humanize_age_none_returns_question_mark() {
        assert_eq!(humanize_age(None), "?");
    }

    #[test]
    fn humanize_age_recent() {
        use chrono::Utc;
        let now = Utc::now().timestamp() as u64;
        // 30 seconds ago
        assert_eq!(humanize_age(Some(now - 30)), "30s");
        // 90 seconds = 1 minute
        assert_eq!(humanize_age(Some(now - 90)), "1m");
        // 2 hours
        assert_eq!(humanize_age(Some(now - 7200)), "2h");
        // 3 days
        assert_eq!(humanize_age(Some(now - 86_400 * 3)), "3d");
    }
}
