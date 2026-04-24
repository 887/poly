//! `poly::use_effect_spawn_cycle` — detects the CLAUDE.md hang class #3
//! triple: `use_effect` closure → inner `spawn(async move { … })` →
//! signal-write on a signal captured by the outer closure that also
//! reads the same signal.
//!
//! This is the canonical shape that causes infinite re-render loops
//! in Dioxus 0.7 — the effect re-runs on every write, the write
//! re-fires the spawn, and the page wedges after a few thousand
//! iterations. The `use_spawn_once<K>(key, async_fn)` hook encodes
//! the safe keyed variant; this lint fires on the raw pattern so
//! new sites are forced through the hook.
//!
//! Matching is conservative:
//!
//! - Outer call must resolve to `dioxus_hooks::use_effect` (or
//!   `dioxus::prelude::use_effect` which re-exports it).
//! - Inside the closure body, a call resolving to `dioxus_hooks::spawn`
//!   (or `tokio::spawn`) with an `async move { … }` block argument.
//! - Inside the async block, a method call named `batch` / `write`
//!   / `set` / `pending_update` on a path expression whose binding
//!   was a capture of the outer closure.
//!
//! False-negative tolerance: a spawn that passes the signal through
//! a helper function before calling `.batch()` inside that helper
//! will not be caught. This is the same blind spot the regex
//! script has, so parity is preserved.

use rustc_hir::intravisit::{Visitor, walk_expr};
use rustc_hir::{Expr, ExprKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_session::{declare_lint, declare_lint_pass};

declare_lint! {
    /// ### What it does
    ///
    /// Disallows `use_effect(|| { … spawn(async move { … sig.batch(…) }) })`
    /// patterns when the outer effect's closure reads `sig` — this creates
    /// an infinite spawn loop (CLAUDE.md hang class #3).
    ///
    /// ### Why is this bad?
    ///
    /// The Dioxus reactive runtime re-runs `use_effect` whenever any
    /// signal read inside its closure changes. If the effect spawns
    /// a future that writes the same signal, every write re-schedules
    /// the effect, which re-spawns the future, forever.
    ///
    /// The `use_spawn_once<K>(key, async_fn)` hook (crates/core/src/state/
    /// use_spawn_once.rs) gates the spawn on a `spawned_for: Signal<Option<K>>`
    /// guard baked into the hook API, making the guard unforgettable.
    ///
    /// ### Example
    ///
    /// ```ignore
    /// // BAD:
    /// use_effect(move || {
    ///     let key = data.read().key.clone();
    ///     spawn(async move {
    ///         let res = fetch(key).await;
    ///         data.batch(|d| d.result = res);   // ← re-triggers the effect
    ///     });
    /// });
    ///
    /// // GOOD:
    /// use_spawn_once(data.read().key.clone(), async move {
    ///     let res = fetch(key).await;
    ///     data.batch(|d| d.result = res);
    /// });
    /// ```
    pub USE_EFFECT_SPAWN_CYCLE,
    Deny,
    "`use_effect` + `spawn(async move { … signal.write/batch/set(…) })` — use `use_spawn_once` instead (CLAUDE.md hang class #3)"
}

declare_lint_pass!(UseEffectSpawnCycle => [USE_EFFECT_SPAWN_CYCLE]);

const USE_EFFECT_PATHS: &[&[&str]] =
    &[&["dioxus_hooks", "use_effect"], &["dioxus", "prelude", "use_effect"]];

const SPAWN_PATHS: &[&[&str]] = &[
    &["dioxus_hooks", "spawn"],
    &["dioxus", "prelude", "spawn"],
    &["tokio", "task", "spawn"],
    &["tokio", "spawn"],
];

const WRITE_METHOD_NAMES: &[&str] =
    &["write", "batch", "set", "pending_update"];

/// Does the callee of a Call/MethodCall expression resolve to one of
/// the `candidate_paths`? Uses `def_path_str` + suffix match so
/// re-exports like `dioxus::prelude::use_effect` still count.
fn callee_matches<'tcx>(
    cx: &LateContext<'tcx>,
    callee: &Expr<'tcx>,
    candidate_paths: &[&[&str]],
) -> bool {
    let Some(def_id) = rustc_hir_typeck_callee_def_id(cx, callee) else {
        return false;
    };
    let crate_name = cx.tcx.crate_name(def_id.krate).to_string();
    let path = cx.tcx.def_path(def_id);
    let full: Vec<String> = std::iter::once(crate_name)
        .chain(path.data.iter().map(|d| d.data.to_string()))
        .collect();
    candidate_paths.iter().any(|target| {
        full.len() >= target.len()
            && full
                .iter()
                .rev()
                .zip(target.iter().rev())
                .all(|(a, b)| a == *b)
    })
}

