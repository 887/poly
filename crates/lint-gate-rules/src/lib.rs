//! poly-lint-gate-rules — shared scanner library for the workspace lint gate.
//!
//! Contains:
//!  - [`Violation`] and [`Baseline`] types (shared data types)
//!  - [`WorkspaceWalker`] for .gitignore-aware workspace file enumeration
//!  - [`allowlist`] module — shared allowlist loader for all scanners
//!  - 9 build-time scanners (ported from `crates/lint-gate/build/`)
//!  - 9 bash-script lints (ported from `tools/scripts/forbid-*.sh`)
//!  - [`all_rules`] — run every scanner and collect violations
//!
//! # Lint allows
//!
//! The scanners parse ASCII Rust source using string slicing on ASCII bytes,
//! `as`-cast line/column numbers, and integer arithmetic for offsets/counters.
//! This is intentional — each scanner runs once per `cargo check`, and
//! overflow/truncation are unreachable in practice on realistic source files.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::string_slice,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::default_numeric_fallback,
    clippy::integer_division,
    clippy::indexing_slicing,
)]
#![cfg_attr(test, allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
))]

pub mod allowlist;
pub mod violation;
pub mod walk;

// Build-time scanners (ported from crates/lint-gate/build/)
pub mod action_enum_coverage;
pub mod action_id_naming;
pub mod allow_ban;
pub mod context_menu_coverage;
pub mod custom_block_usage;
pub mod forbid_backend_slug_match;
pub mod ftl_label_key_coverage;
pub mod nav_push_ban;
pub mod route_graph;
pub mod ui_action_coverage;

// Bash-script lints (ported from tools/scripts/forbid-*.sh)
pub mod forbid_cross_persona_memory;
pub mod forbid_effect_self_write;
pub mod forbid_long_read_guard;
pub mod forbid_raw_backend_read;
pub mod forbid_render_time_read;
pub mod forbid_signal_write;
pub mod forbid_stale_effect_capture;
pub mod forbid_unaudited_persona_tool;
pub mod forbid_use_effect_spawn_cycle;

pub use violation::Violation;
pub use walk::WorkspaceWalker;

use std::path::Path;

/// Run all scanners and return every violation found.
///
/// This is the entry point used by `crates/lint-gate/build.rs`.
pub fn all_rules(walker: &WorkspaceWalker, ws_root: &Path) -> Vec<Violation> {
    let mut violations = Vec::new();

    // Build-time scanners.
    allow_ban::scan(walker, &mut violations);
    action_enum_coverage::scan(walker, &mut violations);
    action_id_naming::scan(walker, &mut violations);
    context_menu_coverage::scan(walker, &mut violations);
    custom_block_usage::scan(walker, &mut violations);
    forbid_backend_slug_match::scan(walker, &mut violations);
    ftl_label_key_coverage::scan(walker, &mut violations);
    nav_push_ban::scan(walker, &mut violations);
    route_graph::scan(ws_root, &mut violations);
    ui_action_coverage::scan(walker, &mut violations);

    // Bash-script lints.
    forbid_signal_write::scan(walker, ws_root, &mut violations);
    forbid_cross_persona_memory::scan(walker, ws_root, &mut violations);
    forbid_effect_self_write::scan(walker, ws_root, &mut violations);
    forbid_long_read_guard::scan(walker, ws_root, &mut violations);
    forbid_raw_backend_read::scan(walker, ws_root, &mut violations);
    forbid_render_time_read::scan(walker, ws_root, &mut violations);
    forbid_stale_effect_capture::scan(walker, ws_root, &mut violations);
    forbid_unaudited_persona_tool::scan(walker, ws_root, &mut violations);
    forbid_use_effect_spawn_cycle::scan(walker, ws_root, &mut violations);

    violations
}
