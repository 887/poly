//! Raw data structures produced by the `old.reddit.com` HTML parsers.
//!
//! These are plain data structs with no parsing logic — they act as the
//! boundary between the scraping layer and the `RedditClient` / backend
//! mapping layer.

#![cfg(feature = "native")]

use chrono::{DateTime, Utc};

/// A subreddit submission ("post" / "thing of type link") parsed from a
/// listing or comments page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPost {
    /// Reddit `t3_` ID without the `t3_` prefix.
    pub id: String,
    /// Username of the submitter (no `u/` prefix).
    pub author: String,
    /// Subreddit display name (no `r/` prefix).
    pub subreddit: String,
    /// Net upvote score (positive integer; downvote-fuzzed but stable).
    pub score: i64,
    /// Submission time as RFC-3339, parsed from `<time datetime="...">`.
    pub timestamp: DateTime<Utc>,
    /// Submission title.
    pub title: String,
    /// Optional self-post body (HTML-rendered markdown). `None` for link
    /// posts.
    pub body: Option<String>,
    /// Reddit-internal permalink path (e.g. `/r/rust/comments/.../slug/`).
    pub permalink: String,
    /// Number of top-level + nested comments per reddit's count.
    pub comment_count: u32,
    /// External URL the link post points at; `None` for self-posts.
    pub url: Option<String>,
    /// Preview/thumbnail image URL when this is an image post or a post with
    /// a thumbnail available (i.e. `data-url` points to an image host like
    /// `i.redd.it`, `i.imgur.com`, `imgur.com`, `preview.redd.it`).
    /// `None` for text-only self-posts and link posts without an image.
    pub preview_url: Option<String>,
    /// `true` when `data-domain` indicates a video host (`v.redd.it`,
    /// `youtu.be`, `youtube.com`, `vimeo.com`, `gfycat.com`,
    /// `streamable.com`).
    pub is_video: bool,
    /// `true` when the post carries `data-is-gallery="true"` (Reddit
    /// multi-image gallery posts).
    pub is_gallery: bool,
}

/// A comment ("thing of type comment", `t1_`) — possibly with nested
/// replies. The nesting reflects the HTML structure rather than reddit's
/// `parent_id` graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawComment {
    /// Reddit `t1_` ID without the prefix.
    pub id: String,
    /// Username of the commenter.
    pub author: String,
    /// HTML-rendered comment body.
    pub body_html: String,
    /// Net score per reddit's `<span class="score unvoted" title="N">`.
    pub score: i64,
    /// Comment time, parsed from `<time datetime="...">`.
    pub timestamp: DateTime<Utc>,
    /// Reddit-internal permalink to this comment.
    pub permalink: String,
    /// Direct child comments (HTML nesting; depth-first preserved).
    pub replies: Vec<RawComment>,
}

/// A direct message thread root (`t4_`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDm {
    /// Reddit `t4_` ID without the prefix.
    pub id: String,
    /// Other party's username (sender for inbox, recipient for sent).
    pub author: String,
    /// Subject line.
    pub subject: String,
    /// HTML-rendered body of the latest message in the thread.
    pub body_html: String,
    /// Time of the latest message in the thread.
    pub timestamp: DateTime<Utc>,
}

/// Aggregate user profile fields extracted from `/user/<u>/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserProfile {
    /// Username (no `u/` prefix).
    pub name: String,
    /// Optional avatar URL — `None` for users without a custom avatar
    /// (the default snoo image is not surfaced).
    pub avatar_url: Option<String>,
    /// Recent submissions and comments rendered on the overview page.
    pub recent_items: Vec<UserOverviewItem>,
}

/// One row from a user's overview — either a post they submitted or a
/// comment they made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserOverviewItem {
    /// A submission (corresponds to a `t3_` thing on the page).
    Post(RawPost),
    /// A comment (corresponds to a `t1_` thing on the page).
    Comment(RawComment),
}
