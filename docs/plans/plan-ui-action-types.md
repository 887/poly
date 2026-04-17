# Plan — Typed UI Action Enums: Compile-Time Behavioral Contracts

> **Created:** 2026-04-17
> **Status:** 🚧 IN PROGRESS
> **Scope:** every interactive `#[component]` in `crates/core/src/ui/` and `clients/*/src/`
> **Goal:** every `#[component]` with interactive elements declares a closed `enum` of the semantic actions it can perform, annotated with `#[ui_action(SomeEnum)]`. The enum implements `UiAction`, whose `apply()` method is an exhaustive match — adding a button requires adding a variant, and adding a variant requires handling it. Behavior is now named, documented, and unit-testable without rendering. This is the semantic layer above the structural guarantees already provided by `plan-ui-completeness`.

---

## The Problem

The three completed plans enforce *structure*: no empty handlers, no orphaned routes, no
missing menu policies. What they can't enforce is *semantics* — that a handler does the
right thing, or anything at all beyond compile.

A rogue or careless agent can today write:

```rust
fn handle_save_settings() {
    todo!("phase-X: implement")
}

onclick: move |_| handle_save_settings(),
```

All lints pass. The handler has a name. Nothing is saved. The action is invisible — there
is no declaration anywhere of what this component *intends* to do.

With typed action enums:

```rust
enum NotificationSettingsAction {
    SetAllMessages,
    SetMentionsOnly,
    SetNothing,
    ToggleSuppressEveryone(bool),
    ToggleMobilePush(bool),
    Save,
}

impl UiAction for NotificationSettingsAction {
    fn apply(self, cx: ActionCx<'_>) {
        match self {
            Self::Save => todo!("phase-3.x: persist notification settings"),
            // ... all variants must be handled — Rust enforces this
        }
    }
}
```

Now:
- Adding a button **requires** adding a variant.
- Adding a variant **requires** a match arm in `apply()` — Rust rejects the build otherwise.
- Every component's behavioral contract is **in one place**, readable without rendering the app.
- `apply()` takes plain `&mut AppState` — **unit-testable** without Dioxus, without WASM.
- `todo!()` is still legal but it is now **named and located** — grep `todo!` in any action enum and you have the complete WIP surface.

---

## Ceiling Acknowledgement

This plan closes the *structural behavioral* gap. It does not close *semantic correctness*:

| What this fixes | What it cannot fix |
|---|---|
| Unnamed / undeclared actions | Wrong logic inside `apply()` |
| Missing match arms (Rust compile error) | `todo!()` stubs that pass all lints |
| Untestable inline closures | Correctness of the state transition |
| No declared action contract | Subtle business logic bugs |

The layer below this (correctness of `apply()` bodies) is covered by unit tests on the
action enums themselves and by the MCP smoke-test loop. The type system has done its
job by the time we reach `apply()`.

---

## Solution Architecture

### The `UiAction` Trait (in `crates/ui-types`)

```rust
/// A closed set of semantic actions a component can perform.
///
/// Implement this for an enum that lists every user-triggered action your
/// component handles. `apply()` is an exhaustive match — Rust will reject
/// any build where a variant is not handled.
///
/// # Contract
/// - Every variant that a button / toggle / select in the component can
///   trigger must appear in the enum.
/// - `apply()` must handle every variant. `todo!()` is permitted for WIP
///   but is visible and greppable.
/// - `ActionCx` carries the mutable app state and navigator; no other
///   global access is needed for most actions.
///
/// # Testing
/// Because `apply()` takes `ActionCx` (not a Dioxus context), you can
/// unit-test every action variant without rendering:
/// ```
/// let mut state = AppState::default();
/// NotifSettingsAction::ToggleMobilePush(true)
///     .apply(ActionCx::test(&mut state));
/// assert!(state.notif.mobile_push);
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not implement `UiAction`",
    label = "add `impl UiAction for {Self} {{ fn apply(self, cx: ActionCx<'_>) {{ ... }} }}`",
    note = "every variant must be handled — use `todo!(\"phase-X: ...\")` for WIP items"
)]
pub trait UiAction: Sized + 'static {
    fn apply(self, cx: ActionCx<'_>);
}
```

### `ActionCx` — Context Passed to `apply()`

```rust
/// Context available to every `UiAction::apply()` call.
///
/// Intentionally minimal — only what actions actually need.
/// Extend via `impl ActionCx` methods rather than adding fields.
pub struct ActionCx<'a> {
    /// Mutable app-wide state.
    pub state: &'a mut AppState,
    /// Dioxus navigator for route transitions.
    pub navigator: Navigator,
}

