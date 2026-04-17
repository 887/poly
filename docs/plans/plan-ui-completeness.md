# Plan — UI Completeness: Every Interactive Element Must Do Something

> **Created:** 2026-04-17
> **Status:** 🚧 IN PROGRESS
> **Scope:** every `onclick`, `onchange`, `onsubmit`, dropdown item, menu entry, settings section, and route target in `crates/core/src/ui/` and all `clients/*/src/`
> **Goal:** every interactive element in the app either has a real implementation, or is explicitly documented as decorative-only with `ui_noop!("specific reason this element is passive")`. There is no escape hatch for unimplemented features — if you can't implement it, don't add the button yet. Bare silent event handlers (`onclick: |_| {}`) and empty view bodies (`rsx! {}`) are **compile errors** — the same bar we set for `#[context_menu(...)]` and `#[connected(...)]`.

---

## The Problem

Right-clicking Notification Settings (or a dozen other surfaces) silently does nothing.
Some settings tabs, submenu items, and toolbar buttons have no handler wired — the user
clicks and nothing happens, with no feedback. There is no compile-time signal that an
element was forgotten. Contrast this with the type system guarantees we now have for
context menus and route connections.

### Categories of broken/silent UI (discovered 2026-04-17 audit)

| Category | Pattern | Count (approx) | Symptom |
|---|---|---|---|
| Empty onclick | `onclick: move \|_\| {}` or `onclick: \|_\| ()` | ~20–40 | Click does nothing, no feedback |
| Stub view body | `rsx! {}` or `rsx! { div {} }` with no text/content | ~10–15 | Route renders blank panel |
| TODO-flagged features | `// TODO(phase-X.X.X)` at top of component file | ~12 | Feature exists but is inert |
| Unlinked settings rows | Settings nav item → section renders empty | ~5–8 | Scrolls to blank area |
| Broken submenu chains | Menu item fires, then nothing happens or navigates to stub | ~6–10 | Menu closes, nothing changes |

The root cause in all cases: **no compile-time obligation** forces a developer to implement
the element or prove it genuinely shouldn't respond.

---

## Solution Architecture

Two primitives only. No escape hatch.

### Layer 1 — Real implementation (the only path forward for interactive elements)

Every `onclick`, `onchange`, `onsubmit`, etc. must call real code.
If the feature is not ready: **remove the button or the menu item entirely** until it is.
A button that does nothing is worse than no button — it actively destroys user trust.

### Layer 2 — `ui_noop!("specific reason")` (the single opt-out, for decorative elements only)

```rust
// Draggable resize handle — interaction is handled via pointermove/pointerup, not onclick
onclick: move |_| ui_noop!("resize handle: drag is on pointermove, click is a no-op"),

// Visual-only status dot that mirrors state, clicking it would have no meaning
onclick: move |_| ui_noop!("presence dot: read-only status indicator, no action defined"),
```

- **Runtime**: literally `()` — zero cost, fully inlined.
- **Requires a non-empty string literal** explaining *specifically* why this element is passive.
  Generic cop-outs (`"decorative"`, `"not implemented"`, `"TODO"`, `"placeholder"`) are
  scanner-rejected as violations in their own right — they're the same as leaving it empty.
- **Compile-time**: scanner treats a valid `ui_noop!("specific reason")` as deliberate
  opt-out and does not flag it.
- **Code review signal**: any `ui_noop!` in a diff is immediately visible and requires
  justification. It cannot be used to silence a WIP feature — that's what removing the
  UI element is for.

### Layer 3 — Lint-gate scanner `ui_action_coverage.rs`

Extends `crates/lint-gate/build/` (same pattern as `context_menu_coverage.rs`).

#### Rule A — No bare empty event handlers (ERROR)

Flags all of:
```
onclick: move |_| {}
onclick: |_| {}
onclick: move |_| ()
onchange: move |_| {}
onsubmit: |_| {}
```

**Violation message:**
```
error[poly-lint] empty event handler in `SomeComponent` at src/ui/foo.rs:42
  = help: implement the handler, or
  = help: remove the element if the feature is not ready, or
  = help: use `ui_noop!("specific reason this is passive")` if genuinely decorative
```

No mention of `not_implemented!` anywhere. There is no such macro.

#### Rule B — No empty view bodies (ERROR)

Flags:
- `#[component]` function whose entire RSX body is `rsx! {}` or `rsx! { div {} }` with
  no text nodes, no child components, and no event handlers
