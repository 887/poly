//! HTML parser layer for `old.reddit.com` responses.
//!
//! Per-page-type submodules — each takes `&str` HTML and returns either
//! the parsed structures or a `ParseError`. The scraping logic lives here
//! so the higher-level `RedditClient` (Phase D-E) only orchestrates HTTP
//! + state and never touches `scraper::Selector` directly.
//!
//! # Module layout
//!
//! | Module | Contents |
//! |---|---|
//! | `error` | `ParseError` variants |
//! | `types` | `RawPost`, `RawComment`, `RawDm`, `UserProfile`, `UserOverviewItem` |
//! | `inbox` | Parser for `/message/inbox/` |
//! | `post` | Parser for `/r/<sub>/comments/<id>/` |
//! | `subreddit` | Parser for subreddit listing pages |
//! | `user` | Parser for `/user/<u>/` overview pages |
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

mod error;
mod types;

pub mod inbox;
pub mod post;
pub mod subreddit;
pub mod user;

// ─── Re-exports — keep the public surface stable ─────────────────────────────

pub use error::ParseError;
pub use types::{RawComment, RawDm, RawPost, UserOverviewItem, UserProfile};

// ─── Glue helpers used by every page-type parser ─────────────────────────────

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
