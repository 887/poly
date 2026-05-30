# Plan: Workspace-wide cranky lint ZERO (per-site, no blanket allows)

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

## Phase B — poly-chat-mcp (225 → 0)
- [x] **B.1** clippy --fix machine-applicable pass — shipped `66a88438`
- [ ] **B.2** manual_let_else ×114 → `let …else` idiom
- [ ] **B.3** significant_drop_tightening ×56 → scope/drop guards early
- [ ] **B.4** remainder (option_if_let_else, cognitive_complexity, too_many_lines, or_fun_call, etc.) per-site
- [ ] **B.5** verify own-dir lib/src count == 0

## Phase C — poly-discord (155 → 0)
- [ ] **C.1** clippy --fix pass
- [ ] **C.2** wildcard_imports ×28 → explicit imports
- [ ] **C.3** manual_string_new ×16 → `String::new()`
- [ ] **C.4** expect_used ×13 → proper error handling / context
- [ ] **C.5** remainder per-site
- [ ] **C.6** verify own-dir count == 0

## Phase D — poly-desktop-electron (148 → 0)
- [ ] **D.1** clippy --fix pass
- [ ] **D.2** structural remainder per-site
- [ ] **D.3** verify own-dir count == 0

## Phase E — poly-web (402 → 0)  [HARDEST — real panic-class lints]
- [ ] **E.1** indexing_slicing ×57 → `.get()` / bounds-checked access
- [ ] **E.2** arithmetic_side_effects ×43 → `checked_*` / `saturating_*`
- [ ] **E.3** as_conversions ×40 + cast_* → `TryFrom` / `try_into`
- [ ] **E.4** wildcard_imports ×32 → explicit
- [ ] **E.5** remainder per-site
- [ ] **E.6** verify own-dir count == 0

## Phase F — long tail (stoat, teams, hackernews, audio/video-backend, host, lint-gate-rules, ui-macros, desktop-devtools, client, demo, server-client)
- [ ] **F.1** one commit per crate, per-site fixes
- [ ] **F.2** verify each own-dir count == 0

## Phase G — workspace verify
- [ ] **G.1** full per-crate recount (own-dir filtered) == 0 everywhere
- [ ] **G.2** poly-core lib still cranky-zero + no hang-grandfather growth
- [ ] **G.3** mark plan DONE

## Invariants
- Own-dir filter when counting (`-p X` JSON includes path-dep lints from
  other crates' dirs — never fix those under the wrong crate).
- Test code (`/tests/`, `#[cfg(test)]`) is gate-exempt — leave its lints.
- poly-core's line-keyed lint-gate baseline must NOT gain hang entries
  (it's the only crate with that gate; the hotspots have none).
- Each subagent commits in its own `jj workspace add` dir and echoes the
  landed commit id before reporting done.
