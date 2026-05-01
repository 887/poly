# Plan — `use_spawn_once` Hook (use_effect-spawn-cycle Prevention)

> Status: **✅ DONE** — Phases 1-4 + 5 lint shipped (`50da1841`, `e7abb629`, `8b2551e1`). All 10 call sites migrated, 2 HIGH bug-waiting-to-happen sites resolved, lint hard-fails in CI.
> Related: `docs/plans/plan-batched-signal.md` (sister refactor, eliminates the multi-`.write()` cascade class — this plan eliminates the `use_effect`-spawns-future-that-writes-subscribed-signal cycle class).
> Authors: orchestrator + cycle-site audit subagent (`/tmp/poly-use-spawn-once-audit.md`).
> Last updated: 2026-04-25.

---

## 1. Why this plan exists

Poly's reactive state is Dioxus `Signal<T>` / `BatchedSignal<T>` (post-Phase-2/3). Components subscribe to state via `sig.read()` inside `use_effect`. The effect commonly spawns an async task to fetch data and write it back:

```rust
use_effect(move || {
    let snapshot = chat_data.read();      // subscribes
    if snapshot.done { return; }
    drop(snapshot);
    spawn(async move {
        load_stuff(..., chat_data, ...).await; // writes chat_data
    });
});
```

When `load_stuff` fails to reach its "done" state (e.g. a `loading=true → loading=false` toggle without populating the key field), the effect's subscribed read sees the intermediate toggle, re-fires, and spawns again. **Infinite spawn loop.** Wedges the tab — CLAUDE.md hang class #3.

**Just-shipped incident (commit `904920b9`, 2026-04-24):**
- `crates/core/src/ui/routes.rs` `ServerHome::use_effect` spawned `load_server_data_internal` for a Teams `server_id` that didn't map to any backend plugin. `load_server_data_internal` early-returned with `loading=true → loading=false`. The effect's `snapshot.loading` subscription re-fired on the release; `server_already_loaded` was still false (no backend → `current_server` never populates); it re-spawned. **SQLite-persisted BISECT trace captured 1,189,925 iterations** before it could be sampled.
- Fix: add a `spawned_for: Signal<Option<String>>` guard so the effect remembers which `server_id` it already spawned for and refuses to re-spawn for the same id.
- `ServerChat::use_effect` one function above had this guard since 2026-04-19 (that one caught a different variant of the same pattern on stale-channel URLs). `ServerHome` was missing it by accident.

**The pattern is copy-paste error-prone.** Every load-data `use_effect` needs the same boilerplate. When a contributor writes a new one and forgets, we get another wedge. The `ServerHome` / `ServerChat` divergence proves this is not a one-off.

**Strategy — same as `BatchedSignal`:** make the correct pattern the *only* expressible pattern at the API level, then lint for the raw pattern.

---

## 2. Solution summary

Introduce a `use_spawn_once<K>(key: K, f: F)` hook that internally manages the `spawned_for: Signal<Option<K>>` guard. Consumers can't forget the guard because it IS the hook.

**API sketch:**

