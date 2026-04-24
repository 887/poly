//! `poly::raw_signal_write` — bans `Signal::write()` method calls.
//!
//! Fires on:
//!   `sig.write()` where `sig: dioxus_signals::Signal<T>` (any T)
//!   `sig.write()` where `sig: dioxus_signals::ReadOnlySignal<T>`
//!
//! Does NOT fire on:
//!   `rwlock.write().await`    (receiver is `tokio::sync::RwLock`)
//!   `file.write(b"data")`     (receiver is `std::io::Write`)
//!   `batched.batch(|v| ...)`  (this is the migration target)
//!   `batched.pending_update()`
//!
//! Message mirrors the regex script's error message so CI output is
//! uniform regardless of which gate fires.

use rustc_hir::{Expr, ExprKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_session::{declare_lint, declare_lint_pass};
use rustc_span::sym;

declare_lint! {
    /// ### What it does
    ///
    /// Disallows `Signal::write()` method calls. Every `.write()` guard drop
    /// schedules a Dioxus reactive re-render; 5–7 consecutive writes wedge
    /// the single-threaded WASM scheduler (CLAUDE.md hang class #1).
    ///
    /// ### Why is this bad?
    ///
    /// Raw `.write()` chains in click handlers / loaders cause hard
    /// freezes of the WASM page. The `BatchedSignal::batch(|v| …)` API
    /// collapses N writes to a single re-render and is the required
    /// replacement for hot-path signals.
    ///
    /// ### Example
    ///
    /// ```ignore
    /// // BAD:
    /// signal.write().field1 = 1;
    /// signal.write().field2 = 2;
    ///
    /// // GOOD:
    /// signal.batch(|v| {
    ///     v.field1 = 1;
    ///     v.field2 = 2;
    /// });
    /// ```
    pub RAW_SIGNAL_WRITE,
    Deny,
    "`Signal::write()` — use `BatchedSignal::batch(|v| …)` instead (CLAUDE.md hang class #1)"
}

declare_lint_pass!(RawSignalWrite => [RAW_SIGNAL_WRITE]);

/// Canonical crate name of dioxus's signal-bearing crate. The concrete
/// crate has shifted names across Dioxus releases (`dioxus_signals`,
/// `generational_box`, re-exported through `dioxus::prelude`). We
/// match against the **type's canonical def_path_str** so re-exports
/// through `dioxus::prelude::Signal` still resolve to the same
/// underlying DefId.
const SIGNAL_PATHS: &[&[&str]] = &[
    &["dioxus_signals", "Signal"],
    &["dioxus_signals", "ReadOnlySignal"],
    // If dioxus renames the crate, add the new canonical path here.
];

impl<'tcx> LateLintPass<'tcx> for RawSignalWrite {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx Expr<'tcx>) {
        // Look at method calls of the shape `<receiver>.write()` with
        // zero arguments. `.write(buf)` (std::io::Write) is filtered by
        // the arg-count check.
        let ExprKind::MethodCall(path_seg, receiver, args, _span) = &expr.kind
        else {
            return;
        };
        if path_seg.ident.name != sym::write {
            // sym::write resolves to the interned "write" symbol; if it
            // isn't in the session's symbol table, fall back to a
            // string compare.
            if path_seg.ident.as_str() != "write" {
                return;
            }
        }
        if !args.is_empty() {
            // `.write(buf)` is std::io::Write — not a Signal method.
            return;
        }

        // Resolve the receiver type. Peel references / auto-deref so
        // `(&sig).write()` and `sig.write()` both resolve to the
        // underlying Signal type.
        let recv_ty = cx.typeck_results().expr_ty_adjusted(receiver).peel_refs();

        // Match against the signal paths. We use `def_path_str` because
        // it returns the canonical crate::module::Type form regardless
        // of how the caller imported the type (`use dioxus::prelude::Signal`
        // still resolves to `dioxus_signals::Signal`).
        let rustc_middle::ty::Adt(adt_def, _substs) = recv_ty.kind() else {
            return;
        };
        let def_id = adt_def.did();
        let path = cx.tcx.def_path(def_id);
        let path_components: Vec<String> = std::iter::once(
            cx.tcx.crate_name(def_id.krate).to_string(),
        )
        .chain(
            path.data
                .iter()
                .map(|d| d.data.to_string()),
        )
        .collect();

        let matches = SIGNAL_PATHS.iter().any(|target| {
            // Match suffix — dioxus re-exports can lengthen the path.
            path_components.len() >= target.len()
                && path_components
                    .iter()
                    .rev()
                    .zip(target.iter().rev())
                    .all(|(a, b)| a == *b)
        });
        if !matches {
            return;
        }

        cx.opt_span_lint(RAW_SIGNAL_WRITE, expr.span, |diag| {
            diag.primary_message(
                "forbidden `Signal::write()` — use `BatchedSignal::batch(|v| …)` or `pending_update()` instead",
            );
            diag.note(
                "This is CLAUDE.md hang class #1 (multi-.write() cascade). See \
                 crates/core/src/state/batched_signal.rs and \
                 docs/plans/plan-batched-signal.md. Allowlist with \
                 `#[allow(poly::raw_signal_write)] // reason: …`.",
            );
        });
    }
}