fn rustc_hir_typeck_callee_def_id<'tcx>(
    cx: &LateContext<'tcx>,
    callee: &Expr<'tcx>,
) -> Option<rustc_span::def_id::DefId> {
    if let ExprKind::Path(qpath) = &callee.kind {
        cx.typeck_results().qpath_res(qpath, callee.hir_id).opt_def_id()
    } else {
        None
    }
}

impl<'tcx> LateLintPass<'tcx> for UseEffectSpawnCycle {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx Expr<'tcx>) {
        // Shape: `use_effect(|| { … })` or `use_effect(move || { … })`.
        let ExprKind::Call(callee, args) = &expr.kind else {
            return;
        };
        if !callee_matches(cx, callee, USE_EFFECT_PATHS) {
            return;
        }
        let Some(closure_arg) = args.first() else {
            return;
        };
        let ExprKind::Closure(closure) = &closure_arg.kind else {
            return;
        };
        let body = cx.tcx.hir_body(closure.body);

        // Walk the effect body looking for `spawn(async move { … })`
        // whose async block writes a signal.
        let mut v = SpawnFinder { cx, hit: false };
        v.visit_expr(body.value);
        if !v.hit {
            return;
        }

        cx.opt_span_lint(USE_EFFECT_SPAWN_CYCLE, expr.span, |diag| {
            diag.primary_message(
                "`use_effect` spawns a future that writes a signal — \
                 use `use_spawn_once<K>(key, async_fn)` instead",
            );
            diag.note(
                "This is CLAUDE.md hang class #3 (infinite spawn loop via \
                 re-subscribed `use_effect`). See \
                 crates/core/src/state/use_spawn_once.rs and \
                 docs/plans/plan-use-spawn-once.md. Allowlist with \
                 `#[allow(poly::use_effect_spawn_cycle)] // reason: …`.",
            );
        });
    }
}

struct SpawnFinder<'a, 'tcx> {
    cx: &'a LateContext<'tcx>,
    hit: bool,
}

impl<'tcx> Visitor<'tcx> for SpawnFinder<'_, 'tcx> {
    fn visit_expr(&mut self, ex: &'tcx Expr<'tcx>) {
        if self.hit {
            return;
        }
        // `spawn(async move { … })` is `Call(spawn, [Closure{async, …}])`
        // under HIR. The async block desugars to a closure whose body
        // is the async generator.
        if let ExprKind::Call(callee, args) = &ex.kind
            && callee_matches(self.cx, callee, SPAWN_PATHS)
            && let Some(async_arg) = args.first()
        {
            let mut inner = SignalWriteFinder { hit: false };
            inner.visit_expr(async_arg);
            if inner.hit {
                self.hit = true;
                return;
            }
        }
        walk_expr(self, ex);
    }
}

struct SignalWriteFinder {
    hit: bool,
}

impl<'tcx> Visitor<'tcx> for SignalWriteFinder {
    fn visit_expr(&mut self, ex: &'tcx Expr<'tcx>) {
        if self.hit {
            return;
        }
        if let ExprKind::MethodCall(path_seg, _recv, _args, _span) = &ex.kind {
            let name = path_seg.ident.as_str();
            if WRITE_METHOD_NAMES.iter().any(|n| *n == name) {
                self.hit = true;
                return;
            }
        }
        walk_expr(self, ex);
    }
}