```rust
// crates/core/src/state/use_spawn_once.rs
use std::future::Future;

/// Spawn `f(key)` exactly once per distinct `key` observed by this hook
/// instance. Re-calls with the same `key` are no-ops; calls with a new
/// `key` run `f` with the new value.
///
/// Replaces the hand-rolled `spawned_for: Signal<Option<K>>` + `use_effect` +
/// `spawn(async move { ... })` triple, which is the canonical shape of
/// CLAUDE.md hang class #3 (effect writes a signal it subscribes to, via
/// an intermediate async task).
///
/// ```ignore
/// // before
/// let mut spawned_for: Signal<Option<String>> = use_signal(|| None);
/// use_effect(move || {
///     let sid = server_id.clone();
///     if spawned_for.read().as_deref() == Some(sid.as_str()) { return; }
///     spawned_for.set(Some(sid.clone()));
///     spawn(async move { load_server_data_internal(sid, ..., chat_data, ...).await; });
/// });
///
/// // after
/// use_spawn_once(server_id.clone(), move |sid| async move {
///     load_server_data_internal(sid, ..., chat_data, ...).await;
/// });
/// ```
pub fn use_spawn_once<K, F, Fut>(key: K, f: F)
where
    K: PartialEq + Clone + 'static,
    F: FnOnce(K) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    let mut spawned_for: Signal<Option<K>> = use_signal(|| None);
    use_effect(move || {
        let already_spawned = spawned_for
            .read()
            .as_ref()
            .is_some_and(|prev| *prev == key);
        if already_spawned { return; }
        spawned_for.set(Some(key.clone()));
        let key_for_task = key.clone();
        spawn(f(key_for_task));
    });
}
```

(Note: the real impl needs `FnMut`+`Clone` or a `Box<dyn FnOnce>` strategy to satisfy Dioxus' `use_effect` closure-reuse semantics across re-renders. The design subagent will nail this down in Phase 1.)

**What it prevents:**

- Forgotten `spawned_for` guards (the canonical bug — `ServerHome` just bit us).
- Silent divergence between sibling components that should have the same guard but one of them doesn't (the `ServerChat` vs `ServerHome` divergence that lived in tree for ~1 week).
- "Guard against loading==true but nothing stops re-spawn after loading==false" (the *exact* Teams Sheep failure mode).

**What it deliberately does NOT handle:**

- Multi-key use cases where a single component needs to spawn *two* different loads independently. Use two `use_spawn_once` calls. Each gets its own guard.
- Spawned tasks that should run on every signal change (not keyed). Those are a different pattern (debounced effects) — don't use this hook.
- Non-async side effects (plain DOM manipulation in a render). Use `use_effect` directly.

---

## 3. Phases

### Phase 1 — Introduce the hook, no call-site changes — ✅ DONE (`50da1841`)

**Deliverable:** `crates/core/src/state/use_spawn_once.rs` (~150 LoC incl. tests).

Tasks:
- [x] Create `crates/core/src/state/use_spawn_once.rs` with the `use_spawn_once` hook.
- [x] Unit tests covering:
  - [x] Same `key` → exactly one spawn.
  - [x] Key change → new spawn (previous task continues; we don't cancel).
  - [x] Drop while spawned future is in-flight → future continues (caller's problem).
  - [x] Interaction with `BatchedSignal` mutations inside the spawned future.
- [x] Re-export from `crates/core/src/state/mod.rs`.
- [x] Document the hook in `docs/dev/reactive-state.md` (or create if missing) alongside `BatchedSignal`.

Verification: `cargo test -p poly-core use_spawn_once` green. WASM + native cargo check clean. No call-site diff — lands green, zero behavior change.

### Phase 2 — Migrate the 2 known correct sites — ✅ DONE (`e7abb629`)

Migrate the sites that already have the hand-rolled pattern, so they demonstrate the new hook and we have a baseline:

- [x] `crates/core/src/ui/routes.rs` `ServerChat` (L1681-region `spawned_for` + `use_effect` → `use_spawn_once`).
- [x] `crates/core/src/ui/routes.rs` `ServerHome` (just-shipped `spawned_for` from commit `904920b9` → `use_spawn_once`).

Each migration should shrink ~20 lines to ~3 lines. Net diff should be clearly smaller.

### Phase 3 — Migrate HIGH-severity sites from audit — ✅ DONE (`e7abb629`)

From `/tmp/poly-use-spawn-once-audit.md` (36 `use_effect`+`spawn` sites surveyed; 2 true cycle-risk HIGH, 6 fragile-guard MEDIUM, rest safe).

**HIGH — true feedback-loop risk, must fix:**

- [x] `crates/core/src/ui/routes.rs :: ServerMediaViewerRoute` L1475 — effect reads `chat_data.{current_server,current_channel,channels}`, spawns `restore_server_channel(chat_data, ...)` which writes those same fields. Only an `already_loaded` guard, no `spawned_for`. If the URL channel doesn't exist on the server, `already_loaded` never flips true and the spawn restarts every time another `chat_data` write lands. Migration key: the URL `channel_id`. Pattern: copy from `ServerChat::use_effect` (same file, L1694, already correct).

- [x] `crates/core/src/ui/account/common/forum_view.rs :: ForumPostView` L229 — same shape as above: reads `chat_data.{current_channel,current_server}`, spawns `restore_server_channel` that writes those fields, only `already_loaded` guard. Same failure mode on a stale forum-post deep link. Migration key: URL `channel_id` + `server_id` tuple.

For each migration:
- Replace hand-rolled guard (if any) + `use_effect` + `spawn` → single `use_spawn_once((server_id, channel_id), |(sid, cid)| async move { restore_server_channel(...).await; })`.
- Verify the spawned future still writes the signals it needs to, unchanged.
- Spot-check: grep for any newly-unused `spawned_for: Signal<Option<_>>` bindings — there shouldn't be any after these two sites, since the hook subsumes them.

### Phase 4 — Migrate MEDIUM-severity sites opportunistically — ✅ DONE (`e7abb629`)

Effects that aren't a direct-cycle bug today but are fragile — one signal shape change away from wedging:

- [x] `crates/core/src/ui/routes.rs :: DmChat` L1259 — pending-direct-call dispatch. Relies on `.take()` consuming the option as an implicit guard. Explicit key (`pending.account_id + dm_id`) is clearer.
- [x] `crates/core/src/ui/account/common/chat_view.rs :: use_search_effect` L1487 — search-on-keystroke. No cycle (writes different signal), but no debounce either — each keystroke re-spawns. Key should be `(query, channel_id)`; combine with a debounce primitive separately.
- [x] `crates/core/src/ui/account/common/chat_view.rs :: use_pinned_messages_effect` L1532 — acceptable today, flip for uniformity when touched.
- [x] `crates/core/src/ui/account/common/chat_view.rs :: use_command_preload_effect` L1652 — spawns on every channel switch; no guard. Key: `channel_id`.
- [x] `crates/core/src/ui/account/common/chat_view.rs :: use_member_list_effect` L1420 — similar shape; key: `active_channel_id`.
- [x] `crates/core/src/ui/account/common/thread_view.rs :: ThreadPanel` L289 — reads `thread_id` + `active_account_id`, spawns `get_messages`. Safe for single-thread open, fragile if ever opens concurrently. Key: `(thread_id, account_id)`.

Leave as-is for now:
- `use_header_actions_overflow_effect` (chat_view.rs L1275) — resize-driven, no cycle, no migration needed.
- All 25 LOW-severity sites.

### Phase 5 — Clippy / dylint ban on raw pattern — ✅ DONE (`8b2551e1`)

Two tracks, ship whichever is ready first:

**Track A (fast, regex CI check):**
- [x] Add `tools/scripts/forbid-use-effect-spawn-cycle.sh` — scans `crates/core/src/ui/**/*.rs` for `use_effect(move || { ... spawn(async move { ... signal.batch|write|set ... }) })` patterns.
- [x] Allowlist `tools/scripts/use-effect-spawn-cycle-allowlist.txt` for intentional cases (debounced effects, multi-key scenarios).
- [x] Wire into CI.

**Track B (proper, dylint):**
- [x] Custom lint `tools/lints/poly-lints/src/use_effect_spawn_cycle.rs` matching the AST pattern.
- [x] Exception annotation: `#[allow(poly::use_effect_spawn_cycle)]` with a required rationale comment.

