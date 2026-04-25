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

---

## Section 6 — Async / keyed effects: use_reactive_effect vs use_effect

### The stale-capture footgun (Hang #6)

Dioxus' `use_effect(move || { … })` re-runs only when **signals the closure
body reads** change. It does NOT re-run because the parent component
re-rendered with new props or local bindings. Any non-Signal value captured
at the time the closure was first created is silently frozen.

```rust
// BAD — Hang #6. `server_id` is captured by value at first render.
// Navigating from server A to server B changes the prop but the effect
// never re-fires; the stale `server_id` is read every time.
let server_id: String = props.server_id.clone();

use_effect(move || {
    do_something_with(&server_id); // silently stale after first render
});
```

This was a real bug in `use_spawn_once` (commit `09d97a01`, 2026-04-25):
the Teams server-switch crashed because the internal `use_effect` only
subscribed to its own guard signal, never to the changing `key` prop.

### Fix: use_reactive_effect for synchronous keyed effects

`use_reactive_effect<Deps>(deps, body)` mirrors `deps` into a Signal each
render so the inner `use_effect` subscription tracks it. The body re-fires
whenever `deps` changes (`PartialEq`).

```rust
// GOOD — body re-fires whenever server_id changes.
use_reactive_effect(server_id.clone(), move |sid| {
    do_something_with(&sid);
});

// Multi-dep: wrap in a tuple.
use_reactive_effect(
    (server_id.clone(), channel_id.clone()),
    move |(sid, cid)| {
        do_something_with(&sid, &cid);
    },
);
```

### Fix: use_spawn_once for async keyed spawns

For patterns that need to spawn an async task once per distinct key (e.g.,
loading server data on navigation), use `use_spawn_once` — already fixed via
commit `09d97a01` to use the same mirror-into-signal pattern internally.

```rust
// GOOD — async load fires once per distinct server_id.
use_spawn_once(server_id.clone(), move |sid| async move {
    load_server_data(sid).await;
});
```

### When raw use_effect is still fine

Use raw `use_effect` only for genuinely **one-shot mount effects** where:

1. The closure body captures **no non-Signal values** (captures only
   `Signal<T>`, `BatchedSignal<T>`, `Copy` primitives, or nothing at all).
2. Or the effect intentionally runs exactly once on mount and the captured
   snapshot is stable for the component's lifetime (e.g., a DOM event
   listener registered once and cleaned up on unmount).

When in doubt, prefer `use_reactive_effect` — over-specifying deps is safe
(worst case: a redundant body call), while under-specifying deps silently
stales the UI.

### Summary table

| Pattern | Use when |
|---------|----------|
| `use_reactive_effect(deps, body)` | Sync side-effect keyed on a prop / local binding that may change across renders. |
| `use_spawn_once(key, async_fn)` | Async task that should re-spawn when a key prop changes. |
| `use_effect(move \|\| { … })` | Genuine one-shot mount effect; closure body only reads Signals or captures no changing values. |

### CI lint

`tools/scripts/forbid-stale-effect-capture.sh` flags every `use_effect(move ||`
site in `crates/core/src/ui/**`. Existing sites that have been manually
verified as safe are listed in `tools/scripts/stale-effect-capture-allowlist.txt`.
Inline suppression: `// poly-lint: allow stale-effect-capture — <reason>`.
