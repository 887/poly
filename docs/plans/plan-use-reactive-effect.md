# Plan — `use_reactive_effect<Deps>` Hook (Stale-Closure-Capture Prevention)

> Status: **✅ DONE** — Phases 1+2+5 shipped with hard-fail CI gate (`d3d8e891`, `94688279`, `81d0373`). 54 raw `use_effect` sites triaged, ~11 migrated, 43 inline-allowlisted as legitimate Signal-only / one-shot mount cases. `forbid-stale-effect-capture.sh` is now `continue-on-error: false`.
> Last updated: 2026-04-25.

---

## 1. Why this plan exists

Dioxus' `use_effect(move || { … })` re-runs only when SIGNALS that the closure body READS change. **It does NOT re-run just because the closure was re-created** with new captured values from the parent component's props or local bindings.

This is a different bug class from any previously closed:

- Hang #1 (cascade) — closed by `BatchedSignal`.
- Hang #2 (read-across-write) — closed by `with` + lint.
- Hang #3 (use_effect spawn cycle) — closed by `use_spawn_once`.
- Hang #4 (RwLock starvation) — closed by `read_with_timeout`.
- Hang #5 (spawn-across-guard) — closed by #1's batch closure.
- **Hang #6 (NEW): `use_effect` captures a non-Signal value that drifts.** Effect runs once with the initial value; subsequent re-renders pass new values that are silently ignored. UI shows stale data; in load-spawn flows this manifests as "second navigation has no effect" or worse, a partially-loaded state that crashes downstream consumers.

**Just-shipped incident (commit `94688279`, 2026-04-25):**
- `crates/core/src/state/use_spawn_once.rs` shipped with this exact mistake. The internal `use_effect` only subscribed to its own `spawned_for` signal. Navigating from Teams team T001 → team T002 changed the captured `key` value but did NOT re-fire the effect, so the second navigation silently kept the first nav's loaded state. User reported as "Teams server-switch crashes."
- Fix: mirror `key` into a second Signal each render so the effect's subscription tracks key changes through PartialEq dedup.

The fix pattern (mirror-into-signal-then-subscribe) is the canonical workaround. **It's also boilerplate.** Any custom hook that takes a non-Signal "dep" parameter has to do this manually. Forgotten ⇒ silently stale UI or a hang.

---

## 2. Solution summary

Introduce `use_reactive_effect<Deps>(deps: Deps, body: impl Fn(Deps))` as the canonical primitive for "effect that re-fires when these dep values change":

```rust
// crates/core/src/state/use_reactive_effect.rs
pub fn use_reactive_effect<Deps, F>(deps: Deps, body: F)
where
    Deps: PartialEq + Clone + 'static,
    F: Fn(Deps) + 'static,
{
    let mut deps_sig: Signal<Option<Deps>> = use_signal(|| None);
    if deps_sig.peek().as_ref() != Some(&deps) {
        deps_sig.set(Some(deps.clone()));
    }
    use_effect(move || {
        if let Some(d) = deps_sig.read().clone() {
            body(d);
        }
    });
}
```

For multi-dep cases, callers wrap in a tuple: `use_reactive_effect((server_id.clone(), channel_id.clone()), move |(sid, cid)| { … })`. `(A, B): PartialEq` for any `PartialEq` A, B — works out of the box.

**For async-spawn cases**, `use_spawn_once` is already the right primitive (just fixed via `94688279`). Other cases — synchronous side effects keyed on props — should migrate to `use_reactive_effect` instead of raw `use_effect`.

**Lint companion (Phase 5 Track A):** `tools/scripts/forbid-stale-effect-capture.sh` flags any `use_effect(move || { … })` whose body references a closure-captured value that is NOT a `Signal<T>` / `BatchedSignal<T>` / `Copy` primitive — those are the patterns that go stale. Replacement: use `use_reactive_effect` or `use_spawn_once`.

---

## 3. Phases

### Phase 1 — Introduce `use_reactive_effect` hook — ✅ DONE (`d3d8e891`)

- [x] `crates/core/src/state/use_reactive_effect.rs` — implementation + tests.
- [x] Re-export from `crates/core/src/state/mod.rs`.
- [x] Tests: same-deps re-render → no re-fire; different-deps → re-fire; tuple deps work; deps drop semantics documented.

