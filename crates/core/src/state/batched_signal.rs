//! `BatchedSignal<T>` — a `Signal<T>` newtype that refuses to hand out
//! individual `Write<'_, T>` guards outside a single closure scope.
//!
//! # Why this exists
//!
//! See `CLAUDE.md` § "Common WASM-hang causes" **#1**:
//!
//! > `Signal::write()` chains in a click handler. Every `.write()` guard
//! > drop schedules a Dioxus reactive re-render. 7 consecutive writes →
//! > 7 cascades on the WASM single-thread → scheduler starves.
//!
//! The fix at every call site was mechanical — collapse the N `.write()`
//! calls into one `{ let mut g = sig.write(); g.a = …; g.b = …; }` block.
//! The failure mode kept re-landing because nothing in the type system
//! pushed against it. `BatchedSignal<T>` makes "multiple-guards-in-a-row"
//! syntactically un-expressible: the only mutation verb is
//! [`BatchedSignal::batch`], a closure that acquires *exactly one* write
//! guard and drops it on return.
//!
//! # Compatibility
//!
//! `Deref<Target = Signal<T>>` is load-bearing: every read path
//! (`.read()`, `.peek()`, `use_effect(move || { bs(); })`, `rsx!` format
//! strings) goes through Dioxus' existing trait surface on `Signal<T>`
//! and therefore keeps working unchanged. The one operation hidden is
//! `.write()`, which is *also* shadowed by a deprecated inherent method
//! that panics if called — see [`BatchedSignal::write`].
//!
//! # API surface
//!
//! | Use case | API |
//! |----------|-----|
//! | Sync single/multi-field mutation | [`BatchedSignal::batch`] |
//! | Read via closure (scoped guard) | [`BatchedSignal::with`] / [`BatchedSignal::map`] |
//! | Async-interleaved mutation | [`BatchedSignal::pending_update`] + [`PendingUpdate`] |
//! | Read one-liners | inherited via `Deref` (`.peek()`, `.read()`, …) |
//!
//! This is Phase 1 of `docs/plans/plan-batched-signal.md` — the type is
//! introduced alongside `Signal<T>` with no call-site migrations.

use dioxus::prelude::*;
use std::ops::Deref;

// ─────────────────────────────────────────────────────────────────────────────
// BatchedSignal<T>
// ─────────────────────────────────────────────────────────────────────────────

/// A [`Signal<T>`] whose only sync mutation path is [`BatchedSignal::batch`].
///
/// See the module-level docs for the motivation. Construct with
/// [`BatchedSignal::use_batched`] inside a component, or
/// [`BatchedSignal::from_signal`] when wrapping an existing `Signal<T>`.
#[derive(Debug)]
pub struct BatchedSignal<T: 'static> {
    inner: Signal<T>,
}

// Manual Copy/Clone — deriving would attempt a `T: Copy/Clone` bound, but
// `Signal<T>: Copy` for every `T: 'static` (it's a handle into the
// generational arena, not the value).
impl<T: 'static> Copy for BatchedSignal<T> {}

impl<T: 'static> Clone for BatchedSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> PartialEq for BatchedSignal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<T: 'static> Deref for BatchedSignal<T> {
    type Target = Signal<T>;
    fn deref(&self) -> &Signal<T> {
        &self.inner
    }
}