- Route-target components (`#[connected(entry_point)]`) that render only empty scaffolding

**Violation message:**
```
error[poly-lint] empty view body in `NotificationsPanel` at src/ui/settings/foo.rs:12
  = help: implement the view, or remove the route entry until it is ready
```

#### Rule C — `ui_noop!` reason quality check (ERROR)

Flags `ui_noop!` calls where the reason string is:
- Empty: `ui_noop!("")`
- A known cop-out word: `"decorative"`, `"todo"`, `"TODO"`, `"placeholder"`,
  `"not implemented"`, `"noop"`, `"none"`, `"fixme"`, `"wip"`
- Fewer than 15 characters (too short to be specific)

**Violation message:**
```
error[poly-lint] ui_noop! reason is too vague in `DragHandle` at src/ui/foo.rs:88
  = help: explain specifically why this element is passive
  = note: bad: ui_noop!("decorative")
  = note: good: ui_noop!("resize splitter: drag is handled via pointermove on the parent")
```

---

## Phases

### Phase 0 — Inventory (build the baseline)

- [ ] **0.1** Write `scripts/audit_ui_actions.sh`: grep `onclick.*|_| {}`, `rsx! {}` bodies,
  `// TODO(phase-` comments. Emit CSV of `<file, component, category, line>` for every
  candidate violation. This is the ground truth for the baseline.

- [ ] **0.2** Hand-verify the CSV. Classify each row into:
  - `needs_implementation` — the feature should exist; implement it
  - `needs_removal` — the UI element is ahead of the feature; remove it until ready
  - `needs_noop` — genuinely decorative passive element; add `ui_noop!("specific reason")`
  - `false_positive` — scanner pattern matched but handler is correct (body spans multiple
    lines, scanner missed it)

- [ ] **0.3** Store the false-positive list as `docs/plans/ui-action-false-positives.toml`
  — the scanner will skip these by file+line key (same pattern as lint-gate baseline).

### Phase A — `ui_noop!` primitive in `crates/ui-macros`

