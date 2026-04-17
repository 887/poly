# Plan — UI Completeness: Every Interactive Element Must Do Something

> **Created:** 2026-04-17
> **Status:** 🚧 IN PROGRESS
> **Scope:** every `onclick`, `onchange`, `onsubmit`, dropdown item, menu entry, settings section, and route target in `crates/core/src/ui/` and all `clients/*/src/`
> **Goal:** every interactive element in the app either has a real implementation, or is explicitly classified via `ui_noop!(UiNoopReason::X)` where `X` is a curated enum variant. No strings. No escape hatch for WIP. If you can't implement it, remove the button. Bare silent handlers (`onclick: |_| {}`) and empty view bodies (`rsx! {}`) are **compile errors** — same bar as `#[context_menu(...)]` and `#[connected(...)]`.

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

The root cause: **no compile-time obligation** forces a developer to implement the element
or prove it genuinely shouldn't respond.

---

## Solution Architecture

Two primitives only. No escape hatch for WIP.

### Layer 1 — Real implementation (the only path for interactive elements)

Every `onclick`, `onchange`, `onsubmit`, etc. must call real code.
If a feature is not ready: **remove the button or menu item entirely** until it is.
A button that does nothing is worse than no button — it destroys user trust on contact.

### Layer 2 — `ui_noop!(UiNoopReason::X)` (single opt-out, decorative elements only)

```rust
use poly_ui_macros::{ui_noop, UiNoopReason};

// Resize splitter — pointer drag is on the document, click has no meaning
onclick: move |_| ui_noop!(UiNoopReason::DragHandle),

// Status presence dot — reflects state, no click action is defined for this surface
onclick: move |_| ui_noop!(UiNoopReason::ReadOnlyIndicator),

// Avatar in a read-only member list row — parent row handles the click
onclick: move |_| ui_noop!(UiNoopReason::DecorativeIcon),
```

- **Runtime**: zero cost, inlined `()`. No allocation, no branch.
- **Type-safe**: the argument must be a `UiNoopReason` variant — the compiler rejects
  everything else. No strings, no integers, no booleans.
- **Not an escape hatch**: adding a new variant requires editing `UiNoopReason`, writing
  its doc comment, and getting it through code review. There is no `UiNoopReason::Todo`
  or `UiNoopReason::NotImplementedYet` — those are not valid reasons for a UI element to
  exist without an action.
- **Scanner-validated**: Rule C checks the argument is `UiNoopReason::` — any other
  expression is a violation.

### `UiNoopReason` enum (in `crates/ui-macros/src/lib.rs`)

```rust
/// Classifies why an event handler on a UI element is intentionally passive.
///
/// # Rules
/// - Every variant must describe a *structural* reason — something about the element's
///   role in the layout, not about its implementation status.
/// - "Not implemented yet" is NOT a valid reason. Remove the element instead.
/// - Adding a new variant requires a doc comment explaining the structural contract
///   and goes through normal code review.
/// - This enum is `#[non_exhaustive]` so downstream crates cannot construct it
///   directly (only `ui_noop!` consumes it).
#[non_exhaustive]
pub enum UiNoopReason {
    /// A drag-resize splitter or reorder handle. The actual interaction is delivered via
    /// `pointermove`/`pointerup` on the document root, not via `onclick` on this element.
    DragHandle,

    /// A read-only visual indicator (status dot, badge, presence ring) that reflects state
    /// but has no defined click action on this surface.
    ReadOnlyIndicator,

    /// A decorative icon or avatar rendered inside a parent row that owns the click target.
    /// Clicking the icon routes through the parent row's handler; this element does not
    /// independently handle clicks.
    DecorativeIcon,

    /// A layout spacer, separator, or divider with no interactive purpose.
    LayoutSpacer,

    /// An event barrier that exists solely to call `event.stop_propagation()` or
    /// `event.prevent_default()` — i.e. the handler body does something, but the
    /// *element* itself has no user-facing action. Use this only when the barrier
    /// logic is encoded elsewhere and this handler is truly a structural no-op.
    EventBarrier,

