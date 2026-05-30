# Plan: Dependency upgrade sweep + edition/MSRV check

> Goal: bring all workspace deps to latest reasonable versions (major
> semver jumps OK — user explicitly approved breaking changes), confirm
> edition + toolchain are current. Done in an isolated jj workspace,
> pulled into main by the orchestrator after verification.

## Baseline (measured 2026-05-30)
- edition = "2024" (already latest stable — NO change needed)
- rust-version = "1.85"; installed toolchain rustc 1.93.1
- cargo-edit 0.12.2 present (`cargo upgrade` works)
- cargo-outdated BROKEN (libgit2.so.1.7 missing) → use `cargo upgrade --dry-run`
- 176 entries in `[workspace.dependencies]`; dioxus 0.7.3

## Phase A — Audit
- [ ] **A.1** `cargo upgrade --dry-run --incompatible` → capture every dep with a newer (incl. major) version available
- [ ] **A.2** Note high-risk majors (dioxus, axum, tokio, reqwest, wit-bindgen, serde, sqlx/rusqlite) — these may need code changes
- [ ] **A.3** Record the audit table in this plan before changing anything

## Phase B — Compatible upgrades (low risk)
- [ ] **B.1** `cargo upgrade` (semver-compatible only, no --incompatible) → bumps within current major
- [ ] **B.2** `cargo build --workspace` + `cargo build -p poly-web --target wasm32-unknown-unknown` (or the dx web build) to confirm nothing broke
- [ ] **B.3** commit "chore(deps): semver-compatible upgrades"

## Phase C — Incompatible (major) upgrades, one cluster at a time
- [ ] **C.1** Apply major bumps in small related clusters (e.g. all tokio-stack together), `cargo upgrade --incompatible -p <crate>` per cluster
- [ ] **C.2** After each cluster: build, fix breakage (API changes), re-build
- [ ] **C.3** Skip/pin any dep whose major jump needs a rewrite too large to be "reasonable" — note it here with the reason
- [ ] **C.4** Bump `rust-version` only if a dep's new MSRV requires it (keep as low as deps allow)
- [ ] **C.5** commit per cluster: "chore(deps): upgrade <cluster> to <ver> (major)"

## Phase D — Verify
- [ ] **D.1** `cargo build --workspace` clean
- [ ] **D.2** wasm build (poly-web) clean — this is the critical target
- [ ] **D.3** `cargo test --workspace --no-run` compiles (don't run, just compile)
- [ ] **D.4** lint-gate still builds (`cargo build -p poly-lint-gate`) — baseline unchanged
- [ ] **D.5** record final before/after version table; mark plan DONE

## Constraints
- Do NOT touch the lint-gate baseline.json.
- Do NOT downgrade rust-version below what deps require.
- If dioxus 0.7.x → 0.8+ exists and the jump is large, flag it for orchestrator
  decision rather than forcing it (dioxus drives the whole UI; a major break is
  a big surface). Note it; don't silently skip.
- Keep `Cargo.lock` updated and committed.