- [ ] **A.1** Add `ui_noop!` macro to `crates/ui-macros/src/lib.rs`:
  ```rust
  /// Explicitly marks an event handler as intentionally passive.
  ///
  /// REQUIRES a specific, non-vague reason string explaining why this element
  /// does not respond to user interaction. Generic reasons ("decorative", "TODO",
  /// "placeholder") are rejected by the lint-gate scanner as violations.
  ///
  /// If you are tempted to write `ui_noop!("not implemented yet")` — don't.
  /// Remove the UI element instead until the feature is ready.
  ///
  /// # Example
  /// ```
  /// // Good — specific reason that would survive code review:
  /// onclick: move |_| ui_noop!("status dot is read-only; no click action is defined"),
  ///
  /// // Bad — will be flagged as a violation same as an empty handler:
  /// onclick: move |_| ui_noop!("TODO"),
  /// ```
  #[macro_export]
  macro_rules! ui_noop {
      ($reason:literal) => { () };
  }
  ```
  The `$reason` is consumed at compile time only. Zero runtime cost.

- [ ] **A.2** Add trybuild compile-fail fixtures:
  - Bare `onclick: move |_| {}` → compile error (Rule A)
  - `ui_noop!("")` → scanner violation (Rule C)
  - `ui_noop!("TODO")` → scanner violation (Rule C)
  - `ui_noop!("specific reason the drag handle is passive")` → OK

### Phase B — Lint-gate scanner `ui_action_coverage.rs`

- [ ] **B.1** Create `crates/lint-gate/build/ui_action_coverage.rs`:
  - `scan_empty_handlers(src: &str) -> Vec<Violation>` — Rule A
  - `scan_empty_views(src: &str) -> Vec<Violation>` — Rule B
  - `scan_vague_noops(src: &str) -> Vec<Violation>` — Rule C
  - No `count_not_implemented` — that macro does not exist

- [ ] **B.2** Wire into `crates/lint-gate/build.rs` (same pattern as context_menu_coverage):
  ```rust
  mod ui_action_coverage;
  // In main scan loop: collect violations, apply baseline, emit cargo::error
  ```

- [ ] **B.3** Unit tests in `crates/lint-gate/src/lib.rs`:
  - `empty_onclick_is_violation` — `onclick: move |_| {}` flagged
  - `ui_noop_with_good_reason_is_ok` — `ui_noop!("resize handle: …")` not flagged
  - `ui_noop_with_todo_reason_is_violation` — `ui_noop!("TODO")` flagged (Rule C)
  - `ui_noop_with_short_reason_is_violation` — `ui_noop!("noop")` flagged (Rule C)
  - `nonempty_onclick_is_ok` — multi-line real handler not flagged
  - `empty_rsx_body_is_violation` — bare `rsx! {}` component body flagged
  - `rsx_with_content_is_ok` — component with real content not flagged

### Phase C — Baseline grandfathering

- [ ] **C.1** Run scanner with `REGEN_BASELINE=1` to produce
  `crates/lint-gate/build/ui_action_baseline.toml`. This grandfathers existing violations
  so `cargo check` passes immediately on landing day.

- [ ] **C.2** Verify `cargo check --workspace` passes with zero errors after the scanner lands.

- [ ] **C.3** From this point: **any new empty handler or empty view body in any file is a
  `cargo::error`**. The baseline only covers lines that existed when the scanner was seeded.

### Phase D — Eliminate the baseline (implement or remove each violation)

Work through `ui_action_baseline.toml`. For each entry, choose one path only:

1. **Implement it** — add real code. Remove the baseline entry.
2. **Remove the UI element** — if the feature is not ready. Remove the baseline entry.
3. **`ui_noop!("specific reason")`** — only for genuinely decorative passive elements.
   Remove the baseline entry. This path should be rare; most UI elements should do something.

Priority order:

- [ ] **D.1** Notification settings submenu — all settings nav items that navigate to empty panels
- [ ] **D.2** Server settings submenu items (Privacy Settings, Audit Log, etc.)
- [ ] **D.3** Voice/Video call toolbar buttons (mute, cam, screen share)
- [ ] **D.4** Account bar action buttons (set status, set avatar)
- [ ] **D.5** All remaining baseline entries — sweep and close

---

## Relationship to Existing Plans

| Plan | Overlap | This plan adds |
|---|---|---|
| `plan-context-menu-quality-control` | ✅ DONE — every component declares a menu policy | Buttons/onclick handlers *within* those components also have a declared policy |
| `plan-connected-routes-static-check` | ✅ DONE — every `Link` target is type-safe | Route *target components* that render empty bodies are now flagged |
| `plan-component-lints` | ✅ DONE — 150-line component rule | N/A |

The three done plans ensure correct *structure*. This plan ensures correct *behavior*:
every element that looks interactive actually does something, or is explicitly proven
to be passive with a specific justification.

---

## Design Decisions

**Why no `not_implemented!` macro?**
Any "soft" escape hatch — a macro that compiles and shows a toast — will be used as a
crutch. AI and humans alike will reach for it to silence the compiler rather than doing
the work. The answer is: don't add the UI element until the feature exists. A missing
button is invisible; a button that does nothing is a broken product. The type system
should enforce this, not merely track it.

**Why require a reason string in `ui_noop!`?**
Without a mandatory reason, `ui_noop!()` becomes the new empty closure — a one-keystroke
escape hatch. With a required specific string, every `ui_noop!` in a diff demands a
human-readable justification that survives code review. The length and cop-out-word checks
make it painful to abuse: writing `"resize splitter: interaction is via pointermove on
the document, onclick is structurally unreachable"` takes effort; that effort is the point.

**Why scanner over a type-wrapper like `Action<T>`?**
A `UiAction` wrapper type would require touching every `onclick` in the codebase (350+
occurrences) in one pass. The scanner approach grandfathers existing code and tightens
the net incrementally — the same strategy that made the context-menu plan shippable.

---

## Acceptance Criteria

- [ ] `cargo check --workspace` passes with zero errors
- [ ] Any new `onclick: move |_| {}` in any file causes `cargo check` to fail with a clear error
- [ ] Any new `#[component]` with `rsx! {}` body causes `cargo check` to fail
- [ ] `ui_noop!("TODO")` and `ui_noop!("")` cause `cargo check` to fail
- [ ] `ui_noop!("resize handle: drag is on pointermove, click is structurally unreachable")` passes
- [ ] Every previously-silent button either has real code or the UI element has been removed
- [ ] `cargo test -p poly-lint-gate` passes all scanner unit tests
- [ ] `cargo test -p poly-ui-macros` passes all trybuild compile-fail fixtures