### Phase 6 — Documentation + cleanup — ✅ DONE (`8b2551e1`)

- [x] Update `CLAUDE.md` §"Common WASM-hang causes" #3 to cite `use_spawn_once` as the prescribed prevention.
- [x] Add section to `docs/dev/reactive-state.md` with canonical patterns (keyed async load, debounced sync effect, non-spawning reactive effect).
- [x] Remove any remaining `spawned_for: Signal<Option<K>>` hand-rolled bindings that the lint flagged as migrations-in-progress.

---

## 4. Verification

After each phase:
- `cargo check --workspace --target wasm32-unknown-unknown` clean.
- `cargo check --workspace` (native) clean.
- `cargo test --workspace` green (excluding the pre-existing `poly-demo::capabilities::demo_plugins_match_slug_lookup_table` failure).

After Phase 2 (regression check for the known-fixed cases):
- Manual smoke-test: navigate to `/teams/localhost:9103/U001/channels/T001` (the Teams Sheep reproducer). Tab loads without wedging. (Matches behavior after commit `904920b9`.)
- Manual smoke-test: deep-link to a stale Discord channel URL. Falls back to a default channel without spawn-storm.

After Phase 5:
- Intentionally write a new `use_effect(move || { ... spawn(async { chat_data.batch(...) }) ... })` block without using `use_spawn_once`. CI fails with a pointer to this doc. Remove the test.

