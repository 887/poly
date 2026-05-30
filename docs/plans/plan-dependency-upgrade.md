## Status: ✅ DONE — all phases shipped

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

## Phase C — Incompatible (major) upgrades (shipped in git commits bf8e0bd9 + followup on branch worktree-agent-a798d20b0f07a332e)
- [x] **C.1** Apply major bumps in small related clusters: sha2/aes/cbc/hmac, tower, jsonwebtoken, tokio-tungstenite, cpal, wasmtime/wit-bindgen, getrandom, scraper — all applied. See table below.
- [x] **C.2** After each cluster: build, fix breakage, re-build — all clean
- [x] **C.3** Skip/pin any dep whose major jump needs a rewrite too large to be "reasonable":
  - **gloo-net 0.6 (SKIPPED)** — 0.7 requires `js-sys ^0.4` which does not exist yet (latest is 0.3.99). Ecosystem-blocked.
  - **gloo-timers 0.3 (SKIPPED)** — 0.4 requires `js-sys ^0.4`, same issue.
  - **wry 0.53 (SKIPPED)** — 0.55 conflicts with `dioxus-desktop 0.7.9` which transitively pins `wry ^0.53.5`. Must wait for dioxus to release with wry 0.55+ support.
  - **tao 0.34 (SKIPPED)** — same root cause as wry (tao and wry version together in dioxus-desktop).
  - **cairo-rs 0.18 (SKIPPED)** — 0.22 type-incompatible with `webkit2gtk 2.0.1` (which is pinned by dioxus-desktop → wry 0.53). The `cairo::Surface` type in webkit2gtk's API refers to cairo-rs 0.18 type; upgrading cairo-rs breaks the type constraint in `.snapshot()` callback.
- [x] **C.4** rust-version unchanged (1.85 — no dep bumped MSRV above that)
- [x] **C.5** commits: bf8e0bd9 + getrandom04-wasm alias removal commit

### Code changes made for major upgrades
- **getrandom 0.3→0.4**: removed `getrandom04-wasm` alias (now redundant — getrandom IS 0.4); updated 4 crates (crates/core, apps/web, apps/desktop, apps/desktop-electron) to use `getrandom = { workspace = true }` directly instead.
- All other majors: no source code changes needed (API-compatible or unused in source).

## Phase D — Verify (all passed)
- [x] **D.1** `cargo build --workspace` clean — Finished in ~2m
- [x] **D.2** wasm build (`cargo build --target wasm32-unknown-unknown -p poly-web --no-default-features --features web`) — Finished in ~1m
- [x] **D.3** `cargo test --workspace --no-run` compiles — Finished in ~3m
- [x] **D.4** lint-gate still builds (`cargo build -p poly-lint-gate`) — Finished in <5s
- [x] **D.5** final version table recorded below

### Final version table (before → after)

| crate | before | after | type |
|---|---|---|---|
| dioxus | 0.7.3 | 0.7.9 | compatible |
| dioxus-cli-config | 0.7.3 | 0.7.9 | compatible |
| surrealdb | 3.0.1 | 3.1.2 | compatible |
| ed25519-dalek | 2.1 | 2.2 | compatible |
| reqwest | 0.13.2 | 0.13.4 | compatible |
| webbrowser | 1.0 | 1.2 | compatible |
| sha2 | 0.10 | 0.11 | major |
| aes | 0.8 | 0.9 | major |
| cbc | 0.1 | 0.2 | major |
| hmac | 0.12 | 0.13 | major |
| tower | 0.4 | 0.5 | major |
| jsonwebtoken | 9 | 10 | major |
| tokio-tungstenite | 0.26 | 0.29 | major |
| cpal | 0.16 | 0.17 | major |
| wasmtime | 42 | 45 | major |
| wasmtime-wasi | 42 | 45 | major |
| wit-bindgen | 0.53 | 0.57 | major |
| getrandom | 0.3 | 0.4 | major |
| scraper (reddit) | 0.20 | 0.27 | major |
| gloo-net | 0.6 | **0.6 (skipped)** | js-sys ecosystem blocked |
| gloo-timers | 0.3 | **0.3 (skipped)** | js-sys ecosystem blocked |
| wry | 0.53 | **0.53 (skipped)** | dioxus-desktop pinned |
| tao | 0.34 | **0.34 (skipped)** | dioxus-desktop pinned |
| cairo-rs | 0.18 | **0.18 (skipped)** | webkit2gtk type incompatibility |

## Constraints
- Do NOT touch the lint-gate baseline.json.
- Do NOT downgrade rust-version below what deps require.
- If dioxus 0.7.x → 0.8+ exists and the jump is large, flag it for orchestrator
  decision rather than forcing it (dioxus drives the whole UI; a major break is
  a big surface). Note it; don't silently skip.
- Keep `Cargo.lock` updated and committed.
