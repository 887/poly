# Plan — `.peek()` vs `.read()` Hygiene (Hang #7 Prevention)

> Status: **✅ DONE (Phases 1+2)** — `forbid-render-time-read.sh` lint shipped (`8321406d`), 988 pre-existing sites allowlisted as MEDIUM, 3 HIGH sites migrated to `.peek()`. Currently `continue-on-error: true`; Phase 5 tightening (flip to hard-fail) deferred until allowlist stabilises.
> Last updated: 2026-04-25.

---

## 1. Why this plan exists

`Signal::read()` and `BatchedSignal::read()` create a tracked subscription: the calling scope re-renders whenever the signal changes. `peek()` returns the same value WITHOUT subscribing — useful when you want to read a snapshot but don't want your scope tied to the signal's reactive graph.

CLAUDE.md hang **#7** = **using `.read()` at the top of a render body to compute a value that doesn't actually need reactive subscription** (typically a hook key, a one-shot snapshot for an event handler closure, or a value that's only ever passed through to a child component that has its own subscription).

**Real incident (just-shipped fix `55f94246`):**

```rust
fn use_member_list_effect(signals: &ChatViewSignals) {
    let app_state = signals.app_state;
    // BUG: .read() at the top of a render body subscribes ChatView
    // to every app_state write. When load_server_data's terminal
    // pending.apply() writes app_state.nav.selected_channel,
    // ChatView re-renders, this body re-runs, the read fires
    // the subscription again — perpetual loop.
    let active_channel_id = app_state.read().nav.selected_channel.cloned();
    use_spawn_once(active_channel_id, move |k| async move { … });
}
```

Bisect captured **1408** ChatView re-renders for **1** `load_server_data` call before WASM scheduler starvation. Fix: `.peek()` instead of `.read()`. The `use_spawn_once` re-evaluates its key on every legitimate ChatView re-render anyway (parent re-renders for unrelated reactive reasons), so channel switches still propagate.

This is structurally the same shape as hang #2 (live read guard across a write on the same signal) but flipped: the write is INDIRECT (downstream of an async load) and the subscription is at RENDER LEVEL not write-guard level. Six prior hang classes don't catch it.

---

## 2. Solution summary

Two layers, ordered cheapest first:

### Layer 1 — Lint (`tools/scripts/forbid-render-time-read.sh`)

Regex CI check that flags any `.read()` call at the TOP LEVEL of a render-body function (i.e., inside `pub fn ComponentName(…) -> Element` or inside a `fn use_…` hook setup function, but NOT inside a `use_effect(move || { … })` / `use_resource(move || { … })` / `use_memo(move || { … })` closure, where the subscription IS intended).

Heuristic:
- Scan `crates/core/src/ui/**/*.rs`.
- For each function whose name matches `^(fn|pub fn)\s+(use_\w+|[A-Z]\w+)\s*\(` (a hook setup or a `#[component]`).
- Find every `\b<ident>\.read\(\)` call inside that function's body that is NOT inside a `use_effect(move ||`, `use_resource(`, `use_memo(`, or `spawn(async move { … })` block.
- Flag.

Inline allowlist `// poly-lint: allow render-time-read — <reason>`. File-level allowlist `tools/scripts/render-time-read-allowlist.txt`.

Error message points at `.peek()` as the replacement and to `docs/dev/reactive-state.md` for the explanation.

### Layer 2 — Optional API hint

A `BatchedSignal::peek_field<U>(|t| -> U) -> U` convenience that's the documented "snapshot one field non-reactively" path. Mostly cosmetic — `.peek().field.clone()` already works — but reads more clearly as intent. The lint should accept either form.

**No type-system newtype option viable** — `peek` and `read` return the same `T`-shaped guard. The difference is a hidden side-effect on the reactive graph; can't be encoded in Rust types without a substantial Dioxus internals change.

---

## 3. Phases

### Phase 1 — Lint script

- [x] `tools/scripts/forbid-render-time-read.sh` per the spec above.
- [x] `tools/scripts/render-time-read-allowlist.txt`.
- [x] CI step in `.github/workflows/lint-test.yml`.
- [x] `continue-on-error: true` initially — pre-existing render-time `.read()` sites should be many; allowlist them in this same commit, flip the flag in a follow-up after Phase 2 migration.

