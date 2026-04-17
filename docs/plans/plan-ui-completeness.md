# Plan — UI Completeness: Every Interactive Element Must Do Something

> **Created:** 2026-04-17
> **Status:** 🚧 IN PROGRESS
> **Scope:** every `onclick`, `onchange`, `onsubmit`, dropdown item, menu entry, settings section, and route target in `crates/core/src/ui/` and all `clients/*/src/`
> **Goal:** every interactive element in the app either has a real implementation, calls `not_implemented!("Human-readable description")` (shows a friendly toast + is counted by lint-gate), or is explicitly opted out with `ui_noop!()`. Bare silent event handlers (`onclick: |_| {}`) and empty view bodies (`rsx! {}`) are **compile errors** — the same bar we set for `#[context_menu(...)]` and `#[connected(...)]`.

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

The root cause in all cases: **no compile-time obligation** forces a developer to either
ship an implementation or mark the element explicitly as work-in-progress.

---

## Solution Architecture

Three layered primitives that mirror the pattern used for context menus and connected routes:

### Layer 1 — `not_implemented!("desc")` macro (the escape hatch)

```rust
// In any onclick, onchange, view body, etc.:
onclick: move |_| not_implemented!("Notification push settings — phase-3.x"),
```

- **Runtime**: dispatches a toast via `AppState` → user sees `"⚠ Not yet implemented: {desc}"`
- **Compile-time**: the lint-gate scanner counts these. With `strict-actions` cargo feature,
  each call emits `cargo::warning` (tracked), never silently ignored.
- **NOT a panic**. Unlike `todo!()` / `unimplemented!()`, this is user-safe and survives in
  production builds. It is the sanctioned "ship this now, track it" pattern.

### Layer 2 — `ui_noop!()` macro (the intentional no-op)

```rust
// For decorative elements that intentionally swallow events:
onclick: move |_| ui_noop!(),     // drag handle — only responds to pointermove
onchange: move |_| ui_noop!(),    // display-only select
```

- **Runtime**: does nothing (zero cost, inlined).
- **Compile-time**: scanner treats this as a deliberate opt-out — not flagged as a violation.
- Forces the developer to *consciously decide* "this element intentionally does nothing"
  rather than leaving an empty closure.

### Layer 3 — Lint-gate scanner `ui_action_coverage.rs`

Extends `crates/lint-gate/build/` (same pattern as `context_menu_coverage.rs`).

#### Rule A — No bare empty event handlers (ERROR)

Catches all of:
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
  = help: use `not_implemented!("description")` for WIP features
  = help: use `ui_noop!()` to explicitly opt out
  = help: implement the handler
```

#### Rule B — No empty view bodies (ERROR)

Catches:
- `#[component]` function whose entire RSX body is `rsx! {}` or `rsx! { div {} }` with
  no text nodes, no child components, and no event handlers
- Route-target components (detected by cross-referencing `route_graph.toml` or
  `#[connected(entry_point)]` annotation) that render only empty scaffolding

**Violation message:**
```
error[poly-lint] empty view body in `NotificationsPanel` at src/ui/settings/foo.rs:12
  = help: add content, or call `not_implemented!("description")` to render a placeholder
```

#### Rule C — `not_implemented!` call counter (WARNING / tracking)

The scanner collects all `not_implemented!("...")` call sites, reports:
```
cargo::warning=poly-action-coverage: 47 not_implemented!() calls remain (target: 0)
```

This is the progress dashboard. It decreases as features ship. It is a `warning`, not an
error, so it never blocks builds — it just keeps the debt visible.

---

## Phases

### Phase 0 — Inventory (build the baseline)

- [ ] **0.1** Write `scripts/audit_ui_actions.sh`: grep `onclick.*|_| {}`, `rsx! {}` bodies,
  `// TODO(phase-` comments. Emit CSV of `<file, component, category, line>` for every
  candidate violation. This is the ground truth for the baseline.