impl<'a> ActionCx<'a> {
    /// Construct a test context from a plain `&mut AppState`.
    /// Use in unit tests — does not require a Dioxus runtime.
    pub fn test(state: &'a mut AppState) -> Self {
        Self { state, navigator: Navigator::stub() }
    }
}
```

### `dispatch_action!` Macro (in `crates/ui-types`)

```rust
/// Dispatch a typed action from an event handler.
///
/// The action must implement `UiAction`. `$state` must be a captured
/// `Signal<AppState>` from the component scope.
///
/// # Example
/// ```rust
/// let mut state = use_context::<Signal<AppState>>();
/// let nav = use_navigator();
///
/// onclick: move |_| dispatch_action!(NotifSettingsAction::Save, state, nav),
/// ```
#[macro_export]
macro_rules! dispatch_action {
    ($action:expr, $state:expr, $nav:expr) => {{
        fn _assert_ui_action<T: $crate::UiAction>(_: &T) {}
        _assert_ui_action(&$action);
        $action.apply($crate::ActionCx {
            state: &mut $state.write(),
            navigator: $nav.clone(),
        });
    }};
}
```

### The `#[ui_action(...)]` Attribute (in `crates/ui-macros`)

Three variants, mirroring `#[context_menu(...)]`:

```rust
// This component's semantic actions are typed as `NotificationSettingsAction`
#[ui_action(NotificationSettingsAction)]
#[context_menu(None)]
#[component]
fn NotificationsPanel(account_id: String) -> Element { ... }

// Display-only component — no semantic actions
// All event handlers must be ui_noop!(UiNoopReason::X)
#[ui_action(None)]
#[context_menu(None)]
#[component]
fn StatusBadge(online: bool) -> Element { ... }

// Sub-component that delegates actions to its parent
// Does not define its own action type
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn NotifToggleRow(label: String, checked: bool, on_toggle: EventHandler<bool>) -> Element { ... }
```

### Lint-gate Scanner `action_enum_coverage.rs`

Extends `crates/lint-gate/build/` (same pattern as `context_menu_coverage.rs` and
`ui_action_coverage.rs`).

#### Rule A-enum — Every `#[component]` must have `#[ui_action(...)]` (ERROR)

Same coverage-counter pattern as context_menu. Missing annotation → build error.

```
error[poly-lint] missing #[ui_action(...)] on `NotificationsPanel` at src/ui/settings/notifications.rs:42
  = help: add #[ui_action(SomeActionEnum)] — declare what this component can do
  = help: add #[ui_action(None)]           — display-only, no semantic actions
  = help: add #[ui_action(inherit)]        — sub-component, delegates to parent
```

#### Rule B-enum — `#[ui_action(None)]` + real event handler (ERROR)

Cross-checks with `ui_action_coverage.rs`: if a component declares `None` but has an
`onclick` that isn't `ui_noop!(...)`, that is a contradiction and a build error.

```
error[poly-lint] `#[ui_action(None)]` component `StatusBadge` has a non-noop event handler at src/ui/foo.rs:88
  = help: either change to #[ui_action(SomeEnum)] and implement the action
  = help: or change the handler to ui_noop!(UiNoopReason::X)
