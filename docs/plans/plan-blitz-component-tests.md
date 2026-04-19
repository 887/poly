# Plan — Dioxus Blitz Component Tests

> **Created:** 2026-04-19
> **Status:** ⏸ DEFERRED — design only, no implementation yet
> **Replaces:** F3 of `plan-client-ui-polish-followups.md` (Playwright dropped)

## Why

The polish plan called for snapshot diffs of visual-regression-prone components (Pack A toolbar, Pack D sidebar layouts, Pack G custom blocks, Pack J chat affordances). The original proposal used Playwright. **Verdict from 2026-04-19 session: Playwright is too slow for the iteration loop we want.**

Poly already ships `apps/desktop-blitz` — Dioxus 0.7 atop Blitz, a WGPU-native renderer. Blitz runs in-process, no browser, no separate runner. That's the right substrate for fast component-level snapshot tests.

## Goal

Per-component snapshot tests that:

1. Mount a single Dioxus component in an isolated Blitz scene.
2. Inject deterministic props (fixture data — no live backend, no network).
3. Render to a known viewport size at a known DPR.
4. Hash the resulting framebuffer (or save a small PNG to `tests/snapshots/<component>/<variant>.png`).
5. CI diffs the new hash against the committed baseline. Mismatch = failed test with a side-by-side artifact uploaded.

Total per-test latency target: **< 100 ms** (vs Playwright's multi-second cold-start). This is what unblocks running them on every commit.

## Open architecture questions (resolve before implementation)

1. **Test crate layout.** New `crates/component-tests` with one fixture file per component? Or co-locate next to the component (e.g. `crates/core/src/ui/account/common/account_bar.rs` + `account_bar.snap.rs`)? Co-location matches the existing `#[cfg(test)] mod tests {}` pattern but adds a dependency on Blitz for any crate with snapshot tests.
2. **Image diff vs hash diff.** Pixel-perfect (BLAKE3 of framebuffer bytes) is fast and deterministic but flaps under any GPU-driver change. Perceptual diff (e.g. `image-compare` crate) is robust but slower. Start with hash, fall back to perceptual on mismatch?
3. **Golden refresh workflow.** Same pattern as `cargo insta` — `BLITZ_UPDATE_SNAPSHOTS=1 cargo test` rewrites baselines, `git diff` shows the change. Or a dedicated `cargo xtask snap` subcommand.
4. **Fixture surface.** Each component takes typed props today. Snapshot tests need a small `fn fixture_*()` per variant (empty / loading / loaded / error / overflow / etc.) — convention to live next to the test or in a shared `fixtures` module.
5. **Coverage targets.** Start small: just the components flagged in the polish plan (Pack A toolbar, Pack D sidebar layouts, Pack G custom blocks, Pack J composer). Expand component-by-component, not all-at-once.
6. **CI integration.** Run on every PR, upload mismatch images as artifacts, fail the build. Cache baselines in the repo (small PNGs) — refuse to merge if `git status` shows changed snapshots without an explicit "snapshot update" PR label.

## Non-goals

- Cross-platform pixel parity. Blitz on Linux CI is the only canonical baseline. Local dev on macOS / Windows may diff; ignore.
- Account-credentialed flows (Stoat / Matrix / Teams / poly-server with real auth). Stay on the demo + demo_forum backends + plain components — same scope F3 originally planned.
- Animation timing — snapshot the steady state, never mid-transition.

## Estimated size

Multi-day effort:
- Day 1: pick the architecture (questions 1-3), wire one demo component (e.g. `AccountBar`) end-to-end as the proof.
- Day 2-3: enumerate the polish-plan target components, write fixtures, baseline.
- Ongoing: every new visual-regression-sensitive component gets a snap test as part of its PR.

## Status / next step

Deferred. Pick up by:
1. Spiking question 1 (test-crate layout) with a single component (`AccountBar`) to prove the integration before locking the layout decision.
2. Then enumerate polish-plan components into fixture stubs.

No urgency vs production goals — visual regressions today get caught manually via the poly-web MCP screenshot loop.