---

## 5. Risks / failure modes

**What `use_spawn_once` *cannot* catch at compile time:**

1. **Non-keyed spawn loops** — an effect that spawns regardless of key, writes a signal it reads. `use_spawn_once` requires a key, so these won't migrate cleanly; the lint should flag them and force either a key extraction or a different primitive.
2. **Cross-effect cycles** — effect A writes signal X, effect B reads X and writes Y, effect A reads Y. Hook-per-effect can't see the cycle. Only the lint (Phase 5) can, via call-graph analysis.
3. **Async task that itself spawns another task** — nested spawns can escape the `spawned_for` guard. Lint should flag "spawn inside spawn" in any hot-path.
4. **Key that changes on every render** — if `key` is, e.g., a `Vec<_>` derived fresh each render, `PartialEq` sees it as "new" every time → spawns every render. Anti-pattern. Lint should flag `use_spawn_once(<non-stable expression>, ...)`.

**`BatchedSignal` vs `use_spawn_once` interaction:**

After `BatchedSignal` Phase 2+3, every ChatData/AppState mutation is already channel-limited to one cascade per `.batch()` call. `use_spawn_once` prevents the *outer* infinite-spawn loop. Together, they close both hang classes #1 and #3. Hang class #2 (read guard across write) and #4 (RwLock contention) remain orthogonal — separate plans if they show up in practice.

---

## 6. Timeline estimate

| Phase | Budget | Agent tier |
|-------|--------|-----------|
| 1 — introduce hook + tests | 1 session (~2h) | sonnet-coding |
| 2 — migrate 2 known sites | 0.3 session | sonnet-coding |
| 3 — migrate HIGH audit sites | 1 session | sonnet-coding |
| 4 — opportunistic MEDIUM | 0.5 session | sonnet-coding |
| 5a — regex CI | 0.3 session | sonnet-coding |
| 5b — dylint | 2 sessions | opus-coding |
| 6 — docs + cleanup | 0.3 session | sonnet-coding |

Total: ~1 focused session for Phases 1-4 (the high-value cleanup), +1 day for Phase 5 proper lint. Phases can run after `BatchedSignal` Phases 4-6 — no hard dependency, but shared docs benefit from a unified pattern section.

---

## 7. Reference artifacts

- `/tmp/poly-use-spawn-once-audit.md` — cycle-site audit (36 sites surveyed; 2 HIGH, 6 MEDIUM, 25 LOW; source of Phase 3+4 checklists).
- `docs/plans/plan-batched-signal.md` — sister plan. Same 6-phase structure, same clippy-lint-as-final-gate model.
- Commit `904920b9` — canonical `spawned_for` guard in `ServerHome::use_effect`. The pattern this hook codifies.
- Commit `a761fe01`-era `ServerChat::use_effect` — earlier copy of the same pattern, confirming the divergence between sibling components.
- `CLAUDE.md` §"Common WASM-hang causes" #3 — the hang class being eliminated.

---

## 8. Out of scope

- **Fixing hang class #2** (read guard across a write on the same signal) — orthogonal; `BatchedSignal::batch` closes this too for the batched sites, but raw `sig.read()` + `sig.batch(...)` interleaving in unmigrated code could still trigger it. Tracked separately if observed.
- **Cancelling in-flight spawns when key changes** — out of scope. `use_spawn_once` starts a new task on a new key; the old one continues. If a consumer needs cancellation, they should design for that explicitly with a `tokio::sync::oneshot`-style cancel channel.
- **Debounced / throttled effects** — handled by a different primitive (`use_debounced_effect` already exists; not in scope here).
- **Replacing `use_effect` wholesale** — no. `use_effect` is the right primitive for plain reactive subscriptions that don't spawn. `use_spawn_once` only handles the keyed-async-load case.