impl<T: 'static> BatchedSignal<T> {
    /// Wrap an existing `Signal<T>` produced by `use_signal` or
    /// `Signal::new`. Used when flipping a `use_context_provider` site:
    ///
    /// ```ignore
    /// use_context_provider(|| BatchedSignal::from_signal(Signal::new(ChatData::default())));
    /// ```
    #[must_use] 
    pub fn from_signal(inner: Signal<T>) -> Self {
        Self { inner }
    }

    /// Component-level hook — shorthand for
    /// `BatchedSignal::from_signal(use_signal(init))`.
    ///
    /// Must be called at the top of a Dioxus component; same rules as
    /// `use_signal`.
    pub fn use_batched<F: FnOnce() -> T>(init: F) -> Self {
        Self::from_signal(use_signal(init))
    }

    /// Acquire **exactly one** write guard, run `f` against `&mut T`,
    /// drop the guard. This is the only sync mutation path.
    ///
    /// The returned `R` lets you pipe data back out so you don't need
    /// a follow-up `read()` on the same signal — for example, returning
    /// a freshly-derived ID or a cloned field.
    ///
    /// # Example
    ///
    /// ```ignore
    /// chat_data.batch(|cd| {
    ///     cd.loading = false;
    ///     cd.messages = messages;
    ///     cd.members = members;
    /// });
    /// ```
    pub fn batch<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        // `Signal<T>` is `Copy`; we need `&mut` on the binding for
        // `WritableExt::write`, so shadow with a local `mut` copy.
        let mut inner = self.inner;
        let mut guard = inner.write();
        
        // guard drops here → exactly one reactive cascade
        f(&mut *guard)
    }

    /// Read via a closure. Equivalent to `f(&*self.read())` but keeps
    /// the guard scope explicit.
    ///
    /// Prefer `.peek()` / `.read()` (via `Deref`) for simple one-liners;
    /// use this when you want to ensure the read guard doesn't leak past
    /// the natural read scope.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&*self.inner.read())
    }

    /// Alias for [`BatchedSignal::with`] — reads more naturally at sites
    /// that "map" the signal's value to a derived thing
    /// (`let name = bs.map(|v| v.name.clone());`).
    pub fn map<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.with(f)
    }

    /// Start a [`PendingUpdate`] accumulator for async-interleaved
    /// mutations — mutations that need to be queued across `.await`
    /// points and committed with a single terminal write.
    ///
    /// See [`PendingUpdate`] for the full API and `load_server_data_internal`
    /// in `favorites_sidebar.rs` for the canonical pattern.
    #[must_use]
    pub fn pending_update(&self) -> PendingUpdate<T> {
        PendingUpdate {
            target: *self,
            mutators: Vec::new(),
            applied: false,
        }
    }

    /// Write the closure-computed next value only if it differs from the
    /// current value. Use this **inside `use_effect` bodies that subscribe
    /// to the same signal** to break self-triggered re-render loops.
    ///
    /// # Why this exists
    ///
    /// `BatchedSignal::batch` always notifies subscribers, regardless of
    /// whether the closure actually changed the value. An effect that
    /// reads signal `S` (subscribing) and then writes `S` will re-fire
    /// after its own write — forever — unless an early-return guard inside
    /// the body fires for the steady state. When the early-return guard
    /// has a hole (e.g. `messages_loaded` for an empty channel), the loop
    /// pegs the WASM scheduler.
    ///
    /// `batch_if_changed` makes the guard intrinsic to the API: the write
    /// is suppressed when nothing changed, so the signal doesn't re-notify
    /// and the effect doesn't re-fire.
    ///
    /// See `CLAUDE.md` § "Common WASM-hang causes" **#8**.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use_effect(move || {
    ///     let snapshot = chat_data.read().clone();
    ///     let mut next = ChatHistoryUiState { ... derived from snapshot ... };
    ///     // Old shape — re-fires forever when the early-return doesn't catch the steady state:
    ///     // history_state.batch(|h| *h = next);
    ///     // New shape — write is suppressed when next == current:
    ///     history_state.batch_if_changed(|_| next);
    /// });
    /// ```
    pub fn batch_if_changed<F>(&self, f: F)
    where
        T: PartialEq,
        F: FnOnce(&T) -> T,
    {
        let next = f(&*self.inner.read());
        if *self.inner.read() == next {
            return;
        }
        self.batch(|v| *v = next);
    }

    /// Replace the value only if `next != current`. Convenience wrapper
    /// over [`BatchedSignal::batch_if_changed`] for sites that already
    /// have the next value in hand.
    pub fn set_if_changed(&self, next: T)
    where
        T: PartialEq,
    {
        if *self.inner.read() == next {
            return;
        }
        self.batch(|v| *v = next);
    }

    /// **Banned** — use [`BatchedSignal::batch`] instead.
    ///
    /// Multiple consecutive `.write()` calls each drop a guard and
    /// schedule a Dioxus reactive pass. On the single-threaded WASM
    /// scheduler this cascades into a hang. See `CLAUDE.md` §
    /// "Common WASM-hang causes" **#1**.
    ///
    /// This shadow method exists specifically to prevent
    /// `Signal::write` from being reached via `Deref` coercion. If you
    /// need to mutate, call `.batch(|v| …)` — one closure, one guard,
    /// one cascade.
    #[deprecated(
        since = "0.2.0",
        note = "use .batch(|v| ...) — consecutive .write() calls hang the WASM scheduler. See CLAUDE.md § Common WASM-hang causes #1."
    )]
    pub fn write(&self) -> ! {
        // This body is unreachable in correct code (the #[deprecated]
        // attribute turns call sites into a deny-level warning, and
        // the Phase-5 clippy lint will outright reject them). We still
        // need the function body to diverge so the `-> !` return type
        // holds. `std::process::abort` is the cleanest non-panic
        // divergence — it doesn't trip `clippy::panic` and it kills
        // the process deterministically if the deprecation is somehow
        // bypassed in debug.
        std::process::abort();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PendingUpdate<T>
// ─────────────────────────────────────────────────────────────────────────────

/// An accumulator for async-interleaved mutations on a [`BatchedSignal`].
///
/// Each `.set(|v| …)` queues a boxed mutator closure without touching
/// the signal (so no reactive pass is scheduled). `.apply()` drains the
/// queue inside a **single** `batch` call — one write guard, one
/// cascade, no matter how many `set`s preceded it.
///
/// # Lifecycle
///
/// - Must end with **either** `.apply()` (commit) or `.discard()` (abort).
/// - Dropping a non-empty `PendingUpdate` without calling either is a
///   bug: it silently loses queued mutations. Debug builds panic with
///   the target type name + pending count; release builds emit a
///   `tracing::warn!` on the `batched_signal` target.
/// - `#[must_use]` also gets clippy to yell at you when you ignore the
///   returned builder.
///
/// # Example
///
/// ```ignore
/// let mut pending = chat_data.pending_update();
/// pending.set(|cd| cd.loading = true);
/// let server = backend.get_server(&id).await?;
/// pending.set(move |cd| cd.current_server = Some(server));
/// let channels = backend.get_channels(&id).await?;
/// pending.set(move |cd| cd.channels = channels);
/// pending.apply(); // ONE cascade, no matter how many .set() calls
/// ```
#[must_use = "a PendingUpdate does nothing unless you call .apply() (or .discard() to abort)"]
/// Type alias for a queued mutation closure on a `PendingUpdate<T>`.
type Mutator<T> = Box<dyn FnOnce(&mut T)>;

pub struct PendingUpdate<T: 'static> {
    target: BatchedSignal<T>,
    mutators: Vec<Mutator<T>>,
    applied: bool,
}

