//! Unit tests for mapping (split out per Pack E.2 §1.2 layer a — B.2).

// lint-allow-unused: test file — unwrap/expect/panic are idiomatic assertion mechanics
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::types::{HnFeed, HnItem, HnItemType};

/// The score / comment-count / timestamp triple every fixture story carries.
/// Bundled so the builders below stay under the 6-argument limit.
#[derive(Clone, Copy)]
struct Stats {
    score: u32,
    descendants: u32,
    time: u64,
}

fn story(id: u64, title: &str, url: Option<&str>, by: &str, stats: Stats) -> HnItem {
    HnItem {
        id,
        item_type: HnItemType::Story,
        by: Some(by.to_string()),
        time: Some(stats.time),
        text: None,
        url: url.map(str::to_string),
        title: Some(title.to_string()),
        score: Some(stats.score),
        descendants: Some(stats.descendants),
        kids: None,
        parent: None,
        dead: None,
        deleted: None,
    }
}

fn ask_hn_story(id: u64, title: &str, text: &str, by: &str, stats: Stats) -> HnItem {
    HnItem {
        id,
        item_type: HnItemType::Story,
        by: Some(by.to_string()),
        time: Some(stats.time),
        text: Some(text.to_string()),
        url: None,
        title: Some(title.to_string()),
        score: Some(stats.score),
        descendants: Some(stats.descendants),
        kids: None,
        parent: None,
        dead: None,
        deleted: None,
    }
}

// -- Feed ID mapping tests --

#[test]
fn feed_channel_ids_roundtrip() {
    let pairs = [
        (HnFeed::Top, "hn-top"),
        (HnFeed::New, "hn-new"),
        (HnFeed::Best, "hn-best"),
        (HnFeed::Ask, "hn-ask"),
        (HnFeed::Show, "hn-show"),
        (HnFeed::Jobs, "hn-jobs-ch"),
    ];
    for (feed, expected_id) in &pairs {
        assert_eq!(feed.channel_id(), *expected_id, "channel_id mismatch for {feed:?}");
        let parsed = HnFeed::from_channel_id(expected_id);
        assert_eq!(parsed, Some(*feed), "from_channel_id failed for {expected_id}");
    }
}

#[test]
fn feed_unknown_channel_id_returns_none() {
    assert!(HnFeed::from_channel_id("hn-forum-unknown").is_none());
}

// -- ViewRow mapping tests --

#[test]
fn map_story_with_url_to_viewrow() {
    let item = story(
        42,
        "Rust 2.0 Released",
        Some("https://example.com"),
        "pg",
        Stats { score: 500, descendants: 200, time: 1_700_000_000 },
    );
    let row = hn_item_to_view_row(&item);

    assert_eq!(row.id, "42");
    assert_eq!(row.primary_text, "Rust 2.0 Released");
    assert_eq!(row.secondary_text.as_deref(), Some("https://example.com"));
    let meta = row.meta_text.expect("meta_text must be Some");
    assert!(meta.contains("500pt"), "meta should contain score: {meta}");
    assert!(meta.contains("200 comments"), "meta should contain descendants: {meta}");
}

#[test]
fn map_ask_hn_story_secondary_is_by() {
    // Ask HN stories have no URL — secondary_text should be "by <author>"
    let item = ask_hn_story(
        99,
        "Ask HN: Best Rust books?",
        "Post body here",
        "tptacek",
        Stats { score: 200, descendants: 80, time: 1_700_000_000 },
    );
    let row = hn_item_to_view_row(&item);

    assert_eq!(row.id, "99");
    assert_eq!(row.primary_text, "Ask HN: Best Rust books?");
    assert_eq!(row.secondary_text.as_deref(), Some("by tptacek"));
}

#[test]
fn map_story_id_is_string() {
    let item = story(
        1234567,
        "Test",
        None,
        "anon",
        Stats { score: 1, descendants: 0, time: 1_700_000_000 },
    );
    let row = hn_item_to_view_row(&item);
    assert_eq!(row.id, "1234567");
}

// -- humanize_age tests --

#[test]
fn humanize_age_none_returns_question_mark() {
    assert_eq!(humanize_age(None), "?");
}

#[test]
fn humanize_age_recent() {
    use chrono::Utc;
    let now = u64::try_from(Utc::now().timestamp()).expect("wall clock is after the unix epoch");
    // 30 seconds ago
    assert_eq!(humanize_age(Some(now - 30)), "30s");
    // 90 seconds = 1 minute
    assert_eq!(humanize_age(Some(now - 90)), "1m");
    // 2 hours
    assert_eq!(humanize_age(Some(now - 7200)), "2h");
    // 3 days
    assert_eq!(humanize_age(Some(now - 86_400 * 3)), "3d");
}
