//! Parser for a single post + comment-thread page —
//! `/r/<sub>/comments/<id>/<slug>/`.
//!
//! Returns the OP (`t3_`) plus a recursively-nested tree of `RawComment`
//! built from `data-fullname^="t1_"` containers. Reddit emits HTML
//! nesting that mirrors the comment tree exactly: each top-level comment
//! is a `.thing[data-fullname^="t1_"]` under the `.commentarea`, and
//! its replies are a sibling `<div class="child"><div class="listing"><div class="thing">…`
//! tree underneath.

#![cfg(feature = "native")]

use scraper::{ElementRef, Selector};

use super::{
    ParseError, RawComment, RawPost, data_attr, parse_html, parse_timestamp_ms,
    subreddit::{parse_post_thing, post_selector},
};

/// Parse the OP submission and the full nested comment tree.
///
/// # Errors
///
/// - `ParseError::LoggedOut` — login redirect.
/// - `ParseError::MissingElement("op t3_")` — page contained no OP
///   container (deleted post / 404).
/// - Any malformed-int / malformed-timestamp from a comment short-
///   circuits the whole parse.
pub fn parse_post_page(html: &str) -> Result<(RawPost, Vec<RawComment>), ParseError> {
    let doc = parse_html(html)?;

    // OP — first .thing[data-fullname^=t3_] on the page is the submission.
    let op_sel = post_selector();
    let op = doc
        .select(&op_sel)
        .next()
        .ok_or(ParseError::MissingElement("op t3_"))?;
    let op_post = parse_post_thing(&op)?;

    // Comments live under .commentarea > .sitetable. Each top-level
    // comment is a direct .thing child of that .sitetable.
    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let top_comments_sel =
        Selector::parse(r#"div.commentarea > div.sitetable > div.thing.comment[data-fullname^="t1_"]"#)
            .unwrap();
    let mut comments = Vec::new();
    for el in doc.select(&top_comments_sel) {
        comments.push(parse_comment_node(&el)?);
    }
    Ok((op_post, comments))
}

fn parse_comment_node(el: &ElementRef<'_>) -> Result<RawComment, ParseError> {
    let id = data_attr(el, "data-fullname")
        .and_then(|v| v.strip_prefix("t1_"))
        .ok_or(ParseError::MissingElement("data-fullname (t1_)"))?
        .to_string();
    let author = data_attr(el, "data-author")
        .unwrap_or("[deleted]")
        .to_string();
    let permalink = data_attr(el, "data-permalink")
        .unwrap_or("")
        .to_string();

    // Score: <span class="score unvoted" title="N">N points</span>.
    // The `title` attribute is the canonical numeric value; the text
    // includes " points" / " point" pluralisation.
    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let score_sel = Selector::parse("span.score.unvoted").unwrap();
    let score = el
        .select(&score_sel)
        .next()
        .and_then(|s| s.value().attr("title"))
        .and_then(|t| t.parse::<i64>().ok())
        .unwrap_or(0);

    // Timestamp: <time class="live-timestamp" datetime="...">.
    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let time_sel = Selector::parse("time.live-timestamp").unwrap();
    let timestamp_raw = el
        .select(&time_sel)
        .next()
        .and_then(|t| t.value().attr("datetime"))
        .ok_or(ParseError::MissingElement("time.live-timestamp"))?;
    let timestamp = parse_timestamp_ms(timestamp_raw)?;

    // Body: <div class="usertext-body"> <div class="md">…</div></div>.
    // Limit to a child selector so nested comments' bodies don't bleed in.
    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let body_sel = Selector::parse(":scope > div.entry > form > div.usertext-body div.md").unwrap();
    let body_html = el
        .select(&body_sel)
        .next()
        .map(|d| d.inner_html())
        .unwrap_or_default();

    // Replies: nested .thing[data-fullname^=t1_] under div.child > div.listing.
    // Limit to direct children to avoid recursing into already-parsed nodes.
    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let reply_sel =
        Selector::parse(r#":scope > div.child > div.listing > div.thing.comment[data-fullname^="t1_"]"#)
            .unwrap();
    let mut replies = Vec::new();
    for child in el.select(&reply_sel) {
        replies.push(parse_comment_node(&child)?);
    }

    Ok(RawComment {
        id,
        author,
        body_html,
        score,
        timestamp,
        permalink,
        replies,
    })
}
