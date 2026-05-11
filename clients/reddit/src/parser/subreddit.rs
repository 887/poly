//! Parser for subreddit listing pages — `/r/<sub>/{hot,new,top,rising,controversial}/`.
//!
//! Extracts every `div.thing[data-fullname^="t3_"]` from the listing
//! using `data-*` attribute access (more stable than text scraping).

#![cfg(feature = "native")]

use scraper::{ElementRef, Html, Selector};

use super::{ParseError, RawPost, data_attr, parse_html, parse_timestamp_ms};

/// Parse every post container in a subreddit listing into `RawPost`s.
///
/// Empty listings (banned subreddits, subscriber-only quarantines on
/// non-quarantined fetches) return `Ok(Vec::new())` — not an error,
/// the caller decides whether to surface that as a UI message.
///
/// # Errors
///
/// - `ParseError::LoggedOut` — the response is the login page (cookie
///   missing or expired).
/// - `ParseError::MalformedInt` / `MalformedTimestamp` — a post had
///   garbage in a `data-*` field that we expected to parse cleanly. The
///   first malformed post short-circuits the whole listing.
pub fn parse_listing(html: &str) -> Result<Vec<RawPost>, ParseError> {
    let doc = parse_html(html)?;
    parse_listing_from_doc(&doc)
}

pub(crate) fn parse_listing_from_doc(doc: &Html) -> Result<Vec<RawPost>, ParseError> {
    let thing_sel = post_selector();
    let mut posts = Vec::new();
    for el in doc.select(&thing_sel) {
        posts.push(parse_post_thing(&el)?);
    }
    Ok(posts)
}

/// Pull the `after=<t3_id>` token out of old.reddit's
/// `<span class="next-button"><a href="...?count=25&after=t3_xxx">next</a></span>`
/// if present. Returns `None` when there is no next-button (final page).
#[must_use]
pub fn extract_next_after(html: &str) -> Option<String> {
    // Locate the next-button span and the first href= within ~512 chars.
    let span_idx = html.find("\"next-button\"")?;
    let window_end = (span_idx + 512).min(html.len());
    let window = &html[span_idx..window_end];
    let href_idx = window.find("href=\"")?;
    let after_href = &window[href_idx + 6..];
    let close = after_href.find('"')?;
    let href = &after_href[..close];
    // href may include the literal `&amp;` from HTML escaping; both
    // shapes can carry an after=... query param.
    let after_marker = href.find("after=")?;
    let raw = &href[after_marker + "after=".len()..];
    let end = raw
        .find(|c: char| c == '&' || c == '#')
        .unwrap_or(raw.len());
    let token = &raw[..end];
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

pub(crate) fn post_selector() -> Selector {
    // Lints: parser-internal selector strings are static and known-good.
    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    {
        Selector::parse(r#"div.thing[data-fullname^="t3_"]"#).unwrap()
    }
}

pub(crate) fn parse_post_thing(el: &ElementRef<'_>) -> Result<RawPost, ParseError> {
    let id = data_attr(el, "data-fullname")
        .and_then(|v| v.strip_prefix("t3_"))
        .ok_or(ParseError::MissingElement("data-fullname (t3_)"))?
        .to_string();
    let author = data_attr(el, "data-author")
        .ok_or(ParseError::MissingElement("data-author"))?
        .to_string();
    let subreddit = data_attr(el, "data-subreddit")
        .ok_or(ParseError::MissingElement("data-subreddit"))?
        .to_string();
    let score = data_attr(el, "data-score")
        .ok_or(ParseError::MissingElement("data-score"))?
        .parse::<i64>()
        .map_err(|_| ParseError::MalformedInt("data-score"))?;
    let timestamp_raw = data_attr(el, "data-timestamp")
        .ok_or(ParseError::MissingElement("data-timestamp"))?;
    let timestamp = parse_timestamp_ms(timestamp_raw)?;
    let permalink = data_attr(el, "data-permalink")
        .ok_or(ParseError::MissingElement("data-permalink"))?
        .to_string();
    let comment_count = data_attr(el, "data-comments-count")
        .unwrap_or("0")
        .parse::<u32>()
        .map_err(|_| ParseError::MalformedInt("data-comments-count"))?;
    let url = data_attr(el, "data-url").map(str::to_owned);

    // Title is in <a class="title may-blank ..."> inside <p class="title">.
    // Selector::parse is infallible on a static literal.
    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let title_sel = Selector::parse("a.title").unwrap();
    let title = el
        .select(&title_sel)
        .next()
        .map(|a| a.text().collect::<String>().trim().to_string())
        .ok_or(ParseError::MissingElement("a.title"))?;

    // Self-post body is in <div class="md"> when present (link posts
    // don't have one).
    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let body_sel = Selector::parse("div.usertext-body div.md").unwrap();
    let body = el
        .select(&body_sel)
        .next()
        .map(|d| d.inner_html().trim().to_string())
        .filter(|s| !s.is_empty());

    // ── Media detection ─────────────────────────────────────────────────────

    let domain = data_attr(el, "data-domain").unwrap_or("");

    // Image domains where `data-url` itself is a renderable image.
    let is_image_domain = matches!(
        domain,
        "i.redd.it" | "i.imgur.com" | "imgur.com" | "preview.redd.it"
    );
    // URL-extension check for cases where the domain alone isn't conclusive
    // (e.g. a post with `data-domain="i.imgur.com"` always has an image URL,
    // but some posts on `imgur.com` only link to an album).
    let url_is_image = url.as_deref().is_some_and(|u| {
        let lower = u.to_lowercase();
        lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".png")
            || lower.ends_with(".gif")
            || lower.ends_with(".webp")
    });

    // Video domains — `v.redd.it` requires the HLS manifest, but other
    // external video hosts link to an embeddable player page.
    let is_video = matches!(
        domain,
        "v.redd.it" | "youtu.be" | "youtube.com" | "vimeo.com" | "gfycat.com"
            | "streamable.com"
    );

    // Gallery: `data-is-gallery="true"`.
    let is_gallery = data_attr(el, "data-is-gallery").is_some_and(|v| v == "true");

    // Build preview_url:
    // • For image posts, use `data-url` directly (it IS the image).
    // • For video posts, attempt to find the listing thumbnail <img>.
    // • Gallery posts: use data-url as a cover preview (single-image stub).
    let preview_url: Option<String> = if is_image_domain || url_is_image {
        url.clone()
    } else if is_video || is_gallery {
        // Try the listing thumbnail link: `<a class="thumbnail ..."><img src="..."></a>`
        #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
        let thumb_sel = Selector::parse("a.thumbnail img").unwrap();
        el.select(&thumb_sel)
            .next()
            .and_then(|img| img.value().attr("src"))
            .filter(|s| !s.is_empty() && !s.contains("//www.redditstatic.com/icon"))
            .map(str::to_owned)
    } else {
        None
    };

    Ok(RawPost {
        id,
        author,
        subreddit,
        score,
        timestamp,
        title,
        body,
        permalink,
        comment_count,
        url,
        preview_url,
        is_video,
        is_gallery,
    })
}