```

#### Rule C-enum — Coverage counter (WARNING)

```
cargo::warning=poly-action-coverage: 47 components declare #[ui_action(SomeEnum)] (N remaining without)
```

---

## Phases

### Phase 0 — Inventory

- [ ] **0.1** Grep all `#[component]` functions in `crates/core/src/ui/` and `clients/*/src/`.
  Classify each as:
  - `needs_action_enum` — has real interactive elements, needs a typed action enum
  - `needs_none` — display-only, `#[ui_action(None)]`
  - `needs_inherit` — sub-component, `#[ui_action(inherit)]`

- [ ] **0.2** Identify the highest-value components for Phase E priority:
  settings sections, toolbar buttons, modal dialogs, context menu hosts.

### Phase A — Primitives in `crates/ui-types`

- [ ] **A.1** Add `UiAction` trait with `#[diagnostic::on_unimplemented]`.

- [ ] **A.2** Add `ActionCx<'a>` struct with `state: &'a mut AppState` and `navigator: Navigator`.
  Add `ActionCx::test(state)` constructor for unit tests (uses a stub navigator).

- [ ] **A.3** Add `dispatch_action!` macro.

- [ ] **A.4** Add `Navigator::stub()` if not already available — a no-op navigator for test contexts.
  If Dioxus doesn't provide one, wrap behind a `#[cfg(test)]` shim.

