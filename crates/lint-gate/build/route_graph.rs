//! Stub for plan-connected-routes-static-check.md §3 reachability check.
//!
//! Once `#[connected(...)]` + `nav!` + `ProgrammaticProducer` impls are in
//! place, this scanner will:
//!   * collect every `Route` variant from `crates/core/src/router.rs`,
//!   * collect every `nav!` / `Link { to: }` call site,
//!   * collect every `ProgrammaticProducer` impl,
//!   * BFS from the `entry_point` marker across the union graph, and
//!   * emit a violation for any route with `in_degree == 0`.
//!
//! Until Phase B of the plan lands this is a no-op.

use std::path::Path;

use crate::baseline::Violation;

pub fn scan(_ws_root: &Path, _violations: &mut Vec<Violation>) {
    // Phase A: empty — awaiting `#[connected]` rollout.
}
