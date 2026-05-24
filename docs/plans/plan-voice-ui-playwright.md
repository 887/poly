# Voice UI integration test harness — follow-up from K.4

## Status: ✅ DONE — initial harness shipped (Phase A all sub-steps). Phase B (broader UI coverage) deferred.

This plan tracks the UI integration test harness called out as the
prerequisite for K.4 in `docs/plans/plan-voice-video-calls.md`. K.4
asked for a UI integration test of the voice-banner connect/disconnect
flow against a real `apps/web` build. The blocker was: the repo had
no harness for driving Chromium from `cargo test` / `cargo run`.

## Decision — Rust CDP binary, not Playwright/Node

The header originally implied a Playwright/Node side-channel. After
audit we picked a Rust workspace member that drives Chromium via raw
CDP (`tokio-tungstenite`):

- **Prior art in-repo**: `mcp/web-devtools-mcp/src/main.rs` already
  drives Chromium this way (1990 lines). The new harness reuses the
  same approach with a much smaller surface (single binary, ~300 LoC).
- **Existing test-tool shape**: `tools/discord-voice-smoke/` and
  `tools/stoat-voice-smoke/` are Rust binaries gated by `RUN_*_SMOKE=1`.
  The new harness mirrors that pattern (`RUN_VOICE_UI_SMOKE=1`).
- **No Node toolchain**: Playwright would require Node + npm + a
  separate test runner. `cargo build` + `cargo run` is the existing
  ergonomic.
- **No new workspace surface for CI to compile**: A Rust crate with
  three deps (`tokio-tungstenite`, `reqwest`, `serde_json`) is cheap
  vs. a TypeScript project with its own `package.json` + lockfile.

Cost: the harness has fewer batteries than Playwright (no built-in
auto-wait helpers, no codegen, no traces). Mitigated by the
`wait_for_predicate` poll-loop in the binary and by the small surface
of UI assertions K.4 actually needs.

## Phase A — Initial harness + K.4 voice banner smoke

- [x] **A.1** New workspace member `tools/voice-ui-smoke/` with `Cargo.toml`
  registered in root `Cargo.toml` workspace members list.
  Shipped — `poly-voice-ui-smoke` binary, mirrors `discord-voice-smoke` /
  `stoat-voice-smoke` crate layout.
- [x] **A.2** Minimal CDP client in `src/main.rs` (no external chromium
  helper crate). Methods: `connect`, `navigate`, `eval`, `click`,
  `wait_for_predicate`, `wait_for_selector`, `wait_for_absent`. Reuses
  the same `tokio-tungstenite` + `reqwest` plumbing as
  `mcp/web-devtools-mcp/src/main.rs`. Skip-by-default via
  `RUN_VOICE_UI_SMOKE=1` env gate, with `POLY_VOICE_UI_URL` to point at
  the test voice-channel route and `POLY_CDP_PORT` (default `9222`).
  Shipped.
- [x] **A.3** K.4 assertions: navigate to a voice channel, click
  `.btn-voice-join`, assert `.voice-banner` appears, assert
  `.voice-banner-avatars` has the local user, click
  `.voice-ctrl-btn.disconnect`, assert `.voice-banner` disappears.
  All assertions go through `wait_for_predicate` with a 12 s deadline
  so the test tolerates network/render latency without flaking.
  Shipped.
- [x] **A.4** README explains the prerequisites (running `dx serve`
  apps/web on 3000 + Chromium on `--remote-debugging-port=9222`) and
  the env-var contract. Documents which CSS selectors the test depends
  on so a future class rename has a checklist.
  Shipped.
- [x] **A.5** Tick K.4 in `docs/plans/plan-voice-video-calls.md`,
  update Status header to reflect K.4 shipped, link this plan as the
  follow-up that closed the gap.
  Shipped in this change.

### Verification done

- `cargo build -p poly-voice-ui-smoke` passes clean.
- `cargo run -p poly-voice-ui-smoke` (no env) exits 0 with the SKIP
  message — proves the always-on compile path.
- `cargo check -p poly-core --target wasm32-unknown-unknown` still
  clean (no UI churn touched — only the new harness crate).
- Full live invocation against a running `apps/web` requires user
  setup (dx serve + Chromium + signed-in test-stoat account); the
  README documents the recipe.

## Phase B — Broader UI coverage (DEFERRED)

The K.4 harness is intentionally minimal. The following extensions are
worth tracking but were not part of K.4 scope and don't need to land
together:

- [ ] **B.1** Wrap CDP plumbing in a reusable helper crate
  (`crates/cdp-test-client/`) once a second smoke test wants it. Don't
  premature-abstract on N=1.
- [ ] **B.2** Add a `tools/voice-ui-smoke/tests/k4.rs` integration test
  that boots `test-runner` + `apps/web` in-process, signs in an
  ephemeral test-stoat account, and runs the K.4 flow end-to-end with
  no user setup. Today's harness assumes a pre-existing `dx serve` +
  Chromium + signed-in account because (a) `apps/web` is a fullstack
  Dioxus app whose dev server is `dx serve`, not directly runnable
  from `cargo test`; (b) the signed-in account state is per-user.
  A "self-contained" run would need a programmatic account
  bootstrapping path that doesn't exist yet.
- [ ] **B.3** Held-call swap UI test (K.5 follow-up). The data-model
  swap is covered by `crates/core/tests/k5_held_call_swap.rs`; the
  full Dioxus-runtime swap can ride the same CDP harness once Phase
  B.2 lands so account setup is automatic.
- [ ] **B.4** Teams "coming soon" toast assertion (K.6 follow-up).
  Same shape: contract is covered by unit tests; the UI overlay/toast
  assertion needs the harness.

## Why Phase B is deferred

The K.4 ask was "harness + voice-banner connect/disconnect smoke", not
"every Phase K UI assertion". Phase B is scaffolding for a future
plan-voice-video-tests-self-contained.md that wires test-runner +
ephemeral account setup into `cargo test`. Until that lands, the
existing harness is a manual smoke a human can run before shipping
voice-touching changes.