- [ ] **A.5** Trybuild compile-fail fixtures:
  - `dispatch_action!(NotAUiAction, state, nav)` → compile error (type doesn't impl `UiAction`)
  - `impl UiAction for MyEnum { fn apply(self, cx: ActionCx) {} }` with non-exhaustive match → compile error (Rust enforcement, not scanner)

### Phase B — `#[ui_action(...)]` Proc-macro in `crates/ui-macros`

- [ ] **B.1** Add `ui_action` proc-macro to `crates/ui-macros/src/lib.rs`.
  Parse three variants: `SomeIdent`, `None`, `inherit`. Emit a no-op attribute (same as
  `context_menu` — the attribute is a marker for the scanner; it does not generate code).

- [ ] **B.2** Register `#[diagnostic::on_unimplemented]` on the `UiAction` trait so that
  `#[ui_action(SomeEnum)]` on a component where `SomeEnum` doesn't impl `UiAction` produces
  a human-readable compile error rather than a cryptic type error.

- [ ] **B.3** Trybuild compile-fail fixtures:
  - `#[ui_action()]` → parse error
  - `#[ui_action(Foo, Bar)]` → parse error (only one argument)
  - `#[ui_action(unknown_keyword)]` → if not an ident that resolves to a type, scanner violation

### Phase C — Lint-gate Scanner `action_enum_coverage.rs`

- [ ] **C.1** Create `crates/lint-gate/build/action_enum_coverage.rs`:
  - `scan_missing_annotation` — Rule A-enum: every `#[component]` must have `#[ui_action(...)]`
  - `scan_none_with_real_handler` — Rule B-enum: `#[ui_action(None)]` + non-noop handler
  - `count_typed_components` — Rule C-enum: coverage counter

- [ ] **C.2** Wire into `crates/lint-gate/build.rs` (same pattern as existing scanners).

- [ ] **C.3** Unit tests in `crates/lint-gate/src/lib.rs`:
  - `missing_ui_action_is_violation`
  - `ui_action_none_is_ok`
  - `ui_action_inherit_is_ok`
  - `ui_action_typed_is_ok`
  - `ui_action_none_with_onclick_is_violation`

### Phase D — Baseline Grandfathering

- [ ] **D.1** `REGEN_BASELINE=1 cargo check` → seeds baseline with all existing
  `#[component]`s that are missing `#[ui_action(...)]`. Same mechanism as all prior plans.

- [ ] **D.2** `cargo check --workspace` passes with zero errors after scanner lands.
  Output: `cargo::warning=poly-action-coverage: N components need #[ui_action(...)]`.

- [ ] **D.3** Any new `#[component]` without `#[ui_action(...)]` is immediately a `cargo::error`.

### Phase E — Implement Action Enums (work through the baseline)

For each component in the baseline, choose one:

1. **Define a typed enum** — implement `UiAction`, add `#[ui_action(SomeEnum)]`. Remove baseline entry.
2. **`#[ui_action(None)]`** — component is display-only. Verify all handlers are `ui_noop!`. Remove baseline entry.
3. **`#[ui_action(inherit)]`** — sub-component. Remove baseline entry.

Priority order:

- [ ] **E.1** Settings sections (notifications, voice, theme, language, general, backup, identity) —
  highest value, most likely to have silent `todo!()` stubs.
- [ ] **E.2** Modal dialogs (confirm delete, create server, add account, etc.)
- [ ] **E.3** Toolbar buttons (voice bar, chat compose toolbar, search)
- [ ] **E.4** Sidebar rows and nav items
- [ ] **E.5** All remaining baseline entries — sweep and close

---

## Relationship to Existing Plans

| Plan | What it covers | What this adds |
|---|---|---|
| `plan-ui-completeness` ✅ | No empty handlers, `ui_noop!(UiNoopReason::X)` for passive elements | Named closed action sets; exhaustive match on `apply()` |
| `plan-context-menu-quality-control` ✅ | Every component has a declared menu policy | Every component also has a declared *action* policy |
| `plan-connected-routes-static-check` ✅ | Every `Link` is type-safe | Route *destinations* are typed; now the *actions that trigger navigation* are also typed |
| `plan-component-lints` ✅ | 150-line component rule | Action enums are naturally small and testable — complements the decomposition rule |

---

## What Rogue Agents Can No Longer Do Silently

| Before this plan | After |
|---|---|
| Add a button with an empty or stub closure — no record of what it was supposed to do | Must declare a variant in the component's action enum — intent is named |
| Stub out multiple actions with scattered `todo!()` comments | All `todo!()` stubs land in one `apply()` match — greppable, auditable |
| Claim a feature is implemented when `apply()` is a no-op | `apply()` has a match arm per variant — reviewer sees exactly what is and isn't done |
| Write an action that is untestable (closes over component signals) | `apply()` takes `ActionCx` — plain unit-testable function |

---

## Design Decisions

**Why `ActionCx` instead of passing `Signal<AppState>` directly?**
`ActionCx` is a named boundary. If actions later need a second piece of context (a message queue, an analytics sink), we add it to `ActionCx` without changing every `apply()` signature. It also makes `ActionCx::test()` a single place to stub.

**Why `dispatch_action!` instead of calling `apply()` directly?**
The macro provides a type check at the call site (`_assert_ui_action`) and a single place to add cross-cutting concerns (logging, undo stack, analytics) later without touching every handler. Calling `apply()` directly is also fine and produces the same behavior — the macro is sugar.

**Why `inherit` instead of requiring every sub-component to declare its own enum?**
Sub-components (toggle rows, icon buttons) typically don't have independent semantic actions — they take `EventHandler<T>` props and bubble up. Forcing them to declare an action enum would mean hundreds of single-variant enums. `inherit` is the explicit declaration that "this component's actions are defined by its parent."

**Why not require `dispatch_action!` instead of allowing direct `apply()` calls?**
Mandating the macro in every handler is maximally invasive. The value — named variants, exhaustive match — is already there via the enum + trait pattern. Enforcement of `dispatch_action!` at every call site is Phase 2, after Phase E has wired up the enums and we can measure false-positive rates.

---

## Acceptance Criteria

- [ ] `cargo check --workspace` passes with zero errors
- [ ] Any new `#[component]` without `#[ui_action(...)]` → `cargo::error`
- [ ] `#[ui_action(None)]` + real onclick → `cargo::error`
- [ ] Any `SomeEnum` passed to `#[ui_action(...)]` that doesn't impl `UiAction` → compile error with readable message (via `#[diagnostic::on_unimplemented]`)
- [ ] Non-exhaustive match in any `UiAction::apply()` → Rust compile error
- [ ] Every settings section has a typed action enum with a unit test per variant
- [ ] `cargo test -p poly-lint-gate` passes all scanner unit tests
- [ ] `cargo test -p poly-ui-types` passes all trybuild compile-fail fixtures
