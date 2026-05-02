//! Parser for the message inbox — `/message/inbox/`.
//!
//! Each DM thread is a `<div class="message" data-fullname="t4_<id>">`
//! container. An empty inbox emits zero such containers; the page still
//! renders the compose-form template + sidebar nav. Empty result returns
//! `Ok(Vec::new())` — not an error.

#![cfg(feature = "native")]

use scraper::{ElementRef, Selector};

use super::{ParseError, RawDm, data_attr, parse_html, parse_timestamp_ms};

/// Parse every DM thread row from `/message/inbox/`.
///
/// # Errors
///
/// - `ParseError::LoggedOut` — login redirect.
/// - `ParseError::MissingElement` — a row was structurally invalid (no
///   author / no timestamp).
pub fn parse_inbox(html: &str) -> Result<Vec<RawDm>, ParseError> {
    let doc = parse_html(html)?;
    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let row_sel =
        Selector::parse(r#"div.message[data-fullname^="t4_"]"#).unwrap();
    let mut dms = Vec::new();
    for el in doc.select(&row_sel) {
        dms.push(parse_dm_row(&el)?);
    }
    Ok(dms)
}

fn parse_dm_row(el: &ElementRef<'_>) -> Result<RawDm, ParseError> {
    let id = data_attr(el, "data-fullname")
        .and_then(|v| v.strip_prefix("t4_"))
        .ok_or(ParseError::MissingElement("data-fullname (t4_)"))?
        .to_string();
    let author = data_attr(el, "data-author")
        .ok_or(ParseError::MissingElement("data-author"))?
        .to_string();

    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let subject_sel = Selector::parse("a.subject").unwrap();
    let subject = el
        .select(&subject_sel)
        .next()
        .map(|a| a.text().collect::<String>().trim().to_string())
        .unwrap_or_default();

    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let body_sel = Selector::parse("div.md").unwrap();
    let body_html = el
        .select(&body_sel)
        .next()
        .map(|d| d.inner_html())
        .unwrap_or_default();

    #[allow(clippy::unwrap_used)] // lint-allow-unused: static selector literal infallible
    let time_sel = Selector::parse("time.live-timestamp").unwrap();
    let timestamp_raw = el
        .select(&time_sel)
        .next()
        .and_then(|t| t.value().attr("datetime"))
        .ok_or(ParseError::MissingElement("time.live-timestamp"))?;
    let timestamp = parse_timestamp_ms(timestamp_raw)?;

    Ok(RawDm {
        id,
        author,
        subject,
        body_html,
        timestamp,
    })
}
