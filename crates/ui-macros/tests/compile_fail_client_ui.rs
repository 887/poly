//! Compile-fail trybuild fixtures for new client-UI lints.
//!
//! Fixtures live under tests/compile-fail-client-ui/ and are added incrementally:
//! - WP 1: ftl-label-key-missing, action-id-not-kebab, backend-missing-export
//! - WP 5: custom-block-contains-script

#[test]
#[ignore = "WP 1: fixtures not yet created"]
fn client_ui_compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile-fail-client-ui/*.rs");
}
