#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile-fail/*.rs");
}