### Phase 2 — Audit + migrate stale-capture sites — ✅ DONE (`81d0373`)

- [x] Audit subagent grep + manual review of every `use_effect(move || { … })` in `crates/core/src/ui/**/*.rs`. Classify by capture shape:
  - **HIGH**: captures a non-Signal prop / local that varies across renders. Migrate to `use_reactive_effect` or `use_spawn_once`.
  - **MEDIUM**: captures a Signal but reads it only in `peek()` — won't re-fire on signal changes; might be intentional. Manual review.
  - **LOW**: captures only Signals (read), `Copy` primitives, or mounts a one-shot side effect that genuinely should run once.
- [x] Migrate every HIGH site.

### Phase 3 — Update `docs/dev/reactive-state.md` — ✅ DONE (`d3d8e891`)

- [x] New section: "When to use `use_reactive_effect` vs `use_effect`."
- [x] Document the stale-capture footgun + the `use_reactive_effect` recipe.
- [x] Cross-link to `use_spawn_once` for async cases.

### Phase 5 — Regex CI lint — ✅ DONE (`d3d8e891`)

- [x] `tools/scripts/forbid-stale-effect-capture.sh` flags `use_effect(move || { … })` whose closure captures a value that is not obviously a Signal/Copy. Heuristic: scan the closure body for identifier references that don't end in `.read()`, `.peek()`, `.with(`, `.batch(`. If any identifier is captured but only USED via `.clone()` or pass-by-value, flag.
- [x] Inline-allowlist: `// poly-lint: allow stale-effect-capture — <reason>`.
- [x] Wire into `lint-test.yml`.

---

## 4. Verification

- After Phase 1: `cargo check --workspace` clean, hook tests pass.
- After Phase 2: every audit-flagged HIGH site migrated; manual smoke per affected route.
- After Phase 5: CI fails on a deliberate `use_effect(move || { let x = some_prop; … })` reintroduction.

---

## 5. Risks / failure modes

1. **`Deps: PartialEq` constraint.** Some captures aren't trivially `PartialEq` (e.g. `Vec<T>` where T has expensive eq). Acceptable: most route-prop captures are `String` / `Option<String>` / `BackendType` (cheap). Heavy types should be replaced with stable IDs + Signals.
2. **Closure body that mutates captured state.** Migrating to `use_reactive_effect` requires `F: Fn`, not `FnMut`. Any captured `&mut something` needs to flip to a Signal mutation. That's actually the right shape.
3. **Async cases.** `use_reactive_effect` is sync. Async-load patterns should use `use_spawn_once<K>` (already shipped, fixed in `94688279`).
4. **Heuristic lint false positives.** Closure bodies that legitimately capture a Signal but never read it (e.g., pass to a child component) would get flagged. Allowlist + tune.

---

## 6. Timeline estimate

| Phase | Budget | Tier |
|-------|--------|------|
| 1 — hook + tests | 0.5 session | sonnet |
| 2 — audit + migration | 1 session | sonnet (audit) + sonnet (migrate) |
| 3 — dev doc | 0.2 session | sonnet |
| 5 — regex lint | 0.5 session | sonnet |

Total: ~1 focused session.

---

## 7. Reference artifacts

- Commit `94688279` — the just-fixed `use_spawn_once` instance of this bug.
- `docs/plans/plan-use-spawn-once.md` — sister plan for the async variant.
- `crates/core/src/state/use_spawn_once.rs` — contains the fixed pattern as reference.
- CLAUDE.md "Common WASM-hang causes" — to be extended with hang #6.

---

## 8. Out of scope

- **Cancellation of stale effects on dep change.** Same as `use_spawn_once`: the previous body's closure may have spawned long-lived work. Caller threads cancellation explicitly if needed.
- **Memoization helpers (`use_memo`).** Different primitive. Dioxus already has `use_memo`. This plan is about EFFECTS, not derived values.
- **Deprecating raw `use_effect`.** Like raw `Signal::read()`, it's load-bearing for genuine one-shot effects. Keep it; the lint catches the dangerous pattern.
