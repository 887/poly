# Plan: Retire cargo-cranky → `[workspace.lints]`

## Status: ✅ SHIPPED (2026-06-08) — Phases A–C landed; **Phase D.2 verified, D.1 RUN AND RED**

> The previous `✅ DONE — all phases shipped & verified` marker was wrong: both
> Phase D verification gates (D.1, D.2) were never ticked, and the Phase C sweep
> did in fact miss `.agent.md`, which kept three `cargo cranky` invocations until
> the 2026-07-28 docs-drift pass (C.4 below). Do not re-mark this plan DONE until
> D.1 and D.2 are actually run and ticked.
>
> **2026-07-28 update (plan-lint-gate-integrity Phase D):** both gates have now
> actually been run. **D.2 is green and ticked.** **D.1 is red** — 19 pre-existing
> `-D warnings` errors across three test targets that CI never reached because its
> Gate A omits `--all-targets`. The three sites and their exact fixes are recorded
> under Phase D below. This plan stays `SHIPPED`, not `DONE`.

cargo-cranky is **archived upstream** (no further maintenance). It was only ever a
config-file front-end for `cargo clippy`; the same policy is now expressed natively
in Cargo's `[lints]` table, which `cargo clippy`, `cargo build`, and rust-analyzer
all honour with **no extra tool to install**. This plan removes cranky and folds its
policy into `[workspace.lints]` with **zero loss of enforcement**.

## Background — the two configs were NOT equivalent

Before this migration, lint policy lived in **two** places that had drifted:

| Lint set | CI (`cargo clippy -- -D warnings`, via `[workspace.lints]`) | cranky (`cranky.toml`) |
|---|---|---|
| safety + curated list (unwrap/expect/panic/casts/…) | deny | deny |
| complexity trio (`too_many_lines`, `cognitive_complexity`, `too_many_arguments`) | **not enforced** | deny |
| hygiene (`todo`, `unimplemented`, `dbg_macro`) | **not enforced** | deny |
| `pedantic` + `nursery` groups | not enabled (deliberate) | warn (advisory) |

cranky was the **stricter** of the two for the agent dev-loop. A naive "delete cranky,
rely on the table" would have silently dropped the complexity + hygiene gates — so this
plan **folds the six hard denies into `[workspace.lints.clippy]`** first.

`pedantic`/`nursery` were only ever `warn` (advisory) under cranky, and the root
`Cargo.toml` carries an explicit, dated decision *not* to enable them as groups
("blanket_clippy_restriction_lints", flood of noise). That decision is honoured —
the groups are dropped, not migrated.

Every crate already had `[lints] workspace = true` (56 crates), so the table
infrastructure was fully wired; nothing new had to be opted in.

## Phase A — Fold cranky-only denies into `[workspace.lints]`

- [x] **A.1** Add `clippy::todo`, `clippy::unimplemented`, `clippy::dbg_macro` = `deny`
  to `[workspace.lints.clippy]` (Hygiene section).
- [x] **A.2** Add complexity trio `too_many_lines` / `cognitive_complexity` /
  `too_many_arguments` = `deny` (new Complexity section); thresholds stay in `clippy.toml`.
- [x] **A.3** Update the `[workspace.lints]` header comment (drop "cargo cranky", note the
  retirement + this plan).

## Phase B — Delete cranky config

- [x] **B.1** Delete root `Cranky.toml`.
- [x] **B.2** Delete all 24 per-crate `cranky.toml` files.
- [x] **B.3** Update `clippy.toml` header comment ("Pairs with … `cranky.toml`" →
  "… `[workspace.lints.clippy]`").

## Phase C — Sweep live docs (`cargo cranky` → `cargo clippy`)

Historical references in `docs/archive/**` and `docs/plans/**` are left as-is — they
record what was actually run at the time.

- [x] **C.1** Mechanical `cargo cranky` → `cargo clippy` across all live agents.md,
  README.md, the 150-line refactor checklist, and `mcp/memory-mcp/src/ops.rs` task
  templates.
- [x] **C.2** Rewrite the root `agents.md` cranky explainer block (§7 + §7a) to describe
  `[workspace.lints]` and drop "install cranky".
- [x] **C.3** Fix file-reference stragglers: `clients/server-client/agents.md` ("has its
  own cranky.toml"), `servers/backup-server/agents.md` (file-tree line), and the
  `crates/core/src/lib.rs` "make cranky quiet" comment.
- [x] **C.4** Sweep miss caught 2026-07-28: the root `.agent.md` (a live
  agent-instruction file, `applyTo: **/*.rs`) still mandated `cargo cranky` in three
  places (Section 5, the Section 14 checklist, and the Example Workflow step 6).
  Replaced with `cargo clippy --workspace -- -D warnings` + `cargo check -p poly-lint-gate`.
  C.1's "all live agents.md" scope did not match the dotfile name `.agent.md`.

## Phase D — Verify (the QA gate) — **RUN 2026-07-28; D.1 FAILS, D.2 PASSES**

- [ ] **D.1** `cargo clippy --workspace --all-targets -- -D warnings` is clean. This is
  the load-bearing check: the folded-in complexity/hygiene denies are now enforced in
  CI for the first time, so any code that passed CI but would have failed cranky must be
  fixed here. Iterate until a clean round finds nothing new.
  → **RUN and it is NOT clean — 19 errors across three test targets, all
  pre-existing and all in code the `--all-targets` flag reaches for the first
  time.** This is exactly the debt the migration was expected to surface (CI's
  Gate A omits `--all-targets`, so these never failed a build). Left unticked and
  unfixed **only** because all three sites are outside the owned-path set of the
  PR that ran this gate (plan-lint-gate-integrity Phases B–D); they are a
  self-contained follow-up, not a deferral of this phase's verification. The
  three sites and their exact fixes:
  1. `servers/server/tests/integration.rs:39` — `non_binding_let_on_must_use`:
     `let _ = tracing_subscriber::fmt()…try_init();` → name the bind,
     `let _init = …try_init();`.
  2. `crates/host-bridge/tests/video.rs` — **17 errors** in `make_bgra_frame`
     (lines 80–86) and the `min_expected` assertion (line 188):
     `as_conversions`, `arithmetic_side_effects`, `cast_possible_truncation`,
     `integer_division`. Fix with `u8::try_from(…)` / `usize::try_from(…)`,
     `saturating_mul` / `saturating_add`, and `.div_euclid(n)` in place of `/`.
  3. `apps/poly-host/src/lib.rs:1798` (inside the `#[cfg(test)]` module) —
     `.map_or(true, std::vec::Vec::is_empty)` → `.is_none_or(std::vec::Vec::is_empty)`.

  Evidence (false-zero protocol on `/tmp/d1b.log`): `Finished` = 0,
  `grep -c '^error'` = 22 (19 lint errors + 3 `could not compile`),
  `could not compile` = 3, exit 101.
- [x] **D.2** `cargo check -p poly-lint-gate` still rc=0 (the hang-class / persona gate
  is independent of cranky but re-run as a regression check).
  → **rc=0.** `Finished` present = 1, `grep -c '^error'` = 0,
  `grep -c 'could not compile'` = 0, `773 grandfathered violations`, zero new.
  Re-verified after the Phase B allowlist re-key and the content-keyed baseline.

**Status stays `SHIPPED`, not `DONE`**, per this plan's own header rule: D.1 has
now genuinely been run, and it fails. Tick D.1 and flip the header to
`✅ DONE` only once the three sites above are fixed and a clean round finds
nothing new.
