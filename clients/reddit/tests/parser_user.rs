//! Phase B unit tests — `parse_user_overview` against a real user
//! profile capture.

#![cfg(feature = "native")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_reddit::parser::user::parse_user_overview;

const OVERVIEW: &str = include_str!("fixtures/user_overview.html");

#[test]
fn parses_user_name_and_recent_items() {
    let prof = parse_user_overview(OVERVIEW).expect("user page parses");
    assert!(!prof.name.is_empty(), "user name extracted");
    // 25 mixed posts/comments was the count at capture.
    assert!(
        !prof.recent_items.is_empty(),
        "user overview has recent items"
    );
    assert!(prof.recent_items.len() <= 30, "shouldn't over-capture");
}
