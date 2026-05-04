//! poly-lint-gate — workspace-wide lint gate.
//!
//! All enforcement happens in `build.rs`; this library exists only because
//! cargo needs something to compile for the crate to be a dependency.
//! The scanner logic lives in `crates/lint-gate-rules` (poly-lint-gate-rules).

pub const VERSION: &str = "1";
