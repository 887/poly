//! Convert HN types to Poly shared types.

use chrono::{DateTime, TimeZone, Utc};
use poly_client::{
    Attachment, BackendType, Category, Channel, ChannelType, Message, MessageContent, Reaction,
    Server, User, PresenceStatus,
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
        icon_url: None,
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
            channel_type: ChannelType::Forum,
            server_id: SERVER_ID.to_string(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
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
    }
}

/// Convert a HN comment item to a Poly Message.
pub fn hn_comment_to_message(item: &HnItem) -> Message {
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

    Message {
        id: item.id.to_string(),
        author,
        content: MessageContent::Text(text),
        timestamp,
        attachments: Vec::new(),
        reactions: Vec::new(),
        reply_to: None,
        edited: false,
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
