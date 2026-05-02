//! Phase B unit tests — `parse_inbox` against the empty-inbox fixture.
//! The "with DMs" populated fixture is deferred (see plan F.2 findings —
//! `/api/compose` is dead, populating requires OAuth or a second
//! account).

#![cfg(feature = "native")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_reddit::parser::inbox::parse_inbox;

const EMPTY: &str = include_str!("fixtures/inbox_empty.html");

#[test]
fn empty_inbox_returns_zero_dms_no_error() {
    let dms = parse_inbox(EMPTY).expect("empty inbox parses");
    assert!(dms.is_empty(), "fresh-account inbox should be empty");
}

#[test]
fn inbox_logged_out_short_circuits() {
    use poly_reddit::parser::ParseError;
    let html = include_str!("fixtures/login_redirect.html");
    assert_eq!(parse_inbox(html).unwrap_err(), ParseError::LoggedOut);
}
