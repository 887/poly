//! poly-lint-gate — workspace-wide lint gate.
//!
//! All enforcement happens in `build.rs`; this library exists only because
//! cargo needs something to compile for the crate to be a dependency.
//! See `docs/plans/plan-component-lints.md`, `plan-context-menu-quality-control.md`,
//! and `plan-connected-routes-static-check.md`.

pub const VERSION: &str = "1";
