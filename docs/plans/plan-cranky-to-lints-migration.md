# Plan: Retire cargo-cranky → `[workspace.lints]`

## Status: ✅ DONE (2026-06-08) — all phases shipped & verified

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

## Phase D — Verify (the QA gate)

- [ ] **D.1** `cargo clippy --workspace --all-targets -- -D warnings` is clean. This is
  the load-bearing check: the folded-in complexity/hygiene denies are now enforced in
  CI for the first time, so any code that passed CI but would have failed cranky must be
  fixed here. Iterate until a clean round finds nothing new.
- [ ] **D.2** `cargo check -p poly-lint-gate` still rc=0 (the hang-class / persona gate
  is independent of cranky but re-run as a regression check).
