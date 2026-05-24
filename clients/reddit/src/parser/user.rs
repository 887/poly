//! Parser for `/user/<u>/` overview pages.
//!
//! User overview is a mixed listing of the user's submissions (`t3_`)
//! and comments (`t1_`), interleaved chronologically. Profile-level
//! fields (avatar, karma) come from sidebar elements rather than
//! per-thing data attributes.

#![cfg(feature = "native")]

use scraper::Selector;

use super::{
    ParseError, RawComment, UserOverviewItem, UserProfile, data_attr, parse_html,
    parse_timestamp_ms, subreddit::parse_post_thing,
};

// ─── Per-call selector factories ────────────────────────────────────────────
// `scraper::Selector` holds `Rc` and is therefore `!Sync`; `LazyLock` is not
// viable. Each call parses a static literal that can never fail.

// lint-allow-unused: static selector literal — infallible
#[allow(clippy::unwrap_used)]
fn page_title_selector() -> Selector {
    Selector::parse("title").unwrap()
}

// lint-allow-unused: static selector literal — infallible
#[allow(clippy::unwrap_used)]
fn avatar_selector() -> Selector {
    Selector::parse("img.profile-img").unwrap()
}

// lint-allow-unused: static selector literal — infallible
#[allow(clippy::unwrap_used)]
fn user_thing_selector() -> Selector {
    Selector::parse(
        r#"div.thing[data-fullname^="t3_"], div.thing.comment[data-fullname^="t1_"]"#,
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
    Selector::parse("div.usertext-body div.md").unwrap()
}
// ────────────────────────────────────────────────────────────────────────────

/// Parse a user overview page into the profile + interleaved items.
///
/// # Errors
///
/// - `ParseError::LoggedOut` — login redirect.
/// - `ParseError::MalformedInt` / `MalformedTimestamp` — bubbled from
///   per-thing parsing.
pub fn parse_user_overview(html: &str) -> Result<UserProfile, ParseError> {
    let doc = parse_html(html)?;

    // Username: <span class="user"><a href="/user/<name>">name</a></span>
    // OR <span class="pagename">u/name</span> — fall back to <title>.
    let title = doc
        .select(&page_title_selector())
        .next()
        .map(|t| t.text().collect::<String>())
        .unwrap_or_default();
    // Title format: "overview for <username>" or "<username>'s posts".
    let name = title
        .trim()
        .strip_prefix("overview for ")
        .or_else(|| title.trim().strip_suffix("'s posts"))
        .unwrap_or(title.trim())
        .to_string();

    // Avatar: <img class="profile-img" src="..."> in the sidebar.
    let avatar_url = doc
        .select(&avatar_selector())
        .next()
        .and_then(|img| img.value().attr("src"))
        .map(str::to_owned);

    // Items: every .thing on the page, classified by data-type or by
    // the data-fullname prefix.
    let thing_sel = user_thing_selector();
    let mut recent_items = Vec::new();
    for el in doc.select(&thing_sel) {
        let fullname = el
            .value()
            .attr("data-fullname")
            .ok_or(ParseError::MissingElement("data-fullname"))?;
        if fullname.starts_with("t3_") {
            recent_items.push(UserOverviewItem::Post(parse_post_thing(&el)?));
        } else {
            // Comment on a user overview page: parse locally — overview
            // pages don't nest replies under each comment, so treat
            // each as a flat item.
            recent_items.push(UserOverviewItem::Comment(parse_user_comment(&el)?));
        }
    }

    Ok(UserProfile {
        name,
        avatar_url,
        recent_items,
    })
}

fn parse_user_comment(el: &scraper::ElementRef<'_>) -> Result<RawComment, ParseError> {
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

    let score = el
        .select(&score_selector())
        .next()
        .and_then(|s| s.value().attr("title"))
        .and_then(|t| t.parse::<i64>().ok())
        .unwrap_or(0);

    let timestamp_raw = el
        .select(&live_timestamp_selector())
        .next()
        .and_then(|t| t.value().attr("datetime"))
        .ok_or(ParseError::MissingElement("time.live-timestamp"))?;
    let timestamp = parse_timestamp_ms(timestamp_raw)?;

    let body_html = el
        .select(&comment_body_selector())
        .next()
        .map(|d| d.inner_html())
        .unwrap_or_default();

    Ok(RawComment {
        id,
        author,
        body_html,
        score,
        timestamp,
        permalink,
        replies: Vec::new(),
    })
}