    /// A progress spinner or loading indicator rendered while an async operation is in
    /// flight. The element is non-interactive during this state by design; once the
    /// operation completes the element is replaced.
    ProgressIndicator,
}
```

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
  = help: use `ui_noop!(UiNoopReason::X)` if genuinely decorative
```

#### Rule B — No empty view bodies (ERROR)

Flags `#[component]` functions whose entire RSX body is `rsx! {}` or `rsx! { div {} }`
with no text nodes, no child components, and no event handlers. Also flags
`#[connected(entry_point)]` route targets that render only empty scaffolding.

**Violation message:**
```
error[poly-lint] empty view body in `NotificationsPanel` at src/ui/settings/foo.rs:12
  = help: implement the view, or remove the route entry until it is ready
```

#### Rule C — `ui_noop!` argument must be a `UiNoopReason` variant (ERROR)

Flags any `ui_noop!` call whose argument does not start with `UiNoopReason::`:
- `ui_noop!()` — missing argument
- `ui_noop!("any string")` — strings are not accepted
- `ui_noop!(42)` — integers are not accepted
- `ui_noop!(true)` — booleans are not accepted

**Violation message:**
```
error[poly-lint] ui_noop! argument must be `UiNoopReason::X` in `DragHandle` at src/ui/foo.rs:88
  = help: ui_noop!(UiNoopReason::DragHandle)
  = help: ui_noop!(UiNoopReason::ReadOnlyIndicator)
  = help: ui_noop!(UiNoopReason::DecorativeIcon)
  = note: if no variant fits, add one to `UiNoopReason` with a doc comment
```

In practice Rule C is mostly a safety net — the Rust compiler itself rejects anything
that is not a `UiNoopReason` variant because the macro signature is `($r:expr)` and
`ui_noop!` passes the value to a function typed `fn _check(_: UiNoopReason) {}`.

---

## Phases

### Phase 0 — Inventory

- [ ] **0.1** Write `scripts/audit_ui_actions.sh`: grep `onclick.*|_| {}`, `rsx! {}` bodies,
  `// TODO(phase-` comments. Emit CSV `<file, component, category, line>`.

- [ ] **0.2** Classify each row:
  - `needs_implementation` — implement it
  - `needs_removal` — remove the element until the feature is ready
  - `needs_noop` — genuinely decorative; add `ui_noop!(UiNoopReason::X)`
  - `false_positive` — scanner matched but handler is real (multi-line body)

- [ ] **0.3** Store false positives in `docs/plans/ui-action-false-positives.toml`
  keyed by file+line, same pattern as lint-gate baseline.

### Phase A — Primitives in `crates/ui-macros`

- [ ] **A.1** Add `UiNoopReason` enum to `crates/ui-macros/src/lib.rs` as documented above.
  Six initial variants: `DragHandle`, `ReadOnlyIndicator`, `DecorativeIcon`, `LayoutSpacer`,
  `EventBarrier`, `ProgressIndicator`. Mark `#[non_exhaustive]`.