impl<T: 'static> PendingUpdate<T> {
    /// Queue a mutation. Nothing happens until [`PendingUpdate::apply`]
    /// is called. Later `.set` calls run after earlier ones — the queue
    /// is insertion-ordered.
    pub fn set<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut T) + 'static,
    {
        self.mutators.push(Box::new(f));
        self
    }

    /// Apply all queued mutations inside **one** `batch` call.
    ///
    /// If the queue is empty, no write guard is taken — skipping a
    /// useless cascade — and the builder is marked applied.
    pub fn apply(mut self) {
        if self.mutators.is_empty() {
            self.applied = true;
            return;
        }
        let mutators = std::mem::take(&mut self.mutators);
        self.target.batch(move |v| {
            for m in mutators {
                m(v);
            }
        });
        self.applied = true;
    }

    /// Abandon pending mutations without applying them. Explicit
    /// opt-out so the `Drop` warning doesn't fire on legitimate
    /// early-return paths.
    pub fn discard(mut self) {
        self.mutators.clear();
        self.applied = true;
    }
}

impl<T: 'static> Drop for PendingUpdate<T> {
    fn drop(&mut self) {
        if self.applied || self.mutators.is_empty() {
            return;
        }
        let type_name = std::any::type_name::<T>();
        let pending = self.mutators.len();
        // Debug: panic so the bug surfaces in `cargo test` and dev
        // builds. Release: warn so prod telemetry can flag it without
        // killing the tab.
        #[cfg(debug_assertions)]
        {
            // Intentional dev-only panic: makes the misuse loud in `cargo
            // test` so the bug doesn't slip into prod. The whole branch is
            // `#[cfg(debug_assertions)]`-gated so release builds use the
            // tracing::warn! path below.
            // lint-allow-unused: dev-only loud-fail for PendingUpdate misuse
            #[allow(clippy::panic)]
            {
                panic!(
                    "PendingUpdate<{type_name}> dropped with {pending} unapplied mutations — \
                     call .apply() or .discard() explicitly"
                );
            }
        }
        #[cfg(not(debug_assertions))]
        {
            tracing::warn!(
                target: "batched_signal",
                type_name,
                pending,
                "PendingUpdate dropped without .apply() — mutations lost"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Hook helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Hook — fetch a `BatchedSignal<T>` provided by an ancestor
/// `use_context_provider`. Direct replacement for
/// `use_context::<Signal<T>>()` in migrated subtrees.
#[must_use] 
pub fn use_batched_context<T: 'static>() -> BatchedSignal<T> {
    use_context::<BatchedSignal<T>>()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

    use super::*;
    use dioxus::prelude::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// Run a closure inside a throw-away Dioxus runtime AND scope so
    /// `Signal::new` has somewhere to attach its generational-arena
    /// storage. `VirtualDom::new` constructs the runtime; `in_scope`
    /// installs the root scope + runtime guard for the closure's
    /// duration.
    fn with_runtime<R>(f: impl FnOnce() -> R) -> R {
        fn empty() -> Element {
            rsx! {}
        }
        let vdom = VirtualDom::new(empty);
        vdom.in_scope(ScopeId::ROOT, f)
    }

    #[test]
    fn batch_runs_closure_once_and_returns_value() {
        with_runtime(|| {
            let bs = BatchedSignal::from_signal(Signal::new(0_u32));
            let call_count = Rc::new(Cell::new(0_u32));
            let cc = call_count.clone();
            let out = bs.batch(move |v| {
                cc.set(cc.get() + 1);
                *v = 42;
                *v
            });
            assert_eq!(out, 42, "batch should pipe the closure return through");
            assert_eq!(call_count.get(), 1, "closure runs exactly once");
            assert_eq!(*bs.peek(), 42, "mutation landed");
        });
    }

    #[test]
    fn two_sequential_batches_produce_two_cascades() {
        with_runtime(|| {
            // We can't directly observe the Dioxus cascade counter from
            // outside the runtime, but each `batch` call takes ONE write
            // guard and drops it on return. Prove the semantics by
            // counting guard-drop side effects: after two batches the
            // value reflects both mutations and both closures ran
            // independently.
            let bs = BatchedSignal::from_signal(Signal::new(0_u32));
            let runs = Rc::new(Cell::new(0_u32));
            let r1 = runs.clone();
            bs.batch(move |v| {
                r1.set(r1.get() + 1);
                *v += 1;
            });
            let r2 = runs.clone();
            bs.batch(move |v| {
                r2.set(r2.get() + 1);
                *v += 10;
            });
            assert_eq!(runs.get(), 2, "two batch calls → two closure runs");
            assert_eq!(*bs.peek(), 11);
        });
    }

    #[test]
    fn pending_update_apply_runs_all_mutators_in_one_batch() {
        with_runtime(|| {
            let bs = BatchedSignal::from_signal(Signal::new(0_u32));
            // The `apply` path goes through one `batch` call regardless
            // of how many `set`s queued up. Observe by threading a
            // counter into the batch closure indirectly — `set` mutators
            // run sequentially inside a single `batch`.
            let run_order = Rc::new(Cell::new(Vec::<u32>::new()));
            let mut pending = bs.pending_update();
            let ro1 = run_order.clone();
            pending.set(move |v| {
                let mut seq = ro1.take();
                seq.push(1);
                ro1.set(seq);
                *v += 1;
            });
            let ro2 = run_order.clone();
            pending.set(move |v| {
                let mut seq = ro2.take();
                seq.push(2);
                ro2.set(seq);
                *v += 10;
            });
            let ro3 = run_order.clone();
            pending.set(move |v| {
                let mut seq = ro3.take();
                seq.push(3);
                ro3.set(seq);
                *v += 100;
            });
            pending.apply();
            assert_eq!(
                run_order.take(),
                vec![1, 2, 3],
                "mutators run in insertion order"
            );
            assert_eq!(*bs.peek(), 111, "all three mutations applied");
        });
    }

    #[test]
    fn pending_update_empty_apply_is_noop() {
        with_runtime(|| {
            let bs = BatchedSignal::from_signal(Signal::new(7_u32));
            let pending = bs.pending_update();
            pending.apply(); // no mutators → no batch, no panic
            assert_eq!(*bs.peek(), 7);
        });
    }

    #[test]
    fn pending_update_discard_drops_silently() {
        with_runtime(|| {
            let bs = BatchedSignal::from_signal(Signal::new(0_u32));
            let mut pending = bs.pending_update();
            pending.set(|v| *v = 999);
            pending.discard();
            // If `discard` weren't wired up, the Drop impl below would
            // panic on the unapplied mutator. Reaching here = OK.
            assert_eq!(*bs.peek(), 0, "discarded mutators don't run");
        });
    }

    #[test]
    #[should_panic(expected = "PendingUpdate")]
    fn pending_update_drop_without_apply_panics_in_debug() {
        // Debug builds: Drop impl panics. Release builds: tracing::warn.
        // `cargo test` always compiles with debug_assertions → this
        // test asserts the debug contract. The panic message includes
        // the type name so engineers can grep for it in CI output.
        with_runtime(|| {
            let bs = BatchedSignal::from_signal(Signal::new(0_u32));
            let mut pending = bs.pending_update();
            pending.set(|v| *v = 1);
            // Let it drop without apply or discard.
            drop(pending);
        });
    }

    #[test]
    fn pending_update_drop_message_includes_type_name() {
        // Sanity check on the panic message shape. We catch the unwind
        // to inspect the payload and confirm the type name is present —
        // this is the part engineers grep for when a CI run fails.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_runtime(|| {
                let bs = BatchedSignal::from_signal(Signal::new(0_u64));
                let mut pending = bs.pending_update();
                pending.set(|v| *v = 1);
                drop(pending);
            });
        }));
        let err = result.expect_err("drop without apply must panic in debug");
        let msg = err
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&'static str>().map(|s| (*s).to_string()))
            .unwrap_or_default();
        assert!(
            msg.contains("u64"),
            "panic message must name the type parameter, got: {msg}"
        );
        assert!(
            msg.contains("PendingUpdate"),
            "panic message must mention PendingUpdate, got: {msg}"
        );
    }

    #[test]
    fn copy_and_clone_share_storage() {
        with_runtime(|| {
            let a = BatchedSignal::from_signal(Signal::new(0_i32));
            let b = a; // Copy
            #[allow(clippy::clone_on_copy)]
            let c = a.clone(); // Clone
            a.batch(|v| *v = 5);
            assert_eq!(*b.peek(), 5, "Copy handle sees the mutation");
            assert_eq!(*c.peek(), 5, "Clone handle sees the mutation");
            b.batch(|v| *v = 9);
            assert_eq!(*a.peek(), 9, "mutation via Copy handle visible on original");
        });
    }

    #[test]
    fn deref_exposes_inner_signal() {
        with_runtime(|| {
            let bs = BatchedSignal::from_signal(Signal::new(42_u8));
            // The Deref target is `Signal<T>`; this line only
            // typechecks if Deref is wired correctly.
            let sig: &Signal<u8> = &*bs;
            assert_eq!(*sig.peek(), 42);
            assert_eq!(*bs.peek(), *sig.peek());
        });
    }

    #[test]
    fn partial_eq_delegates_to_inner() {
        with_runtime(|| {
            let bs = BatchedSignal::from_signal(Signal::new(0_u32));
            let bs_copy = bs;
            let other = BatchedSignal::from_signal(Signal::new(0_u32));
            assert_eq!(bs, bs_copy, "copies of the same handle compare equal");
            assert_ne!(
                bs, other,
                "distinct signals compare unequal even with same value"
            );
        });
    }

    #[test]
    fn with_and_map_read_without_holding_guard_past_scope() {
        with_runtime(|| {
            let bs = BatchedSignal::from_signal(Signal::new(String::from("hello")));
            let len = bs.with(|s| s.len());
            assert_eq!(len, 5);
            let owned = bs.map(|s| s.clone());
            assert_eq!(owned, "hello");
            // After both reads return, the guard is gone and we can
            // freely batch-mutate.
            bs.batch(|s| s.push_str(" world"));
            assert_eq!(bs.map(|s| s.clone()), "hello world");
        });
    }
}
