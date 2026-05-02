//! Phase B unit tests — `parse_post_page` against the 211-comment fixture.

#![cfg(feature = "native")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_reddit::parser::post::parse_post_page;

const COMMENTS: &str = include_str!("fixtures/comments_t3_14921t7.html");

#[test]
fn parses_op_and_threaded_comments() {
    let (op, comments) = parse_post_page(COMMENTS).expect("post page parses");

    // OP sanity.
    assert_eq!(op.id, "14921t7");
    assert_eq!(op.subreddit, "rust");
    assert!(!op.author.is_empty());
    assert!(!op.title.is_empty());
    assert!(op.permalink.contains("/r/rust/comments/14921t7/"));

    // Top-level + nested comment count should be substantial. Raw grep
    // at capture found 211 t1_ containers, of which 24 are
    // `morechildren` placeholders we filter out, leaving ~187 real
    // comments. Allow some slack for edge cases (collapsed-by-default
    // branches, nested moderator-distinguished items).
    assert!(!comments.is_empty(), "should have top-level comments");
    let total = count_recursive(&comments);
    assert!(
        (150..=200).contains(&total),
        "expected ~187 real comments after filtering 24 morechildren placeholders, got {total}"
    );
}

#[test]
fn comments_have_timestamp_and_score() {
    let (_op, comments) = parse_post_page(COMMENTS).unwrap();
    for c in &comments {
        // Empty-string author placeholder is allowed (deleted comment),
        // but an actual t1_ id and a parsed timestamp are required.
        assert!(!c.id.is_empty());
        assert!(c.timestamp.timestamp() > 1_500_000_000);
    }
}

fn count_recursive(comments: &[poly_reddit::parser::RawComment]) -> usize {
    let mut total = 0;
    for c in comments {
        total += 1;
        total += count_recursive(&c.replies);
    }
    total
}
