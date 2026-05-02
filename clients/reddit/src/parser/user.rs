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
    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let title_sel = Selector::parse("title").unwrap();
    let title = doc
        .select(&title_sel)
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
    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let avatar_sel = Selector::parse("img.profile-img").unwrap();
    let avatar_url = doc
        .select(&avatar_sel)
        .next()
        .and_then(|img| img.value().attr("src"))
        .map(str::to_owned);

    // Items: every .thing on the page, classified by data-type or by
    // the data-fullname prefix.
    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let thing_sel = Selector::parse(
        r#"div.thing[data-fullname^="t3_"], div.thing.comment[data-fullname^="t1_"]"#,
    )
    .unwrap();
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

    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let score_sel = Selector::parse("span.score.unvoted").unwrap();
    let score = el
        .select(&score_sel)
        .next()
        .and_then(|s| s.value().attr("title"))
        .and_then(|t| t.parse::<i64>().ok())
        .unwrap_or(0);

    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let time_sel = Selector::parse("time.live-timestamp").unwrap();
    let timestamp_raw = el
        .select(&time_sel)
        .next()
        .and_then(|t| t.value().attr("datetime"))
        .ok_or(ParseError::MissingElement("time.live-timestamp"))?;
    let timestamp = parse_timestamp_ms(timestamp_raw)?;

    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let body_sel = Selector::parse("div.usertext-body div.md").unwrap();
    let body_html = el
        .select(&body_sel)
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
