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

// ─── Per-call selector factories ────────────────────────────────────────────
// `scraper::Selector` holds `Rc` and is therefore `!Sync`; `LazyLock` is not
// viable. Each call parses a static literal that can never fail.

// lint-allow-unused: static selector literal — infallible
#[allow(clippy::unwrap_used)]
fn top_comments_selector() -> Selector {
    Selector::parse(
        r#"div.commentarea > div.sitetable > div.thing.comment[data-fullname^="t1_"]"#,
    )
    .unwrap()
}

// lint-allow-unused: static selector literal — infallible
#[allow(clippy::unwrap_used)]
fn score_selector() -> Selector {
    Selector::parse("span.score.unvoted").unwrap()
}

// lint-allow-unused: static selector literal — infallible
#[allow(clippy::unwrap_used)]
fn live_timestamp_selector() -> Selector {
    Selector::parse("time.live-timestamp").unwrap()
}

// lint-allow-unused: static selector literal — infallible
#[allow(clippy::unwrap_used)]
fn comment_body_selector() -> Selector {
    Selector::parse(":scope > div.entry > form > div.usertext-body div.md").unwrap()
}

// lint-allow-unused: static selector literal — infallible
#[allow(clippy::unwrap_used)]
fn comment_reply_selector() -> Selector {
    Selector::parse(
        r#":scope > div.child > div.listing > div.thing.comment[data-fullname^="t1_"]"#,
    )
    .unwrap()
}

// ────────────────────────────────────────────────────────────────────────────

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
    let top_comments_sel = top_comments_selector();
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
    let score = el
        .select(&score_selector())
        .next()
        .and_then(|s| s.value().attr("title"))
        .and_then(|t| t.parse::<i64>().ok())
        .unwrap_or(0);

    // Timestamp: <time class="live-timestamp" datetime="...">.
    let timestamp_raw = el
        .select(&live_timestamp_selector())
        .next()
        .and_then(|t| t.value().attr("datetime"))
        .ok_or(ParseError::MissingElement("time.live-timestamp"))?;
    let timestamp = parse_timestamp_ms(timestamp_raw)?;

    // Body: <div class="usertext-body"> <div class="md">…</div></div>.
    // Limit to a child selector so nested comments' bodies don't bleed in.
    let body_html = el
        .select(&comment_body_selector())
        .next()
        .map(|d| d.inner_html())
        .unwrap_or_default();

    // Replies: nested .thing[data-fullname^=t1_] under div.child > div.listing.
    // Limit to direct children to avoid recursing into already-parsed nodes.
    let reply_sel = comment_reply_selector();
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

/// Parse a Reddit gallery JSON response (`/comments/<id>/.json`) and
/// return the ordered list of high-resolution image URLs.
///
/// Reddit galleries store the gallery layout under `gallery_data.items`
/// (an ordered list of `{caption, media_id, ...}`) and the actual image
/// URLs under `media_metadata.<media_id>.s.u`. The URLs are HTML-entity
/// encoded by reddit (`&amp;` for `&`); we decode them so reqwest /
/// browser fetch resolves them correctly.
///
/// Returns an empty `Vec` for non-gallery posts (the JSON shape is
/// missing both `gallery_data` and `media_metadata`).
///
/// # Errors
///
/// `ParseError::LoggedOut` if the JSON shape indicates the response was
/// a login redirect rather than a real post. Otherwise infallible —
/// missing fields silently yield an empty `Vec`.
pub fn parse_gallery_metadata(json: &serde_json::Value) -> Result<Vec<String>, ParseError> {
    // Reddit comments JSON is an array; the first element holds the post.
    // For galleries the post sits at [0].data.children[0].data.
    let post = json
        .get(0)
        .and_then(|v| v.get("data"))
        .and_then(|d| d.get("children"))
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("data"));
    let Some(post) = post else {
        return Ok(Vec::new());
    };

    // The ordered media_id list lives in gallery_data.items[].
    let items = post
        .get("gallery_data")
        .and_then(|g| g.get("items"))
        .and_then(|i| i.as_array());
    let Some(items) = items else {
        return Ok(Vec::new());
    };

    // Resolve each media_id → media_metadata[id].s.u (the source URL).
    let metadata = post.get("media_metadata");
    let mut urls = Vec::new();
    for item in items {
        let Some(media_id) = item.get("media_id").and_then(|m| m.as_str()) else {
            continue;
        };
        let url = metadata
            .and_then(|m| m.get(media_id))
            .and_then(|md| md.get("s"))
            .and_then(|s| s.get("u"))
            .and_then(|u| u.as_str());
        if let Some(url) = url {
            urls.push(url.replace("&amp;", "&"));
        }
    }
    Ok(urls)
}

#[cfg(test)]
mod gallery_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::parse_gallery_metadata;

    const GALLERY_JSON: &str =
        include_str!("../../tests/fixtures/comments_gallery_t3_1t22ox5.json");

    #[test]
    fn extracts_two_image_urls_from_real_gallery() {
        let json: serde_json::Value =
            serde_json::from_str(GALLERY_JSON).expect("fixture is valid json");
        let urls = parse_gallery_metadata(&json).expect("parses cleanly");
        assert_eq!(urls.len(), 2, "expected 2 gallery items");
        for url in &urls {
            assert!(
                url.starts_with("https://preview.redd.it/"),
                "expected preview.redd.it URL, got {url}"
            );
            assert!(
                !url.contains("&amp;"),
                "HTML entities should be decoded, got {url}"
            );
        }
    }

    #[test]
    fn empty_for_non_gallery_post() {
        let json: serde_json::Value =
            serde_json::from_str("[{\"data\":{\"children\":[{\"data\":{}}]}}]").unwrap();
        let urls = parse_gallery_metadata(&json).unwrap();
        assert!(urls.is_empty());
    }

    #[test]
    fn empty_for_malformed_json() {
        let json: serde_json::Value = serde_json::from_str("{}").unwrap();
        let urls = parse_gallery_metadata(&json).unwrap();
        assert!(urls.is_empty());
    }
}