- [ ] **A.2** Add `ui_noop!` macro:
  ```rust
  /// Marks an event handler as intentionally passive.
  ///
  /// The argument MUST be a `UiNoopReason` variant. Bare strings, integers, and
  /// missing arguments are compile errors — use the enum.
  ///
  /// Do NOT add a `UiNoopReason` variant for an unimplemented feature.
  /// Remove the UI element instead until the feature is ready.
  ///
  /// # Example
  /// ```
  /// onclick: move |_| ui_noop!(UiNoopReason::DragHandle),
  /// ```
  #[macro_export]
  macro_rules! ui_noop {
      ($reason:expr) => {{
          // Type-check: ensures $reason is actually a UiNoopReason.
          // Zero runtime cost — the compiler eliminates this.
          fn _assert_reason(_: $crate::UiNoopReason) {}
          _assert_reason($reason);
      }};
  }
  ```

- [ ] **A.3** Trybuild compile-fail fixtures:
  - `ui_noop!()` → compile error (missing argument)
  - `ui_noop!("DragHandle")` → compile error (string, not enum)
  - `onclick: move |_| {}` → lint error (Rule A)
  - `ui_noop!(UiNoopReason::DragHandle)` → OK

### Phase B — Lint-gate scanner `ui_action_coverage.rs`

- [ ] **B.1** `scan_empty_handlers(src: &str) -> Vec<Violation>` — Rule A
- [ ] **B.2** `scan_empty_views(src: &str) -> Vec<Violation>` — Rule B
- [ ] **B.3** `scan_invalid_noops(src: &str) -> Vec<Violation>` — Rule C (catches `ui_noop!` without `UiNoopReason::`)
- [ ] **B.4** Wire into `crates/lint-gate/build.rs` with baseline grandfathering
- [ ] **B.5** Unit tests in `crates/lint-gate/src/lib.rs`:
  - `empty_onclick_is_violation`
  - `ui_noop_with_reason_enum_is_ok`
  - `ui_noop_with_string_is_violation` (Rule C)
  - `ui_noop_without_arg_is_violation` (Rule C)
  - `nonempty_onclick_is_ok`
  - `empty_rsx_body_is_violation`
  - `rsx_with_content_is_ok`

### Phase C — Baseline grandfathering

- [ ] **C.1** `REGEN_BASELINE=1 cargo check` → produces `ui_action_baseline.toml`
- [ ] **C.2** `cargo check --workspace` passes with zero errors
- [ ] **C.3** Any new violation after baseline is seeded → immediate `cargo::error`

### Phase D — Eliminate the baseline

Three choices per violation, in priority order:

1. **Implement it** — real code. Remove baseline entry.
2. **Remove the UI element** — feature not ready. Remove baseline entry.
3. **`ui_noop!(UiNoopReason::X)`** — rare, genuinely decorative. Remove baseline entry.

Order:

- [ ] **D.1** Notification settings submenu — settings nav items that navigate to empty panels
- [ ] **D.2** Server settings submenu items (Privacy Settings, Audit Log, etc.)
- [ ] **D.3** Voice/Video toolbar buttons (mute, cam, screen share)
- [ ] **D.4** Account bar action buttons (set status, set avatar)
- [ ] **D.5** All remaining baseline entries

---

## Relationship to Existing Plans

| Plan | What it covers | What this adds |
|---|---|---|
| `plan-context-menu-quality-control` | ✅ DONE — every component has a declared menu policy | `onclick` handlers inside those components also have a declared policy |
| `plan-connected-routes-static-check` | ✅ DONE — every `Link` target is type-safe | Route target components that render empty bodies are now flagged |
| `plan-component-lints` | ✅ DONE — 150-line component rule | N/A |

---

## Design Decisions

**Why an enum instead of a string?**
A string is a free-text escape hatch. An enum is a closed set of pre-approved reasons
that each require a doc comment and code review to add. You cannot type
`UiNoopReason::NotImplementedYet` because that variant does not exist. You cannot add it
without a reviewer asking "why does this structural role justify a no-op?" Strings allow
`"not implemented"` to slip in; the enum structurally prevents it.

**Why no `not_implemented!` macro at all?**
Any soft escape hatch gets used as a crutch — by AI and humans alike — to silence the
compiler instead of doing the work. The correct response to an unimplemented feature is:
don't add the button. A missing button is invisible. A button that silently does nothing
is a broken product. The type system enforces this.

**Why scanner over `Action<T>` type wrapper?**
Wrapping every `onclick` in a type-safe `Action<T>` would require touching 350+ call
sites in one pass. The scanner grandfathers existing code and tightens the net
incrementally — the same strategy that shipped the context-menu plan in one session.

---

## Acceptance Criteria

- [ ] `cargo check --workspace` passes with zero errors
- [ ] `onclick: move |_| {}` → `cargo check` fails
- [ ] `rsx! {}` body on a `#[component]` → `cargo check` fails
- [ ] `ui_noop!("any string")` → compile error
- [ ] `ui_noop!()` → compile error
- [ ] `ui_noop!(UiNoopReason::DragHandle)` → compiles cleanly
- [ ] Every previously-silent button either has real code or the element has been removed
- [ ] `cargo test -p poly-lint-gate` passes all scanner unit tests
- [ ] `cargo test -p poly-ui-macros` passes all trybuild compile-fail fixtures
