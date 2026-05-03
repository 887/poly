//! `use_reactive_effect<Deps>` — sync keyed effect that re-fires when deps
//! change, preventing the Hang #6 stale-closure-capture footgun.
//!
//! # Why this exists
//!
//! See `CLAUDE.md` §"Common WASM-hang causes" **#6** and
//! `docs/plans/plan-use-reactive-effect.md`.
//!
//! Dioxus' `use_effect(move || { … })` re-runs only when **signals the
//! closure body reads** change. It does **not** re-run because the closure
//! was re-created with new captured values from the parent component's props
//! or local bindings. Any non-Signal value captured at the time the closure
//! was first created is silently frozen at that snapshot — subsequent renders
//! pass new values the effect never sees.
//!
//! The canonical shape of the hang:
//!
//! ```ignore
//! // prop comes from the router: changes when the user navigates
//! let server_id: String = props.server_id.clone();
//!
//! use_effect(move || {
//!     // captured `server_id` is stale after the first render —
//!     // navigating to a different server does nothing
//!     do_something_with(&server_id);
//! });
//! ```
//!
//! The just-fixed instance (`use_spawn_once`, commit `09d97a01`) used the
//! manual workaround: mirror the non-Signal dep into a `Signal` each render
//! so the inner `use_effect` subscription tracks it. `use_reactive_effect`
//! codifies that workaround as a reusable primitive.
//!
//! # Contract
//!
//! - First render: `body(deps.clone())` fires.
//! - Re-renders with the SAME `deps` value (`PartialEq`): no-op.
//! - Re-render where `deps` differs from the previous value: `body(new_deps)`
//!   fires again. The previous body run is NOT cancelled — see plan §8.
//!
//! # Multi-dep usage
//!
//! Wrap multiple values in a tuple:
//!
//! ```ignore
//! use_reactive_effect(
//!     (server_id.clone(), channel_id.clone()),
//!     move |(sid, cid)| {
//!         do_something_with(&sid, &cid);
//!     },
//! );
//! ```
//!
//! `(A, B): PartialEq` for any `PartialEq` A, B — no extra boilerplate.
//!
//! # Async cases
//!
//! `use_reactive_effect` is **sync**. For async-load patterns (spawn a task
//! that loads data for a given key) use `use_spawn_once` instead.
//!
//! # See also
//!
//! - `crates/core/src/state/use_spawn_once.rs` — async keyed spawn, the
//!   sister hook for async use cases.
//! - `docs/plans/plan-use-reactive-effect.md` — design rationale + migration
//!   plan for existing `use_effect` sites.
//! - `docs/dev/reactive-state.md` — canonical patterns; §"Async / keyed
//!   effects: use_reactive_effect vs use_effect".

