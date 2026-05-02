//! Live integration tests against `old.reddit.com` for the anonymous
//! read flows. Gated `#[ignore]` so CI doesn't depend on the live
//! internet — manual run:
//!
//! ```
//! cargo test -p poly-reddit --test integration_anonymous_read -- --ignored
//! ```
//!
//! These tests confirm that:
//! 1. The default UA still passes Reddit's anti-bot wall.
//! 2. The endpoint shapes (paths + 301-to-canonical-slug behaviour)
//!    we built into Phase D are still what Reddit serves.
//! 3. The parsers eat real wire HTML, not just our captured fixtures.
//!
//! If any of these fail in the future, the parser fixtures are the
//! wrong shape and need re-capture (Phase F.2 cycle).

#![cfg(feature = "native")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_reddit::{RedditClient, SortKind};

#[tokio::test]
#[ignore = "live network — run manually with --ignored"]
async fn lists_r_rust_hot_anonymously() {
    let client = RedditClient::new().expect("client builds");
    let posts = client
        .list_subreddit("rust", SortKind::Hot)
        .await
        .expect("hot listing fetches and parses");
    assert!(!posts.is_empty(), "r/rust hot should have posts");
    for p in &posts {
        assert_eq!(p.subreddit, "rust");
        assert!(!p.id.is_empty());
        assert!(!p.author.is_empty());
        assert!(p.timestamp.timestamp() > 1_500_000_000);
    }
}

#[tokio::test]
#[ignore = "live network — run manually with --ignored"]
async fn fetches_a_post_with_comments_anonymously() {
    let client = RedditClient::new().expect("client builds");
    // First find a real post in r/rust/hot
    let posts = client
        .list_subreddit("rust", SortKind::Hot)
        .await
        .expect("hot listing fetches");
    let post = posts.first().expect("at least one post");
    // Then drill in
    let (op, comments) = client
        .get_post(&post.id)
        .await
        .expect("post fetch + parse");
    assert_eq!(op.id, post.id, "OP id matches the listing entry");
    // Comment count is non-strict — could be 0 for a brand-new post.
    eprintln!(
        "fetched post {} ({} top-level comments)",
        op.id,
        comments.len()
    );
}

#[tokio::test]
#[ignore = "live network — run manually with --ignored"]
async fn fetches_a_user_overview_anonymously() {
    let client = RedditClient::new().expect("client builds");
    // Fetch posts first to grab a real author handle
    let posts = client
        .list_subreddit("rust", SortKind::New)
        .await
        .expect("new listing fetches");
    let author = &posts.first().expect("at least one post").author;
    let profile = client
        .get_user(author)
        .await
        .expect("user fetch + parse");
    assert!(!profile.name.is_empty());
    eprintln!("fetched user {} with {} recent items", profile.name, profile.recent_items.len());
}

#[tokio::test]
#[ignore = "live network — run manually with --ignored"]
async fn each_sort_returns_results() {
    let client = RedditClient::new().expect("client builds");
    for sort in [
        SortKind::Hot,
        SortKind::New,
        SortKind::Top,
        SortKind::Rising,
    ] {
        let posts = client
            .list_subreddit("rust", sort)
            .await
            .unwrap_or_else(|e| panic!("{sort:?} fetch failed: {e}"));
        assert!(!posts.is_empty(), "{sort:?} should have posts");
        // Be polite — 1s between requests.
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}
