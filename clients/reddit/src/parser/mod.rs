//! HTML parser layer for `old.reddit.com` responses.
//!
//! Per-page-type submodules — each takes `&str` HTML and returns either
//! the parsed structures or a `ParseError`. The scraper logic lives here
//! so the higher-level `RedditClient` (Phase D-E) only orchestrates HTTP
//! + state and never touches `scraper::Selector` directly.
//!
//! # LoggedOut detection
//!
//! Every parser checks for the LoggedOut markers FIRST. Reddit redirects
//! authenticated routes to a login page when the cookie is missing or
//! expired:
//! - **Legacy form**: presence of `<form ... class="login-form">`
//! - **Modern shreddit**: `class="theme-beta"` AND
//!   `<title>Welcome to Reddit</title>` (the `/login` URL now serves a
//!   React app, no scrapable form)
//!
//! Both markers must surface as `ParseError::LoggedOut` so the caller
//! can prompt for a fresh `reddit_session` cookie.

#![cfg(feature = "native")]

use chrono::{DateTime, Utc};
use scraper::Html;
use thiserror::Error;

pub mod inbox;
pub mod post;
pub mod subreddit;
pub mod user;

/// Errors produced by every parser in this module.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    /// Response is the login page — caller's `reddit_session` cookie is
    /// missing or expired.
    #[error("response is a login redirect (cookie missing or expired)")]
    LoggedOut,
    /// A required CSS selector matched zero elements where at least one
    /// was expected (e.g. the comments page contained no OP container).
    #[error("missing required element: {0}")]
    MissingElement(&'static str),
    /// A `data-*` attribute that was expected to be a non-negative
    /// integer (score, timestamp, comment-count) failed to parse.
    #[error("malformed integer attribute: {0}")]
    MalformedInt(&'static str),
    /// A `<time datetime="...">` value failed RFC-3339 parsing.
    #[error("malformed timestamp attribute: {0}")]
    MalformedTimestamp(String),
}

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

/// Detect the two LoggedOut markers without parsing the full DOM.
///
/// Cheap byte-level check up front; intended to be called by every
/// page-type parser as the first step.
pub(crate) fn detect_logged_out(html: &str) -> bool {
    // Modern shreddit React app served at /login.
    if html.contains("class=\"theme-beta\"")
        && html.contains("<title>Welcome to Reddit</title>")
    {
        return true;
    }
    // Legacy login form (still served on some auth redirects).
    if html.contains("class=\"login-form\"") || html.contains("class=\" login-form\"") {
        return true;
    }
    false
}

/// Parse the input HTML once and return the live `Html` document along
/// with the `LoggedOut` short-circuit applied.
pub(crate) fn parse_html(html: &str) -> Result<Html, ParseError> {
    if detect_logged_out(html) {
        return Err(ParseError::LoggedOut);
    }
    Ok(Html::parse_document(html))
}

/// Extract a `data-*` attribute value from a node. Returns `None` if the
/// attribute is absent OR empty (reddit emits empty strings for some
/// attrs on logged-out renders).
pub(crate) fn data_attr<'a>(
    el: &'a scraper::ElementRef<'_>,
    name: &str,
) -> Option<&'a str> {
    let v = el.value().attr(name)?;
    if v.is_empty() { None } else { Some(v) }
}

/// Parse a `data-timestamp` value (epoch milliseconds, as reddit emits)
/// into a UTC `DateTime`. Falls back to RFC-3339 parse if the value
/// looks like a date string.
pub(crate) fn parse_timestamp_ms(raw: &str) -> Result<DateTime<Utc>, ParseError> {
    if let Ok(ms) = raw.parse::<i64>() {
        let secs = ms.div_euclid(1000);
        let nsec = u32::try_from(ms.rem_euclid(1000).saturating_mul(1_000_000))
            .unwrap_or(0);
        return DateTime::from_timestamp(secs, nsec)
            .ok_or_else(|| ParseError::MalformedTimestamp(raw.to_string()));
    }
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| ParseError::MalformedTimestamp(raw.to_string()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn logged_out_matches_modern_shreddit_marker() {
        let html =
            r#"<html class="theme-beta"><head><title>Welcome to Reddit</title></head></html>"#;
        assert!(detect_logged_out(html));
    }

    #[test]
    fn logged_out_matches_legacy_form() {
        let html = r#"<form class="login-form" action="/api/login/">…</form>"#;
        assert!(detect_logged_out(html));
    }

    #[test]
    fn logged_out_negative_on_normal_page() {
        let html = r#"<html><body><div class="thing" data-fullname="t3_abc"></div></body></html>"#;
        assert!(!detect_logged_out(html));
    }

    #[test]
    fn parse_html_short_circuits_logged_out() {
        let html =
            r#"<html class="theme-beta"><head><title>Welcome to Reddit</title></head></html>"#;
        assert_eq!(parse_html(html), Err(ParseError::LoggedOut));
    }

    #[test]
    fn parse_timestamp_ms_handles_epoch_millis() {
        // 2026-04-30T04:23:38Z
        let ts = parse_timestamp_ms("1777523018000").unwrap();
        assert_eq!(ts.timestamp(), 1_777_523_018);
    }

    #[test]
    fn parse_timestamp_ms_falls_back_to_rfc3339() {
        let ts = parse_timestamp_ms("2026-04-30T04:23:38+00:00").unwrap();
        assert_eq!(ts.timestamp(), 1_777_523_018);
    }
}