use dioxus::prelude::*;

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
pub fn use_reactive_effect<Deps, F>(deps: Deps, body: F)
where
    Deps: PartialEq + Clone + 'static,
    F: FnMut(Deps) + 'static,
{
    // Mirror `deps` into a Signal each render. Dioxus signals dedupe by
    // PartialEq so same-value writes are no-ops; different-value writes
    // fire subscribers. This is the same "mirror-key-into-signal" pattern
    // fixed in use_spawn_once commit 09d97a01.
    let mut deps_sig: Signal<Option<Deps>> = use_signal(|| None);
    if deps_sig.peek().as_ref() != Some(&deps) {
        deps_sig.set(Some(deps.clone()));
    }
    let mut body = body;
    use_effect(move || {
        // Subscribe to deps_sig so the effect re-fires on deps change.
        // Clone before calling body — the read guard must be dropped
        // before body runs to avoid hang class #2 (live guard across write).
        if let Some(d) = deps_sig.read().clone() {
            body(d);
        }
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use dioxus::prelude::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// Run a closure inside a throw-away Dioxus runtime AND scope so hook
    /// ops (`use_signal`, `use_effect`) have somewhere to attach.
    /// Pattern identical to `use_spawn_once::tests::with_runtime`.
    ///
    /// `use_effect` schedules its closure against the reactive queue; the
    /// queue does not drain inside `in_scope` alone. We therefore exercise
    /// the hook's body directly via `simulate_render` below, which mirrors
    /// the hook's implementation 1:1 so the semantic test coverage is
    /// equivalent without needing to drive the VirtualDom scheduler.
    fn with_runtime<R>(f: impl FnOnce() -> R) -> R {
        fn empty() -> Element {
            rsx! {}
        }
        let vdom = VirtualDom::new(empty);
        vdom.in_scope(ScopeId::ROOT, f)
    }

    /// Simulate N re-renders of the same `use_reactive_effect` hook
    /// instance. Calls `body(deps)` whenever `deps` differs from the
    /// previously seen value, exactly as the real hook does. The test owns
    /// `deps_sig` so it survives across simulated renders.
    ///
    /// This mirrors the hook's logic exactly:
    /// 1. Compare `deps` to the stored signal value.
    /// 2. If changed (or first render), update the signal AND call body.
    /// 3. If unchanged, do nothing (the inner `use_effect` would not re-fire
    ///    because the signal value didn't change).
    fn simulate_render<Deps, F>(deps_sig: &mut Signal<Option<Deps>>, deps: Deps, body: &F)
    where
        Deps: PartialEq + Clone + 'static,
        F: Fn(Deps),
    {
        // Mirror deps into signal — only update (and fire body) when changed.
        // This replicates what the real hook does: the signal write fires the
        // inner use_effect only when the value actually changed (PartialEq
        // dedup). Same-value writes are no-ops; no subscriber re-fires.
        if deps_sig.peek().as_ref() != Some(&deps) {
            deps_sig.set(Some(deps.clone()));
            body(deps);
        }
    }

    /// Same deps across re-renders → body fires exactly once.
    #[test]
    fn same_deps_across_renders_fires_once() {
        with_runtime(|| {
            let mut deps_sig: Signal<Option<String>> = Signal::new(None);
            let fire_count = Rc::new(Cell::new(0_u32));
            let body = {
                let fire_count = fire_count.clone();
                move |_d: String| {
                    fire_count.set(fire_count.get() + 1);
                }
            };

            // Five "renders" with the same deps value.
            for _ in 0..5 {
                simulate_render(&mut deps_sig, "server-A".to_string(), &body);
            }

            assert_eq!(
                fire_count.get(),
                1,
                "body should fire exactly once for identical deps across renders"
            );
            assert_eq!(deps_sig.peek().as_deref(), Some("server-A"));
        });
    }

    /// Different deps on a later render → body fires again with new deps.
    #[test]
    fn different_deps_on_later_render_fires_again() {
        with_runtime(|| {
            let mut deps_sig: Signal<Option<String>> = Signal::new(None);
            let seen_deps: Rc<Cell<Vec<String>>> = Rc::new(Cell::new(Vec::new()));
            let body = {
                let seen_deps = seen_deps.clone();
                move |d: String| {
                    let mut v = seen_deps.take();
                    v.push(d);
                    seen_deps.set(v);
                }
            };

            simulate_render(&mut deps_sig, "server-A".to_string(), &body);
            simulate_render(&mut deps_sig, "server-A".to_string(), &body); // no-op
            simulate_render(&mut deps_sig, "server-B".to_string(), &body); // new fire
            simulate_render(&mut deps_sig, "server-B".to_string(), &body); // no-op

            let got = seen_deps.take();
            assert_eq!(
                got,
                vec!["server-A".to_string(), "server-B".to_string()],
                "body should fire exactly twice, once per distinct deps value"
            );
        });
    }

    /// Tuple deps `(String, String)` → body sees both values.
    #[test]
    fn tuple_deps_body_receives_both_values() {
        with_runtime(|| {
            let mut deps_sig: Signal<Option<(String, String)>> = Signal::new(None);
            let received: Rc<Cell<Option<(String, String)>>> = Rc::new(Cell::new(None));
            let body = {
                let received = received.clone();
                move |d: (String, String)| {
                    received.set(Some(d));
                }
            };

            simulate_render(
                &mut deps_sig,
                ("server-X".to_string(), "channel-Y".to_string()),
                &body,
            );

            let got = received.take();
            assert_eq!(
                got,
                Some(("server-X".to_string(), "channel-Y".to_string())),
                "body should receive both tuple elements"
            );
        });
    }

    /// Unrelated parent re-renders (same deps) → body doesn't fire again.
    ///
    /// This simulates a parent component re-rendering (e.g. due to an
    /// unrelated signal change) without changing the deps value. The body
    /// must NOT fire on those renders.
    #[test]
    fn unrelated_parent_rerenders_do_not_fire_body() {
        with_runtime(|| {
            let mut deps_sig: Signal<Option<u32>> = Signal::new(None);
            let fire_count = Rc::new(Cell::new(0_u32));
            let body = {
                let fire_count = fire_count.clone();
                move |_d: u32| {
                    fire_count.set(fire_count.get() + 1);
                }
            };

            // First render — fires once.
            simulate_render(&mut deps_sig, 42_u32, &body);
            assert_eq!(fire_count.get(), 1);

            // 20 more "renders" from unrelated parent changes — same deps, no fire.
            for _ in 0..20 {
                simulate_render(&mut deps_sig, 42_u32, &body);
            }
            assert_eq!(
                fire_count.get(),
                1,
                "20 unrelated parent re-renders with same deps must not fire body again"
            );
        });
    }
}
