# Reactive State — Canonical Patterns

> Scope: `crates/core/src/ui/**` — Dioxus components using `BatchedSignal<T>`.
> Last updated: 2026-04-24

This document records the canonical read and write patterns for reactive signals
in the Poly UI crate, the anti-patterns that cause WASM hangs, and how to use
the inline allowlist when a deviation is intentional and safe.

---

## Section 1 — Reading signal state

### Pattern A — Immediate clone (preferred for function arguments)

Use this when you need to pass signal data to a helper function that takes `&T`.
Clone once, then pass the snapshot. The read guard is dropped before the next
statement.

```rust
// Good: guard dropped immediately after clone.
let st_snap = app_state.read().clone();
let cd_snap = chat_data.read().clone();
let user = current_account_bar_user(&st_snap, &cd_snap);
```

### Pattern B — Short-scoped block read

Use this for field extraction when you only need one or two values. Wrap in an
explicit block so the guard drops before you call any write/batch/set.

```rust
// Good: guard is dropped at the end of the block.
let voice_conn = {
    let cd = chat_data.read();
    cd.voice_connection.clone()
};
// Safe to batch-write here — cd guard is gone.
chat_data.batch(|cd| cd.voice_connection = None);
```

### Pattern C — BatchedSignal::with closure

For read-only inspection inside a closure scope. The closure borrows the inner
`T` directly and the guard is never exposed to the caller's scope.

```rust
// Good: no guard escapes the closure.
let account_count = app_state.with(|st| st.accounts.len());
```

---

## Section 2 — Writing signal state

All mutation of `BatchedSignal<T>` MUST go through `.batch(|v| ...)`. This
coalesces multiple field assignments into a single Dioxus reactive pass,
eliminating the hang-class-1 cascade.

```rust
// Good: single reactive pass regardless of how many fields change.
app_state.batch(|st| {
    st.settings_section = SettingsSection::Accounts;
    st.nav.last_visited = Some(Route::SettingsRoute);
});
```

Never hold a live read guard on the same (or a dependent) signal when calling
`.batch()`. Drop it first:

```rust
// Good: explicit drop before batch.
let cm = client_manager.read();
let demo_active = cm.demo_active;
drop(cm); // poly-lint: allow long-read-guard — explicit drop(cm) before batch, audit M1
app_state.batch(|st| st.settings_section = SettingsSection::Accounts);
```

---

## Section 3 — Anti-patterns

### Anti-pattern 1 — Bare long-scoped read guard (hang class 2)

A `let <var> = <sig>.read();` binding that is still live when a write/batch/set
on the same signal fires. On the WASM single-threaded scheduler this produces a
reactive cycle or, if the signal is a `tokio::sync::RwLock` underneath, starves
the reader indefinitely.

```rust
// BAD: `cm` guard is live when `app_state.batch(...)` fires, which may
// transitively trigger a re-read of client_manager. Even if it does not
// today, future reactive subscribers make this a time bomb.
let cm = client_manager.read();
if cm.demo_active { return; }
app_state.batch(|st| st.settings_section = SettingsSection::Accounts);
//             ^ cm still live here
```

Fix: drop before batch (see Section 2, "explicit drop" example), or extract
the value you need into a `bool`/`Clone` before releasing the guard.

### Anti-pattern 2 — `.write()` cascade (hang class 1)

Multiple consecutive `Signal::write()` guard drops trigger one Dioxus reactive
pass per drop on the WASM main thread. Five or more writes in a click handler
saturate the scheduler.

```rust
// BAD: 3 reactive passes, each scheduling a full re-render.
some_signal.write().field_a = 1;
some_signal.write().field_b = 2;
some_signal.write().field_c = 3;
```

Fix: use `.batch(|v| ...)` (see Section 2).

### Anti-pattern 3 — `.read()` inside a `use_effect` that writes the same signal

```rust
// BAD: infinite re-render loop — effect reads signal, writes signal,
// write schedules effect again, ...
use_effect(move || {
    let val = my_signal.read().value;
    if val == 0 { my_signal.write().value = 1; }  // triggers itself
});
```

Fix: move the write to a `use_future` gated by a "did-init" flag, or read the
signal once outside the effect and pass the value in via `use_memo`.

---

## Section 4 — Inline allowlist syntax

When a long-scoped guard is genuinely safe (e.g., you call `drop()` explicitly
before any write, and you have verified the signal graph has no reactive cycle),
suppress the CI lint with an inline comment on the `let` line or the `drop` line:

```rust
let cm = client_manager.read();
// ... use cm for read-only inspection ...
drop(cm); // poly-lint: allow long-read-guard — explicit drop(cm) before batch, audit M1
app_state.batch(|st| st.settings_section = SettingsSection::Accounts);
```

The inline comment token is:

```
// poly-lint: allow long-read-guard — <reason>
```

The reason is mandatory. The CI script (`tools/scripts/forbid-long-read-guard.sh`)
skips any `let` line that contains this token. For multi-site suppressions, add a
`path:line # reason` entry to `tools/scripts/long-read-guard-allowlist.txt` instead.

---

## Section 5 — References

- `crates/core/src/state/batched_signal.rs` — `BatchedSignal<T>` implementation.
- `docs/plans/plan-read-guard-scoping.md` — audit findings + migration plan.
- `docs/plans/plan-batched-signal.md` — migration history for hang class 1.
- `tools/scripts/forbid-long-read-guard.sh` — CI enforcement script.
- `tools/scripts/long-read-guard-allowlist.txt` — file/line allowlist.
- CLAUDE.md "Common WASM-hang causes" section — ranked frequency list.
