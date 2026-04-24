//! UI tests for `poly-lints`.
//!
//! Runs each `tests/ui/*.rs` fixture under dylint_testing and compares
//! the emitted diagnostics against the matching `.stderr` golden file.
//!
//! To regenerate `.stderr` files after intentional lint-output changes:
//!
//! ```sh
//! BLESS=1 cargo test --package poly-lints --test ui
//! ```
//!
//! These tests require a working nightly toolchain with `rustc-dev`
//! and `llvm-tools-preview`; see the crate-level rust-toolchain.toml.

#[test]
fn ui() {
    dylint_testing::ui_test(env!("CARGO_PKG_NAME"), "tests/ui");
}