- [ ] **0.2** Hand-verify the CSV. Classify each row into:
  - `needs_not_implemented` — real WIP feature, needs the macro + toast
  - `needs_noop` — intentional passive element
  - `needs_implementation` — should have been implemented, low effort to add
  - `false_positive` — scanner pattern matched but handler is correct (e.g., handler body
    spans multiple lines, scanner missed it)

- [ ] **0.3** Store the false-positive list as `docs/plans/ui-action-false-positives.toml`
  — the scanner will skip these (by file+line key, same pattern as lint-gate baseline).

### Phase A — Primitives in `crates/ui-macros`

- [ ] **A.1** Add `not_implemented!` proc-macro (or `macro_rules!`) to `crates/ui-macros/src/lib.rs`:
  ```rust
  /// Show a "Not yet implemented" toast and log a warning.
  /// Use this instead of leaving onclick/onchange closures empty.
  ///
  /// # Example
  /// ```
  /// onclick: move |_| not_implemented!("Push notification settings"),
  /// ```
  #[macro_export]
  macro_rules! not_implemented {
      ($desc:expr) => {{
          tracing::warn!("not_implemented: {}", $desc);
          // Dispatch toast via AppState if available
          if let Ok(mut state) = dioxus::prelude::try_use_context::<
              dioxus::prelude::Signal<$crate::state::AppState>
          >() {
              state.write().push_toast(
                  $crate::state::Toast::warning(
                      format!("⚠ Not yet implemented: {}", $desc)
                  )
              );
          }
      }};
  }
  ```
  > Note: the toast dispatch uses `try_use_context` so the macro is safe outside
  > component scope (e.g. in a `spawn` future). It degrades to `tracing::warn!` only.

- [ ] **A.2** Add `ui_noop!()` macro — literally `()`. Exists solely as a scanner-recognizable
  explicit opt-out:
  ```rust
  #[macro_export]
  macro_rules! ui_noop {
      () => { () };
  }
  ```

- [ ] **A.3** Add trybuild compile-fail test fixture for a bare empty onclick that is expected
  to fail once Rule A is enforced (gated behind the `strict-actions` feature so it only
  fails when the lint-gate feature is enabled, not on every `cargo test`).

### Phase B — Lint-gate scanner `ui_action_coverage.rs`

- [ ] **B.1** Create `crates/lint-gate/build/ui_action_coverage.rs`:
  - `scan_empty_handlers(src: &str) -> Vec<Violation>` — regex scan for bare empty event handler patterns
  - `scan_empty_views(src: &str) -> Vec<Violation>` — detect `#[component]` + `rsx! {}` stubs
  - `count_not_implemented(src: &str) -> usize` — count `not_implemented!(` occurrences
  - `count_ui_noop(src: &str) -> usize` — count `ui_noop!(` occurrences

- [ ] **B.2** Wire into `crates/lint-gate/build.rs`:
  ```rust
  mod ui_action_coverage;
  // In the main scan loop:
  let action_violations = ui_action_coverage::scan_file(&src, &path);
  // Apply baseline grandfathering (same pattern as context_menu_coverage)
  // Emit cargo::error for non-baseline violations
  // Emit cargo::warning for not_implemented count
  ```

- [ ] **B.3** Tests in `crates/lint-gate/src/lib.rs` (same location as scanner_tests):
  - `empty_onclick_is_violation` — scanner catches `onclick: move |_| {}`
  - `not_implemented_is_ok` — `onclick: move |_| not_implemented!("X")` is not flagged
  - `ui_noop_is_ok` — `onclick: move |_| ui_noop!()` is not flagged
  - `nonempty_onclick_is_ok` — multi-line real handler is not flagged
  - `empty_rsx_body_is_violation` — bare `rsx! {}` component body is flagged
  - `rsx_with_content_is_ok` — component with real content is not flagged

### Phase C — Baseline grandfathering

