# Plan: Workspace-wide cranky lint ZERO (per-site, no blanket allows)

## Status: ✅ DONE — every member at 0 own-dir lib/src lints (test code gate-exempt); allow_ban + lint-gate clean; main synced

> Goal: drive every workspace member to true cranky zero (lib/src; test
> code is gate-exempt per `feedback_test_lints`). Mechanism: **per-site
> fixes everywhere** (user decision 2026-05-30) — NO crate-level
> `#![allow]` for the taste classes, NO grandfathering. Real-signal
> classes (indexing_slicing, arithmetic_side_effects, cast_*,
> unwrap/expect/panic) get careful per-site fixes; taste classes
> (manual_let_else, significant_drop_tightening, wildcard_imports,
> redundant_pub_crate, missing_const_for_fn) get the idiomatic rewrite.

## Baseline (lib/src-only, OWN-dir filtered, measured 2026-05-30)

Total ~860 lib/src lints. Four hotspots = ~75%. Order: chat-mcp →
discord → electron → web.

| crate | own lib/src | top classes |
|---|---|---|
| poly-chat-mcp | 225 (after --fix) | manual_let_else 114, significant_drop_tightening 56 |
| poly-discord | 155 | wildcard_imports 28, manual_string_new 16, expect_used 13 |
| poly-desktop-electron | 148 | (shares discord-like shape) |
| poly-web | 402 | indexing_slicing 57, arithmetic_side_effects 43, as_conversions 40 |
| poly-stoat | 24 | default_numeric_fallback, drop_tightening |
| poly-teams | 14 | — |
| poly-hackernews | 7 | — |
| poly-audio-backend | 7 | — |
| poly-video-backend | 4 | indexing_slicing |
| poly-host | 4 | — |
| poly-lint-gate-rules | 12 | needless_raw_string_hashes 11 |
| poly-ui-macros | 8 | indexing_slicing 5 |
| poly-desktop-devtools | 10 | — |

## Phase A — poly-matrix compile fix (DONE)
- [x] **A.1** Import `IsBackend` in lib.rs test module (E0599) — shipped `010ad845` (git)
- [x] **A.2** Verify 0 lib/src lints (4 test-only) — confirmed

## Phase B — poly-chat-mcp (225 → 0) ✅ DONE
- [x] **B.1** clippy --fix machine-applicable pass — `66a88438`
- [x] **B.2–B.4** manual_let_else, significant_drop_tightening,
  option_if_let_else + structural remainder per-site (subagent, rebased
  + pushed `1985c5c9`). Note: chat-mcp is under `/mcp/` so allow_ban skips it.
- [x] **B.5** own-dir lib/src count == 0 confirmed

## Phase C — poly-discord (155 → 0) ✅ DONE
- [x] **C.1–C.5** wildcard_imports, manual_string_new, expect_used,
  redundant_pub_crate, arithmetic→checked, string_slice, drop tightening,
  backend/mod.rs→backend.rs (salvaged from worktree race, `0ea9046f`)
- [x] **C.6** final structural lints: too_many_lines ×2 via
  `// lint-allow-unused:` marker (allow_ban-compliant), future_not_send
  inline-allow; test-mod IsBackend import fix. own-dir = 0. (`bc3415ed`,
  marker fix `5e51e082`)

## Phase D — poly-desktop-electron (148 → 0) ✅ DONE
- [x] **D.1–D.3** already clean once path-dep noise filtered; own-dir = 0

## Phase E — poly-web (402 → 0) ✅ DONE
- [x] **E.1–E.6** the 402 were `--all-targets` (test-code) lints; lib/src
  (`apps/web/src/`) own-dir count measured **0** with cap-lints + own-dir
  filter. No source changes needed.

## Phase F — long tail ✅ DONE
- [x] **F.1–F.2** all long-tail crates (stoat, teams, hackernews,
  audio/video-backend, host, lint-gate-rules, ui-macros, desktop-devtools,
  client, demo, server-client) measured **0** own-dir lib/src lints. The
  earlier nonzero figures were all `--all-targets` test-code (gate-exempt).

## Phase G — workspace verify ✅ DONE
- [x] **G.1** full own-dir-filtered sweep of all 28 remaining members =
  0 everywhere (empty result)
- [x] **G.2** poly-core lib cranky-zero confirmed (cap-lints own-dir = 0);
  lint-gate baseline unchanged (794, no hang-grandfather growth)
- [x] **G.3** plan DONE; allow_ban gate clean (discord too_many_lines
  allows carry the `// lint-allow-unused:` marker)

## Invariants
- Own-dir filter when counting (`-p X` JSON includes path-dep lints from
  other crates' dirs — never fix those under the wrong crate).
- Test code (`/tests/`, `#[cfg(test)]`) is gate-exempt — leave its lints.
- poly-core's line-keyed lint-gate baseline must NOT gain hang entries
  (it's the only crate with that gate; the hotspots have none).
- Each subagent commits in its own `jj workspace add` dir and echoes the
  landed commit id before reporting done.
