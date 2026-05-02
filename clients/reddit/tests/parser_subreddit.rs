//! Phase B unit tests — `parse_listing` against captured fixtures.
//!
//! Fixtures live in `clients/reddit/tests/fixtures/` (see
//! `docs/plans/plan-reddit-stub.md` Phase F.2).

#![cfg(feature = "native")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_reddit::parser::subreddit::parse_listing;

const HOT: &str = include_str!("fixtures/r_rust_hot.html");
const NEW: &str = include_str!("fixtures/r_rust_new.html");
const TOP: &str = include_str!("fixtures/r_rust_top.html");

#[test]
fn parses_25_posts_from_rust_hot() {
    let posts = parse_listing(HOT).expect("hot listing parses");
    assert_eq!(posts.len(), 25, "r/rust hot should have 25 posts");
    let first = &posts[0];
    assert!(!first.id.is_empty(), "first post has an id");
    assert!(!first.author.is_empty(), "first post has an author");
    assert_eq!(first.subreddit, "rust");
    assert!(!first.title.is_empty(), "first post has a title");
    assert!(first.permalink.starts_with("/r/rust/comments/"));
}

#[test]
fn parses_25_posts_from_rust_new() {
    let posts = parse_listing(NEW).expect("new listing parses");
    assert_eq!(posts.len(), 25);
    for p in &posts {
        assert_eq!(p.subreddit, "rust");
        assert!(p.score >= -100, "score parse sanity (no garbage)");
        // Timestamp should be from the past — use a very loose bound to
        // tolerate fixture age (fixtures captured 2026-05-02; posts can
        // span months back).
        assert!(p.timestamp.timestamp() > 1_500_000_000, "timestamp is post-2017");
    }
}

#[test]
fn parses_18_posts_from_rust_top() {
    let posts = parse_listing(TOP).expect("top listing parses");
    assert_eq!(posts.len(), 18);
    // Top sort should have higher scores on average than hot — sanity
    // check by asserting the top entry has score > 100.
    assert!(posts[0].score > 100, "top entry should have a real score");
}

#[test]
fn detects_logged_out_on_login_page() {
    use poly_reddit::parser::ParseError;
    let html = include_str!("fixtures/login_redirect.html");
    let err = parse_listing(html).unwrap_err();
    assert_eq!(err, ParseError::LoggedOut);
}