- [ ] **C.1** Run the scanner with `REGEN_BASELINE=1` (same mechanism as context menu plan)
  to produce the initial `crates/lint-gate/build/ui_action_baseline.toml`. This grandfathers
  all existing violations so `cargo check` passes immediately.

- [ ] **C.2** Verify `cargo check --workspace` passes with zero new errors after the scanner
  lands. The only output should be `cargo::warning=poly-action-coverage: N not_implemented!() calls remain`.

- [ ] **C.3** From this point on, **every new empty handler added to any file is a `cargo::error`**
  — the baseline only covers lines that existed when the scanner was seeded.

### Phase D — Fix existing violations (work through the baseline)

Work through `ui_action_baseline.toml` top-to-bottom. For each violation, choose one:

- **Implement it** — add real code; remove the baseline entry.
- **`not_implemented!("desc")`** — replace the empty handler; remove the baseline entry.
  The toast tells the user what's missing and why. The scanner warning tracks the debt.
- **`ui_noop!()`** — if the element genuinely should do nothing (decorative, display-only),
  add the opt-out; remove the baseline entry.

Priority order for D:

- [ ] **D.1** Notification settings submenu (the original bug report) — all settings nav
  items that navigate to empty panels.
- [ ] **D.2** Server settings submenu items (Privacy Settings, Audit Log, etc.).
- [ ] **D.3** Voice/Video call toolbar buttons (mute, cam, share screen).
- [ ] **D.4** Account bar action buttons (set status, set avatar).
- [ ] **D.5** All remaining baseline entries — sweep through the TOML and eliminate it.

---

## Relationship to Existing Plans

| Plan | Overlap | This plan adds |
|---|---|---|
| `plan-context-menu-quality-control` | ✅ DONE — every component declares a menu policy | Buttons/onclick handlers *within* those components also have a declared policy |
| `plan-connected-routes-static-check` | ✅ DONE — every `Link` target is type-safe | Route *target components* that render empty bodies are now flagged |
| `plan-component-lints` | ✅ DONE — 150-line component rule | N/A |

The three done plans ensure correct *structure*. This plan ensures correct *behavior*:
every element that looks interactive actually does something, or loudly says why it doesn't.

---

## Design Decisions & Trade-offs

**Why `not_implemented!` instead of `todo!()` / `unimplemented!()`?**
`todo!()` panics at runtime — unacceptable in a shipped UI. `not_implemented!` degrades
gracefully to a toast and a log line. It communicates clearly to testers: "this is WIP,
not a crash."

**Why `ui_noop!()` over just leaving the handler empty?**
An empty handler and a `ui_noop!()` handler are semantically different: one is a bug,
one is a decision. The macro makes the decision explicit and scanner-visible.

**Why scanner over a type-wrapper like `Action<T>`?**
A `UiAction` wrapper type would require touching every `onclick` in the codebase (350+
occurrences) at once. The scanner approach lets us grandfather existing code and tighten
the net incrementally — the same strategy that made the context-menu plan shippable in
one session.

**Why `cargo::warning` for `not_implemented!` count, not `cargo::error`?**
The count is a *progress metric*, not a blocker. Shipping the feature means the count
hits zero over time. Blocking builds on any `not_implemented!` call would prevent
iterative development. The strict-actions feature flag provides an opt-in zero-tolerance
mode for release branches.

---

## Acceptance Criteria

- [ ] `cargo check --workspace` passes with zero errors
- [ ] Any new `onclick: move |_| {}` added to any file causes `cargo check` to fail with a clear error
- [ ] Any new `#[component]` with `rsx! {}` body causes `cargo check` to fail
- [ ] The `not_implemented!` count `cargo::warning` is visible on every build
- [ ] Clicking any previously-silent button now shows a "Not yet implemented: X" toast
- [ ] `cargo test -p poly-lint-gate` passes all scanner unit tests
- [ ] `cargo test -p poly-ui-macros` passes all trybuild compile-fail fixtures