### Phase 2 — Audit + migrate HIGH-risk sites

Audit subagent finds every render-time `.read()` in `crates/core/src/ui/**/*.rs`. Classify:
- **HIGH** — value is used to compute a hook key (`use_spawn_once`, `use_reactive_effect`, etc.) or fed into `app_state.batch` / `chat_data.batch` later in the same body. These are the canonical hang trigger shape.
- **MEDIUM** — value flows into rsx! formatting (e.g. `"{x.read().y}"`). Subscribing IS the intent — these are correct.
- **LOW** — value passed to a child component that has its own subscription. Subscribing redundant but harmless.

Migrate every HIGH site to `.peek()`.

Wave 1 HIGH migrations (commits before 2026-04-27):
- [x] `thread_view.rs` — two `.read()` keys → `.peek()`
- [x] `chat_view.rs use_search_messages_effect` → `.peek()`
- [x] `chat_view.rs use_pinned_messages_effect` → `.peek()`

Wave 2 HIGH migrations (2026-04-27):
- [x] `forum_view.rs:205` — `app_state.read().forum_scope` → `.peek()` (comment already said use peek(); now done)

Wave 2 allowlist refresh (2026-04-27):
- [x] 464 sites re-triaged after line-number drift; all confirmed MEDIUM/false-positive and added to allowlist.
- [x] Lint bug fixed: comment lines containing `.read()` were incorrectly flagged; added `^//` skip to awk filter.

### Phase 3 — Dev doc

`docs/dev/reactive-state.md` already has a section on read patterns. Extend it with:
- "When to use `.peek()` vs `.read()`."
- Concrete before/after example based on the just-fixed `use_member_list_effect`.
- Lint script reference.

### Phase 5 — Tighten lint

After Phase 2 migration shrinks the allowlist:
- Flip `continue-on-error` to `false` in CI.
- Promote the lint to warn-on-allowlist-entry (require quarterly review of remaining allowlisted sites).

---

## 4. Verification

- After Phase 1: lint script runs clean (post-allowlist).
- After Phase 2: every HIGH site migrated, allowlist shrunk to MEDIUM/LOW only.
- Manual: re-run the Teams T001 → T002 server-switch reproducer; confirm no hang.
- Synthetic test: deliberately reintroduce a `app_state.read().nav.selected_channel.cloned()` at the top of a hook, confirm CI fails.

---

## 5. Risks / failure modes

1. **False positives** — `.read()` at the top of a render body that legitimately wants the subscription (e.g., to drive conditional rendering). Allowlist with a comment.
2. **`.read()` chained with `.cloned()` is the typical pattern** — but `.peek()` returns a guard that doesn't implement `.cloned()` directly on the field path. Caller must do `.peek().field.clone()` (extra `.clone()`). Document.
3. **Inside `use_effect` closures, `.read()` IS the right choice** — that's how the effect subscribes to its deps. Lint must NOT flag those (heuristic uses the `use_effect(move ||` boundary).

---

## 6. Timeline

| Phase | Budget | Tier |
|-------|--------|------|
| 1 — lint script | 0.5 session | sonnet |
| 2 — audit + migration | 1 session | sonnet |
| 3 — dev doc | 0.2 session | sonnet |
| 5 — tighten | 0.2 session | sonnet |

---

## 7. Reference artifacts

- Commit `55f94246` — the just-fixed `use_member_list_effect` instance.
- `/tmp/poly-bisect-teams-switch.md` (if the bisect agent's report was saved) — the SQLite trace showing the 1408× re-render loop.
- `docs/plans/plan-batched-signal.md` — sister plan; hang #2 is the closest analog (read across write on same signal).
- `docs/plans/plan-use-reactive-effect.md` — also adjacent; hang #6 is the closure-capture variant.

---

## 8. Out of scope

- **Replacing `.read()` with `.peek()` in `use_effect` bodies** — those bodies WANT subscription. Lint must distinguish.
- **Deprecating `Signal::read()`** — load-bearing for `rsx!` and effect bodies.
- **Detecting cross-signal cascades** — covered by hang #2's read-guard scoping plan / lint.
