# Plan — Read-Guard Scoping (Class #2 Hang Prevention)

> Status: **✅ DONE** — Phases 1+2+5 shipped (`5c1e13c7`). `BatchedSignal::with(|v|)` documented as preferred read API; audit found zero live HIGH incidents (BatchedSignal Phases 2-3 disciplined the codebase); `forbid-long-read-guard.sh` lint ships as the regression gate. Canonical patterns at `docs/dev/reactive-state.md`.
> Authors: orchestrator (audit at [`/tmp/poly-hang-class-2-audit.md`](file:///tmp/poly-hang-class-2-audit.md)).
> Last updated: 2026-04-25.

---

## 1. Why this plan exists

CLAUDE.md "Common WASM-hang causes" #2:

> **Live `Signal::read()` guard across a `.write()` of the same signal.** WASM panics → no panic_hook unwinding → tight loop / unreachable. Wrap reads in tightly-scoped `{ … }` so the guard drops before any write.

Sister plans closed adjacent classes:

- Class #1 (multi-cascade) → `plan-batched-signal.md` + `tools/scripts/forbid-signal-write.sh`.
- Class #3 (use_effect cycle) → `plan-use-spawn-once.md` + `tools/scripts/forbid-use-effect-spawn-cycle.sh`.
- Class #4 (RwLock starvation) → `read_with_timeout` + `tools/scripts/forbid-raw-backend-read.sh`.

Class #2 is the only one without a type-system or lint backstop. The audit at [`/tmp/poly-hang-class-2-audit.md`](file:///tmp/poly-hang-class-2-audit.md) found:

| Severity | Count |
|----------|------:|
| HIGH | **0** |
| MEDIUM | 3 |
| LOW | 97+ |

**Zero live HIGH-severity incidents** — the BatchedSignal Phase-2/3 migration cleaned up the codebase well, with authors using explicit `{ … }` block-scoping or `drop(guard)` where needed. There is even an explicit comment in `favorites_sidebar.rs:673-675` warning future contributors about the pattern.

That said — per standing user directive ("do not defer; ship preventively") — and because the three MEDIUM sites are the kind of shape that *looks fine* until a future refactor flips them to HIGH, this plan ships a lint + an opt-in closure-scoped read API so the next regression is a CI failure, not a tab freeze.

---

## 2. Solution summary

Recommended path: **(c) Combination — formalise `BatchedSignal::with(|v| ...)` as the preferred read API AND ship a regex CI lint that flags long-scoped raw `.read()` bindings.**

Two layers, ordered cheapest-first:

1. **Lint (Track A)** — `tools/scripts/forbid-long-read-guard.sh` (regex).
   - Flags any `let <var> = <sig>.read();` where `<var>` is referenced more than `N=4` lines later, OR where a `<sig>.batch(`, `<sig>.pending_update(`, or `<sig>.write(` call appears in the same scope after the `let`.
   - Allowlist file `tools/scripts/long-read-guard-allowlist.txt` for intentional cases (the explicit-block pattern, `drop(g)` pattern, etc.).
   - Wired into `lint-test.yml`.

2. **Closure-read formalisation (Track B)** — already-shipped `BatchedSignal::with(|v| ...) -> R` becomes the **documented preferred read API** for any read where the value is used more than once in the same scope.
   - The deprecation question: `Signal::read()` access via `Deref` is **load-bearing for `rsx!` format strings** (`"{chat_data.read().loading}"`) and for inline single-statement reads. Deprecating it would break ~150 sites. **Do not deprecate.** Instead: ship `BatchedSignal::with` as a *style preference* in the dev docs, and rely on the lint above to catch the dangerous pattern.

The combination buys defense-in-depth: the lint catches the regression at PR time; `with(|v| ...)` gives reviewers a clean rewrite target when it fires.

---

## 3. Phases

Each phase lands independently. Phase N blocks on phase N−1.

### Phase 1 — Formalise `BatchedSignal::with` as the preferred read closure API — ✅ DONE (`5c1e13c7`)

**Deliverable:** docs change + canonical example, no behaviour change.

Tasks:
- [x] Add a §3 to `docs/dev/reactive-state.md` (created in `plan-batched-signal.md` Phase 6) titled "Read-guard scoping" covering the four safe shapes:
  - Inline `sig.read().field.clone()` (single-statement temporary).
  - `sig.with(|v| { … })` (closure-scoped multi-field read).
  - Explicit `let var = { let g = sig.read(); … (g, …) };` block.
  - Explicit `drop(g)` before subsequent write (only for cross-block patterns where the block form is awkward).
- [x] Add a "do not" example showing the panic shape: `let g = chat_data.read(); … chat_data.batch(|cd| ...);`.
- [x] Add doc-link from `BatchedSignal::with` rustdoc into the new section.
- [x] Verify all current `.with` / `.map` examples still compile (they do — these are existing methods).

Verification: `cargo doc --no-deps -p poly-core` clean.

### Phase 2 — Migrate the three MEDIUM sites — ✅ DONE (`5c1e13c7`)

**Deliverable:** the three MEDIUM call sites identified in the audit each move to a no-guard-across-helper-call shape.

Tasks:
- [x] `crates/core/src/ui/mod.rs:1254-1272` — collapse the `let cm = client_manager.read();` + nested `app_state.read()` into either two explicit blocks or one `client_manager.with(|cm| { … app_state.peek()… })` shape. Justify with a comment why the explicit `drop(cm)` was necessary.
- [x] `crates/core/src/ui/account/common/account_bar.rs:371` — change the `current_account_bar_user(&app_state.read(), &chat_data.read())` call to bind via `with`:
  ```rust
  let user = chat_data.with(|cd| {
      app_state.with(|st| current_account_bar_user(st, cd))
  });
  ```
  Or rewrite the helper to take `BatchedSignal<…>` handles and read internally — preferred if the helper is the only caller.
- [x] `crates/core/src/ui/electron_titlebar.rs:132` — same treatment as account_bar.

Verification: `cargo check --workspace --target wasm32-unknown-unknown`, smoke-test electron titlebar (Phase-2 desktop build) and the AccountBar render path on each backend.

### Phase 3 — Ship the lint script — ✅ DONE (`5c1e13c7`)

**Deliverable:** `tools/scripts/forbid-long-read-guard.sh` + allowlist + CI wiring.

Tasks:
- [x] Create `tools/scripts/forbid-long-read-guard.sh`. Algorithm:
  - Walk `crates/core/src/ui/**/*.rs`.
  - For each `let <var> = <sig>.read();` (or `.peek();`) line:
    - Find the closing `}` of the enclosing block (cheap heuristic: count braces forward).
    - Inside that range, look for `<sig>.batch(`, `<sig>.pending_update(`, or `<sig>.write(`.
    - If found AND the `let` is NOT followed within 1-2 lines by a `}` (the explicit-block pattern), AND no `drop(<var>);` appears between the `let` and the write call, fail.
  - Print file:line for every fail.
- [x] Create `tools/scripts/long-read-guard-allowlist.txt` seeded with the LOW sites that the regex misclassifies (mostly inline `.read().field.clone()` patterns where the regex sees the `.read();` token but no binding actually escapes).
- [x] Wire into `.github/workflows/lint-test.yml` alongside the existing three lint scripts.
- [x] Document in `tools/scripts/README.md` (or wherever the existing lint scripts are documented).

Verification:
- Manually inject a HIGH-severity regression: edit a sample file to introduce `let g = chat_data.read(); chat_data.batch(|cd| { cd.loading = false; });` and confirm the script fails.
- Run against current main; confirm only the 3 MEDIUM sites flag (and that they're allowlisted post-Phase-2).

### Phase 4 — (Optional) Dylint upgrade path — ⏸ SKIPPED (intentional)

Same pattern as `plan-batched-signal.md` Phase 5b — re-implement the regex check as a `cargo dylint` HIR-aware lint so it can distinguish `Signal::read` from `RwLock::read` / `std::io::Read::read`. Deferred until the regex script proves insufficient (i.e. emits false positives that the allowlist can't keep up with). As of 2026-05-02 the regex script is keeping pace with no allowlist churn, so this phase remains skipped.

### Phase 5 — Documentation cleanup — ✅ DONE (`5c1e13c7`)

- [x] Update `CLAUDE.md` "Common WASM-hang causes" #2 to point at this plan + the lint as the prevention.
- [x] Add a row to the "lint scripts" table in `docs/dev/reactive-state.md` (or wherever the existing scripts are catalogued).

---

## 4. Verification

After each phase:
- `cargo check --workspace --target wasm32-unknown-unknown` clean.
- `cargo check --workspace` clean.
- `cargo test --workspace` green.

After Phase 3:
- The lint script must pass on `main` (post-Phase-2 migrations).
- Inject one synthetic HIGH-severity site in a test file → confirm CI fails.
- Remove the synthetic site → confirm CI passes again.

After Phase 5:
- Manual smoke: AccountBar renders correctly across all six backends, electron titlebar shows the correct title for DMs / Server / Settings views.

---

## 5. Risks / failure modes

1. **Deprecating `Signal::read` is prohibitively disruptive.** ~150 `rsx!` format-string and inline-temporary call sites would break (`"{chat_data.read().loading}"`). **Mitigation:** do not deprecate. The lint catches the dangerous pattern; the inline / format-string patterns are short-lived and safe by construction.
2. **Regex script false positives.** The script can't tell `.read().field.clone();` (safe) from `let g = sig.read();` (potentially unsafe). The first-pass heuristic is "look for `let <var> = <sig>.read();` as a whole-line statement"; this naturally skips inline temporaries. The allowlist absorbs the rest.
3. **Regex script false negatives.** The script won't catch:
   - `let g = sig.read(); helper_that_internally_writes_sig(…);` — needs whole-program flow analysis (Phase 4 dylint upgrade).
   - Cross-signal aliasing where two distinct `Signal<T>` handles wrap the same storage. Vanishingly rare in practice; not worth chasing.
4. **The three MEDIUM sites' fixes add a render-time clone.** Negligible (these structs are small) but worth flagging.
5. **Author drift.** A new contributor could still write the panic pattern in a fresh file. The lint runs on every PR, so the regression is caught at CI rather than at runtime.

---

## 6. Timeline estimate

| Phase | Budget | Agent tier |
|-------|--------|-----------|
| 1 — docs formalisation | 0.2 session | sonnet-coding |
| 2 — migrate 3 MEDIUM sites | 0.3 session | sonnet-coding |
| 3 — regex lint + CI wiring | 0.4 session | sonnet-coding |
| 4 — (optional) dylint | 1-2 sessions | opus-coding (deferred) |
| 5 — docs cleanup | 0.1 session | sonnet-coding |

Total to ship phases 1-3 + 5: **~1 focused session** (~2-3h). Phase 4 is parked behind concrete need.

---

## 7. Reference artifacts

- [`/tmp/poly-hang-class-2-audit.md`](file:///tmp/poly-hang-class-2-audit.md) — full site-by-site classification, 0 HIGH / 3 MEDIUM / 97+ LOW.
- [`docs/plans/plan-batched-signal.md`](file:./plan-batched-signal.md) — sister plan, closed class #1; same shape and lint pipeline.
- [`docs/plans/plan-use-spawn-once.md`](file:./plan-use-spawn-once.md) — sister plan, closed class #3.
- `crates/core/src/state/batched_signal.rs` — already exposes `.with(|v| ...)` and `.map(|v| ...)` — the closure-read primitive this plan formalises.
- `tools/scripts/forbid-signal-write.sh` — canonical regex-lint precedent.
- `CLAUDE.md` § "Common WASM-hang causes" #2 — the hazard being eliminated.

---

## 8. Out of scope

- Refactoring `ChatData` / `AppState` field shapes. Orthogonal to this plan.
- A full HIR-aware Rust lint (Phase 4) — deferred until the regex script proves insufficient.
- Deprecating `Signal::read` / `Signal::peek` Deref access. Too disruptive; the rsx! format-string idiom and inline `.read().field.clone()` pattern depend on it. Out of scope by design.
- Cross-signal aliasing detection (two `Signal<T>` handles backing the same arena slot). Vanishingly rare; would need full dataflow analysis.
- Replacing or removing the existing `drop(guard)` / explicit-block patterns. They are safe and clear; the plan formalises `with(|v| ...)` as preferred only for *new* code.
