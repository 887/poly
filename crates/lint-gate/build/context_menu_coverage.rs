//! Stub for plan-context-menu-quality-control.md §3.1.2 coverage scan.
//!
//! Once the `#[context_menu(...)]` macro lands, this module will walk every
//! `#[component]` site and confirm that either:
//!   * it carries a `#[context_menu(...)]` attribute, or
//!   * its file sits in a pre-approved opt-out list (tests/, examples/,
//!     non-visual helpers, etc.).
//!
//! Until Phase B of the plan lands, the scanner is a no-op and lets the
//! build proceed.

use crate::baseline::Violation;
use crate::walk::WorkspaceWalker;

pub fn scan(_walker: &WorkspaceWalker, _violations: &mut Vec<Violation>) {
    // Phase A: empty — awaiting `#[context_menu]` rollout.
}
