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

## Phase A — Audit (shipped in change slqszoutprvy)
- [x] **A.1** `cargo upgrade --dry-run --incompatible` → capture every dep with a newer (incl. major) version available
- [x] **A.2** Note high-risk majors (dioxus, axum, tokio, reqwest, wit-bindgen, serde, sqlx/rusqlite) — these may need code changes
- [x] **A.3** Record the audit table in this plan before changing anything

### Audit Table (before upgrades)

| crate | old | new | type | notes |
|---|---|---|---|---|
| dioxus | 0.7.3 | 0.7.9 | compatible | patch only |
| dioxus-cli-config | 0.7.3 | 0.7.9 | compatible | patch only |
| surrealdb | 3.0.1 | 3.1.2 | compatible | minor bump |
| ed25519-dalek | 2.1 | 2.2 | compatible | |
| reqwest | 0.13.2 | 0.13.4 | compatible | patch |
| webbrowser | 1.0 | 1.2.1 | compatible | minor |
| tao | 0.34 | 0.35.3 | compatible | minor |
| sha2 | 0.10 | 0.11 | MAJOR | crypto, may affect API |
| gloo-net | 0.6 | 0.7 | MAJOR | WASM net |
| aes | 0.8 | 0.9 | MAJOR | crypto |
| cbc | 0.1 | 0.2 | MAJOR | crypto |
| hmac | 0.12 | 0.13 | MAJOR | crypto |
| getrandom | 0.3 | 0.4 | MAJOR | WASM random |
| tower | 0.4 | 0.5 | MAJOR | middleware |
| jsonwebtoken | 9 | 10 | MAJOR | JWT |
| tokio-tungstenite | 0.26 | 0.29 | MAJOR | websocket |
| cpal | 0.16 | 0.17 | MAJOR | audio |
| wasmtime | 42 | 45 | MAJOR | wasm runtime |
| wit-bindgen | 0.53 | 0.57 | MAJOR | wasm bindgen |
| cairo-rs | 0.18 | 0.22 | MAJOR | graphics |
| wry | 0.53 | 0.55 | MAJOR | webview |
| scraper | 0.20 | 0.27 | MAJOR | HTML parsing (poly-reddit only) |
| gloo-timers | 0.3 | 0.4 | MAJOR | WASM timers (poly-core, poly-discord) |

**Not outdated:** axum (0.8 = latest), tokio (1 = latest), serde (1 = latest), rusqlite/sqlx (not in workspace deps directly)

## Phase B — Compatible upgrades (shipped in git commit 4a34090f on branch worktree-agent-a798d20b0f07a332e)
- [x] **B.1** `cargo upgrade` (semver-compatible only, no --incompatible) → bumps within current major
- [x] **B.2** `cargo build --workspace` — clean (also fixed pre-existing discord gateway feature build failure)
- [x] **B.3** commit "chore(deps): semver-compatible upgrades"

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
