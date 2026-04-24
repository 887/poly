//! # poly-lints — HIR-precise dylint lints for CLAUDE.md hang classes
//!
//! Two lints:
//!
//! - [`RAW_SIGNAL_WRITE`] — CLAUDE.md hang class #1
//! - [`USE_EFFECT_SPAWN_CYCLE`] — CLAUDE.md hang class #3
//!
//! Both have regex-based counterparts in `tools/scripts/forbid-*.sh`
//! that ship as the authoritative CI gate (Phase 5 Track A). This
//! crate is Phase 5 Track B: an AST-precise alternative that resolves
//! the `Signal` / `BatchedSignal` types via their canonical DefPath
//! and visits the HIR of `use_effect` closures to check for the
//! hang-triple pattern.
//!
//! ## Scope
//!
//! The lints only reason about:
//!
//! - method calls resolved to `dioxus_signals::Signal::<T>::write` /
//!   `dioxus_signals::ReadOnlySignal::write` (for `RAW_SIGNAL_WRITE`);
//! - method calls resolved to `dioxus_hooks::use_effect` whose closure
//!   body contains a `dioxus_hooks::spawn` call whose future body
//!   writes a `Signal` / `BatchedSignal` captured from the outer scope
//!   (for `USE_EFFECT_SPAWN_CYCLE`).
//!
//! ## Allowlist
//!
//! A site is silenced by `#[allow(poly::raw_signal_write)]` or
//! `#[allow(poly::use_effect_spawn_cycle)]` on the enclosing function
//! or `impl`. By convention, attach a `// reason: …` comment next to
//! every allow; the lint does NOT enforce the reason comment (the
//! existing regex allowlist files already document allowlisted sites
//! and this lint is additive, not replacement).
//!
//! See `tools/lints/poly-lints/README.md` for allowlist conventions
//! and known divergences from the regex heuristics.

#![feature(rustc_private)]
#![warn(unused_extern_crates)]

extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_lint;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;

mod raw_signal_write;
mod use_effect_spawn_cycle;

dylint_linting::dylint_library!();

#[unsafe(no_mangle)]
pub fn register_lints(
    sess: &rustc_session::Session,
    lint_store: &mut rustc_lint::LintStore,
) {
    dylint_linting::init_config(sess);

    lint_store.register_lints(&[
        raw_signal_write::RAW_SIGNAL_WRITE,
        use_effect_spawn_cycle::USE_EFFECT_SPAWN_CYCLE,
    ]);

    lint_store
        .register_late_pass(|_| Box::new(raw_signal_write::RawSignalWrite));
    lint_store.register_late_pass(|_| {
        Box::new(use_effect_spawn_cycle::UseEffectSpawnCycle)
    });
}
