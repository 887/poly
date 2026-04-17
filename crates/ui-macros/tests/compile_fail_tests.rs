//! Trybuild compile-fail tests for `#[connected(...)]` and `#[context_menu(...)]`
//! attribute macro validators.
//!
//! Per plan-connected-routes-static-check.md §7.2 and
//! plan-context-menu-quality-control.md (§2.1 macro validation).
//!
//! Each fixture under `tests/compile-fail/` must produce the error recorded in
//! the matching `.stderr` file. Run `TRYBUILD=overwrite cargo test -p poly-ui-macros`
//! to regenerate `.stderr` snapshots after changing error messages.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile-fail/*.rs");
}
