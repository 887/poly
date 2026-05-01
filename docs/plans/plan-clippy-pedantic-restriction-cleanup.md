# Plan — Clippy `pedantic` + `restriction` Cleanup (Workspace-Wide Triage)

## Status: 🚧 PLANNED — not started (audit run 2026-04-30; design decisions resolved 2026-05-01)

> Last updated: 2026-05-01
> Audit logs: `/tmp/audit/workspace-clippy.log` + `/tmp/audit/audit-<crate>-{clippy,test}.log`

## Design decisions (frozen 2026-05-01)

User-confirmed answers to the two open design questions:

- **`servers/test-*` strategy:** **per-lint cleanup** (option b). NO blanket
  `#![allow(clippy::pedantic, clippy::restriction)]` on the test-server
  crates. Test fixtures get the same pedantic+restriction discipline as
  production code — agents pick up bad habits from any wiggle-room and
  carry them into prod paths. Yes, this is more work; it's required work.
- **Safety-critical lints (`arithmetic_side_effects`, `as_conversions`,
  `default_numeric_fallback`):** **workspace-wide `warn`** (option ii). One
  global level. NO per-subtree allow carve-outs. UI dev is hard enough
  without programming-logic bugs masquerading as render bugs; the lint
  noise is preferable to the bug-hunting it would prevent. Same
  zero-wiggle-room rationale: agents will mimic any `allow` they find.

---

## 1. Why this plan exists

Workspace `Cargo.toml` (line ~249) just enabled

```toml
[workspace.lints.clippy]
unwrap_used      = "deny"
expect_used      = "deny"
panic            = "deny"
indexing_slicing = "deny"
pedantic    = { level = "warn", priority = -1 }
restriction = { level = "warn", priority = -2 }
```

Result on `cargo clippy --workspace --all-targets`:

| Metric | Value |
|---|---|
| Workspace members audited | 47 |
| Crates that **compiled** (clippy short-circuit-free) | 12 |
| Crates blocked by `error: indexing/slicing may panic` | 4 (cascading to 35 dependents) |
| Total `warning:` lines emitted | **5,564** |
| Total `error:` lines emitted | **15** (4 blocker errors × duplicates per target + summary) |
| Sum of per-target `generated N warnings` lines (lib + tests, with dupes) | **9,473** |
| Distinct lint categories surfaced | ~120 |

The user's instruction is **triage, do not silence**. The bulk of the noise
is `clippy::restriction` (a deliberately conservative group never meant to
be enabled wholesale — see `blanket_clippy_restriction_lints` warning that
clippy itself emits). A small fraction is genuinely useful (`cast_lossless`,
`map_unwrap_or`, `redundant_closure`, `match_same_arms`).

This plan splits the workspace into:

- **Phase 0** — bulk allows for noise lints (one commit, no source edits).
- **Phase 1** — fix the 11 `indexing_slicing` blocker errors so all 47 crates can be linted.
- **Phases 2–6** — per-tier crate cleanup against the *signal* lints that survive Phase 0.
- **Phase 7** — flip workspace `pedantic`/`restriction` to `deny` once the
  workspace is clean.

---

## 2. Sequencing & dependencies

**Phase 0 unblocks everything.** Until the noise is suppressed at the
workspace level, no crate's clippy output is human-readable. Phase 0 is
a single edit to `Cargo.toml` and ships in one commit.

**Phase 1 unblocks the audit.** 11 `error:` lines in `crates/lint-gate`,
`crates/host-bridge` (lib test), `tools/poly-cli`, and `clients/discord`
prevent clippy from checking 35 of 47 crates. After Phase 1 the *real*
per-crate warning counts replace today's "compile-stopped early" data
points.

**Phase 1 blocks any release** because `cargo build --workspace` still
succeeds — these are clippy-only `deny`d lints, not rustc errors. But
the moment we flip Phase 7 (deny clippy in CI), Phase 1 sites become
hard build failures.

**Tier order is a recommendation.** Tier 1 is "load-bearing crates the
user touches every day"; Tier 5 is "dev/fuzz/test infrastructure".
The user can pull a Tier-3 crate forward if priorities shift.

**Conflict warning — `crates/core/src/ui/`** is governed by 6 in-repo
custom lint scripts (`forbid-render-time-read.sh`,
`forbid-long-read-guard.sh`, `forbid-stale-effect-capture.sh`, etc.) plus
`#![deny(deprecated)]` on the migrated `BatchedSignal` API surface.
Pedantic suggestions in this directory **must be triaged for hang-class
implications before applying**:

- `redundant_clone` on a `Signal::read()` guard — DO NOT apply; the clone
  is what makes the guard drop early.
- `needless_pass_by_value` on a `BatchedSignal<T>` parameter — usually
  safe to apply (signals are `Copy`), but check the call site.
- `cognitive_complexity` flags inside `chat_view.rs` / `favorites_sidebar.rs`
  — these are tracked by `plan-component-lints.md` already; cross-reference
  before refactoring.

---

## 3. Top-of-the-mountain noise inventory

### 3.1 Top 20 noisiest crates (by `generated N warnings` from workspace clippy run)

Only the 12 crates that clippy reached are listed; the other 35 will be
populated after Phase 1 unblocks the audit.

| # | Crate | lib warns | lib-test warns | Notes |
|--:|-------|-----------|----------------|-------|
| 1 | `poly-server` | 1,605 | 1,436 | Largest single source; mostly `arbitrary_source_item_ordering` + `implicit_return` + `missing_docs_*`. |
| 2 | `poly-memory-mcp` | 549 | 530 | Heavy `min_ident_chars`, `single_call_fn`, `implicit_return`. |
| 3 | `poly-host-bridge` | 501 | 483 | + 2 indexing-may-panic blockers in lib test. |
| 4 | `poly-devtools-protocol` | 476 | 479 | Same shape — lots of `?` operator, `implicit_return`. |
| 5 | `poly-backup-server` | 475 | 474 | |
| 6 | `poly-client` | 850 | (lib only) | `clients/client` is the polyglot trait crate; many `pub_use`/`absolute_paths`. |
| 7 | `poly-lint-gate` | 495 | (build script) | + 5 indexing-may-panic blockers. |
| 8 | `poly-cli` | 200 | 188 | + 4 indexing-may-panic blockers. |
| 9 | `poly-test-common` | 179 | 198 | |
| 10 | `poly-ui-macros` | 96 | 111 | + 7 in compile_fail tests. |
| 11 | `poly-core` (build script only) | 45 | — | Real `crates/core` lib didn't compile-finish before clippy short-circuited; expected to be in the thousands once Phase 1 unblocks. |
| 12 | `poly-discord` (via chat-mcp dep) | 1,031 | — | + 8 indexing-may-panic blockers. |

After per-crate audits land (background job `bvftjcr2k`), this table will
extend to all 47 crates. Per-crate raw clippy logs at
`/tmp/audit/audit-<name>-clippy.log`.

### 3.2 Top 20 lint categories across the workspace (frequency)

Bucketed by inferred lint name from the warning message text:

| # | Lint | Count | Group | Phase 0 verdict |
|--:|------|------:|-------|-----------------|
| 1 | `implicit_return` | 1,203 | restriction | **ALLOW** — Rust idiom is no `return`; this lint is anti-idiomatic. |
| 2 | `arbitrary_source_item_ordering` | 635 | restriction | **ALLOW** — purely cosmetic; conflicts with logical grouping. |
| 3 | `question_mark_used` | 600 | restriction | **ALLOW** — `?` is the canonical error-propagation; banning it is absurd. |
| 4 | `min_ident_chars` | 438 | restriction | **ALLOW** — `i`, `j`, `n`, `s`, `e` are fine; loop counters and matchers everywhere. |
| 5 | `missing_inline_in_public_items` | 371 | restriction | **ALLOW** — `#[inline]` on every pub item bloats binaries; rustc decides. |
| 6 | `missing_docs_in_private_items` | 307 | restriction | **ALLOW** — private items don't need rustdoc. |
| 7 | `single_call_fn` | 217 | restriction | **ALLOW** — single-call helpers are a fine SRP technique. |
| 8 | `exhaustive_structs`/`exhaustive_enums` | 217 | restriction | **ALLOW workspace-wide** (we don't promise stable wire types here); revisit per-crate if a public API stabilises. |
| 9 | `absolute_paths` | 208 | restriction | **ALLOW** — `crate::module::foo` is often clearer than re-`use` clutter. |
| 10 | `str_to_string` | 143 | restriction | **ALLOW** — `.to_string()` and `.to_owned()` are interchangeable; not a bug. |
| 11 | `missing_errors_doc` | 122 | pedantic | **ALLOW workspace-wide**; opt-in per public-API crate (Phase 6 candidate). |
| 12 | `doc_markdown` | 110 | pedantic | **ALLOW** — too many false positives on identifiers, URLs, version strings. |
| 13 | `arithmetic_side_effects` | 107 | restriction | **KEEP as warn** — workspace-wide, per design decision 2026-05-01. Yes, every `+` either becomes `checked_add` or gets a tightly-scoped `#[allow]` with rationale. Catches real overflow bugs that masquerade as render glitches in WASM. |
| 14 | `let_underscore_untyped` | 91 | restriction | **ALLOW** — `let _ = x;` is idiomatic ignore. |
| 15 | `non_ascii_literal` | 55 | restriction | **ALLOW** — emoji + unicode in test strings + UI messages is intentional. |
| 16 | `must_use_candidate` | 54 | pedantic | **KEEP as warn** — it's actually useful for builder-style methods. Migrate per-crate (Tier 1 first). |
| 17 | `let_underscore_must_use` | 38 | restriction | **KEEP as warn** — borderline signal; catches forgotten `Result`s. |
| 18 | `std_instead_of_core` / `std_instead_of_alloc` | 47 | restriction | **ALLOW** — we are not `no_std`; meaningless here. |
| 19 | `pattern_type_mismatch` | 36 | restriction | **ALLOW** — pedantic about `&Some(x)` vs `Some(&x)`; not a bug. |
| 20 | `as_conversions` | 30 | restriction | **KEEP as warn** — workspace-wide, per design decision 2026-05-01. Use `From`/`TryFrom`/`u32::try_from()` instead of bare `as`; the latter silently truncates. `cast_lossless`/`cast_possible_truncation` overlap but `as_conversions` catches the cases where the user wrote `as` to bypass the type system at all. |

**KEEP as warn (signal lints)** that survive Phase 0:

- `must_use_candidate` (54) — pedantic, real signal.
- `let_underscore_must_use` (38) — restriction, real signal.
- `cast_possible_truncation` (24) — pedantic, real signal.
- `cast_lossless` — pedantic, real signal.
- `map_unwrap_or` (19) — pedantic, real signal.
- `redundant_closure_for_method_calls` (28) — pedantic, real signal.
- `needless_pass_by_value` (14) — pedantic, real signal (clones are expensive).
- `default_numeric_fallback` (20) — restriction, **real signal everywhere** (not just numeric code). Workspace-wide warn per design decision 2026-05-01.
- `arithmetic_side_effects` (107) — restriction, **promoted from ALLOW to KEEP** per design decision 2026-05-01.
- `as_conversions` (30) — restriction, **promoted from ALLOW to KEEP** per design decision 2026-05-01.
- `string_slice` (28) — restriction, real signal (UTF-8 panics).
- `integer_division` (15) — restriction, sometimes signal — review per crate.
- `map_err_ignore` (10) — restriction, real signal (loses error context).
- `print_stdout` / `print_stderr` (19) — restriction, real signal in libraries.
- `mod_module_files` (8) — restriction, project-wide convention check (we have 8 violations in `servers/`).
- `wildcard_enum_match_arm` (7) — restriction, real signal.
- `missing_trait_methods` (4) — restriction, occasional signal (`clone_from`).
- `match_same_arms` (5) — pedantic, real signal.
- `unwrap_used` regressions (1+) — already `deny`d but counted here; Phase 1 fix.

### 3.3 Phase-1 blocker errors (15 `error:` lines)

| File | Line | Error |
|---|---|---|
| `crates/lint-gate/build/custom_block_usage.rs` | 82 | `slicing may panic` |
| `crates/lint-gate/build/custom_block_usage.rs` | 83, 86×2, 89 | `indexing may panic` |
| `tools/poly-cli/src/main.rs` | 183×2, 278, 283 | `indexing may panic` |
| `crates/host-bridge/src/client_config.rs` | 326 (test), 408 (test) | `indexing may panic` |
| `clients/discord/src/http.rs` | 385 | `indexing may panic` (`body["key"] = json!(…)` — serde `IndexMut`) |
| `clients/discord/src/lib.rs` | 1133, 1136, 1139, 1142, 1145, 2011, 2014 | `indexing may panic` (same pattern) |

Total **20** distinct error sites (some `error:` lines are summary "could
not compile" messages, hence the 15 vs 20 discrepancy).

Discord pattern (8 sites): `serde_json::Value` indexing with `[…] = json!(…)`
panics on non-object values. Refactor to `body.as_object_mut().expect("…")
.insert(k.into(), v)` — or a small helper. Cannot use `.expect` (workspace-deny);
must use `if let Some(obj) = body.as_object_mut() { obj.insert(…); }`.

Lint-gate / poly-cli sites: classic `bytes[i]` / `args[i]` indexing in parser
loops. Replace with `bytes.get(i).copied()` / `args.get(i)?.as_str()`.

---

## Phase 0 — Bulk allows for noise lints

**Effort:** S (15 min) | **Depends on:** nothing | **Blocks:** Phases 2-7.

Edit `Cargo.toml` `[workspace.lints.clippy]` to add `<lint> = "allow"` for
the 18 noise lints from §3.2, each with a one-line `# rationale` comment
on the same line above. Keep the existing `unwrap_used`/`expect_used`/
`panic`/`indexing_slicing = "deny"`. Keep `pedantic`/`restriction = "warn"`
so newly-violated signal lints surface.

**Acceptance:** `cargo clippy --workspace --all-targets 2>&1 | grep -c '^warning:'`
drops from 5,564 to **< 1,000**. (Estimate based on §3.2: 18 lints
suppress ~4,800 of 5,564 warnings.)

- [ ] **A.1** Add `Cargo.toml` allow block + per-line rationale comments for the 18 lints from §3.2.
- [ ] **A.2** Re-run `cargo clippy --workspace --all-targets > /tmp/audit/post-phase-0.log 2>&1`; assert warning count dropped 80%+.
- [ ] **A.3** Commit: `chore(lints): suppress noise pedantic/restriction lints (cleanup phase 0)`.

---

## Phase 1 — Fix the 11 indexing/slicing blocker errors

**Effort:** S (45 min) | **Depends on:** Phase 0 (so the diff is readable)
| **Blocks:** Phase 2+.

Each site is a small refactor (`bytes.get(i)` / `as_object_mut()` /
`get_mut`). All 20 sites listed in §3.3.

- [ ] **B.1** `crates/lint-gate/build/custom_block_usage.rs` — replace 5 raw indexes with `.get()` + `?` / `else continue`.
- [ ] **B.2** `tools/poly-cli/src/main.rs` — 4 raw indexes → `args.get(i)` + `match`.
- [ ] **B.3** `crates/host-bridge/src/client_config.rs` — 2 test indexes → `assert_eq!(back.mechanisms.first(), …)` + named field assertions.
- [ ] **B.4** `clients/discord/src/http.rs` + `clients/discord/src/lib.rs` — 8 `body["k"] = v` sites → `if let Some(obj) = body.as_object_mut() { obj.insert(…); }` (or a small `set_field(&mut body, k, v)` helper).
- [ ] **B.5** Re-run `cargo clippy --workspace --all-targets`; assert **0** `error:` lines, all 47 crates checked.
- [ ] **B.6** Re-run audit script; populate per-crate warning counts for the previously-blocked 35 crates and append to §3.1 table.
- [ ] **B.7** Commit: `fix(lints): eliminate 20 indexing_slicing panic sites blocking workspace clippy`.

---

## Phase 2 — Tier 1: Load-bearing crates

**Crates:** `crates/core`, `crates/host-bridge`, `crates/plugin-host`,
`clients/discord`, `clients/matrix`, `mcp/chat-mcp`, `apps/web`.

Promoted to Tier 1 per user direction 2026-05-01: `plugin-host` and
`host-bridge` are foundational + small enough that high-signal lints
(`arithmetic_side_effects`, `as_conversions`,
`default_numeric_fallback`) catch real bugs that would otherwise leak
into every UI path that uses them. Do them first; the rest of the
workspace benefits from cleaner foundations.

**Effort:** L (4-6 hours per crate, total ~20-30 h) | **Depends on:** Phase 1.

These are touched daily. Pedantic + restriction signal lints here are
worth fixing because they catch real bugs and the user re-encounters
them on every edit.

Per-crate sub-step:

- [ ] **C.1** `crates/host-bridge` — start here. Foundational, ~500 warns.
      Focus on `arithmetic_side_effects` (KV byte offsets), `as_conversions`
      (length casts), `default_numeric_fallback`. Effort: L (~6h).
- [ ] **C.2** `crates/plugin-host` — second. Sandboxing + WIT plumbing —
      every UI plugin call goes through here, so any cast/overflow bug
      cascades. Effort: L (~5h).
- [ ] **C.3** `crates/core` — third. Apply `must_use_candidate`,
      `redundant_closure_for_method_calls`, `map_unwrap_or`, `cast_lossless`,
      plus the three safety lints. Effort: L (~6-10h, ~50-200 sites).
      Hang-class cross-check: see §2 (don't blindly apply `redundant_clone`
      on Signal guards, don't apply `needless_pass_by_value` on `Signal<T>`
      params without checking).
- [ ] **C.4** `clients/discord` — also fix `string_slice` + `cast_possible_truncation` from JSON parsing. Effort: M (~3h).
- [ ] **C.5** `clients/matrix` — same pattern as discord. Effort: M (~2-3h).
- [ ] **C.6** `mcp/chat-mcp` — large surface, lots of HTTP handlers; focus on `wildcard_enum_match_arm` + `must_use_candidate`. Effort: L (~4h).
- [ ] **C.7** `apps/web` — small bin crate; mostly `single_call_fn` (allow per-file) and `must_use_candidate` on the WASM entry. Effort: S (~30m).
- [ ] **C.8** Per-crate acceptance: `cargo clippy -p <crate> --all-targets 2>&1 | grep -c '^warning:'` is 0.
- [ ] **C.9** Commits: one per crate, e.g. `chore(core): clippy pedantic cleanup (tier 1)`.

---

## Phase 3 — Tier 2: Active client backends

**Crates:** `clients/teams`, `clients/lemmy`, `clients/forgejo`,
`clients/github`, `clients/stoat`, `clients/poly-server` (a.k.a.
`server-client` crate name TBD), `clients/hackernews`.

**Effort:** M (1-2h per crate, total ~10-14h) | **Depends on:** Phases 1-2.

These follow the same pattern as `clients/discord` (tier 1) — JSON-shaped
HTTP wrappers around remote APIs. Most warnings will be the same
recurring patterns; opportunistic copy-paste of fixes from Phase 2 is
expected.

- [ ] **D.1** `clients/teams` — Effort M.
- [ ] **D.2** `clients/lemmy` — Effort M.
- [ ] **D.3** `clients/forgejo` — Effort M.
- [ ] **D.4** `clients/github` — Effort M.
- [ ] **D.5** `clients/stoat` — Effort M.
- [ ] **D.6** `clients/server-client` (poly-server-client) — Effort M.
- [ ] **D.7** `clients/hackernews` — Effort S (smaller surface).
- [ ] **D.8** Per-crate acceptance: `cargo clippy -p <crate> --all-targets` clean.

---

## Phase 4 — Tier 3: Support / infrastructure

**Crates:** `crates/host-sandbox`, `apps/poly-host`,
`tools/poly-cli`, `crates/plugin-host-tests`,
`crates/ui-types`, `crates/ui-macros`, `crates/lint-gate`,
`mcp/devtools-protocol`, `mcp/desktop-devtools-mcp`,
`mcp/web-devtools-mcp`, `mcp/electron-devtools-mcp`, `mcp/memory-mcp`.

(`crates/host-bridge` and `crates/plugin-host` moved to Tier 1 — they're
foundational and gate every UI path.)

**Effort:** M (1-2h per crate, total ~14-20h) | **Depends on:** Phases 1-2.

Infrastructure code; less velocity than tier 1, but still warrants
cleanup. `host-bridge` and `memory-mcp` are 500+ warnings each, so
allow-per-file is the realistic strategy for many sites (e.g. allow
`module_name_repetitions` in the bridge route module, allow
`single_call_fn` in MCP tool handlers).

- [ ] **E.1** `crates/host-sandbox` — Effort M.
- [ ] **E.2** `apps/poly-host` — Effort S.
- [ ] **E.3** `tools/poly-cli` (188 + 200 warns) — Effort M.
- [ ] **E.4** `crates/plugin-host-tests` — Effort M.
- [ ] **E.6** `crates/ui-types`, `crates/ui-macros`, `crates/lint-gate` — Effort M (combined; macros heavy on `pub_use` + `absolute_paths`).
- [ ] **E.7** `mcp/devtools-protocol` (476 lib + 479 lib-test warns) — Effort M; mostly `implicit_return` + `?` operator already allow-listed in Phase 0, so this should drop dramatically.
- [ ] **E.8** `mcp/desktop-devtools-mcp` + `mcp/web-devtools-mcp` + `mcp/electron-devtools-mcp` — Effort S (each).
- [ ] **E.9** `mcp/memory-mcp` (549 + 530 warns) — Effort M post-Phase-0.

---

## Phase 5 — Tier 4: Test servers and test infrastructure

**Crates:** `servers/server`, `servers/backup-server`, `servers/test-common`,
`servers/test-matrix`, `servers/test-stoat`, `servers/test-discord`,
`servers/test-teams`, `servers/test-poly`, `servers/test-lemmy`,
`servers/test-hackernews`, `servers/test-forgejo`, `servers/test-github`,
`servers/test-runner`.

**Effort:** L (test code accepts more sloppiness; mostly drive-by)
| **Depends on:** Phases 1-2.

`servers/server` is the **largest single source of warnings** in the
workspace (3,041 across lib + lib-test). Phase 0 will collapse most of
it; remaining signal lints will be `must_use_candidate` on builder API
and `wildcard_enum_match_arm` in match dispatchers.

For test-* crates: **NO blanket `#![allow(...)]` escape hatch** per design
decision 2026-05-01. Test fixtures earn the same per-lint cleanup as
production code. Specific lints (`unwrap_used`, `expect_used`, `panic`)
already get test-only `#![allow(...)]` per existing user feedback (see
`feedback_test_lints.md`); pedantic+restriction do NOT get added to
that list.

- [ ] **F.1** `servers/server` (3,041 warns total) — Effort L (~6h post-Phase-0).
- [ ] **F.2** `servers/backup-server` (949 warns) — Effort M.
- [ ] **F.3** `servers/test-common` (377 warns) — Effort M.
- [ ] **F.4** `servers/test-discord` — per-lint cleanup, not blanket allow. Effort M.
- [ ] **F.5** `servers/test-matrix` — per-lint cleanup. Effort M.
- [ ] **F.6** `servers/test-stoat` — per-lint cleanup. Effort M.
- [ ] **F.7** `servers/test-teams` — per-lint cleanup. Effort M.
- [ ] **F.8** `servers/test-poly` — per-lint cleanup. Effort M.
- [ ] **F.9** `servers/test-lemmy` — per-lint cleanup. Effort M.
- [ ] **F.10** `servers/test-hackernews` — per-lint cleanup. Effort M.
- [ ] **F.11** `servers/test-forgejo` — per-lint cleanup. Effort M.
- [ ] **F.12** `servers/test-github` — per-lint cleanup. Effort M.
- [ ] **F.13** `servers/test-runner` — per-lint cleanup. Effort S.

---

## Phase 6 — Tier 5: App shells, dev tools, fuzz

**Crates:** `apps/web`, `apps/desktop`, `apps/desktop-blitz`,
`apps/desktop-electron`, `apps/desktop-devtools`, `apps/desktop-web`,
`apps/android`, `apps/ios`, `apps/poly-host`, `clients/demo`,
`clients/client` (the polyglot trait crate, 850 warns).

**Effort:** M (mostly already covered by tier 1-3) | **Depends on:** Phases 1-5.

These are thin shells around the core, mostly already touched in earlier
phases. This phase is the "did we miss anything?" sweep.

- [ ] **G.1** `clients/client` (850 warns — the trait surface) — Effort M; mostly `pub_use` + `absolute_paths` (already allow-listed in Phase 0). Real signal: `missing_trait_methods` (`clone_from`).
- [ ] **G.2** `clients/demo` — Effort S.
- [ ] **G.3** `apps/web`, `apps/desktop`, `apps/desktop-blitz`, `apps/desktop-electron`, `apps/desktop-web`, `apps/desktop-devtools` — Effort S each (small bin crates).
- [ ] **G.4** `apps/android`, `apps/ios` — Effort S (often empty stubs).

(Excluded from this plan: `tools/lints/poly-lints` and `mcp/chat-mcp/fuzz`
— both deliberately outside the workspace per `Cargo.toml` comments at
line ~270.)

---

## Phase 7 — CI gate: flip `pedantic`+`restriction` to `deny`

**Effort:** S (15 min) | **Depends on:** Phases 0–6 fully ticked.

- [ ] **H.1** Edit `Cargo.toml` `[workspace.lints.clippy]`: change `level = "warn"` to `level = "deny"` for both `pedantic` and `restriction`.
- [ ] **H.2** Run `cargo clippy --workspace --all-targets -- -D warnings`; assert exit 0.
- [ ] **H.3** Add CI step (or extend existing one) in `.github/workflows/lint-test.yml`: `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] **H.4** Document the flip in `CLAUDE.md` "Lint policy" section.
- [ ] **H.5** Commit: `ci(lints): enforce clippy pedantic+restriction = deny workspace-wide`.

**Acceptance:** `cargo clippy --workspace --all-targets -- -D warnings`
exits 0 on a clean checkout.

---

## 4. Open questions

1. **Should `must_use_candidate` apply to internal helpers?** The lint
   has 54 hits; many are private fn that don't benefit from `#[must_use]`.
   Per-crate override may be more sensible than annotating each fn.
2. **`exhaustive_structs`/`exhaustive_enums`** — currently allow-listed
   workspace-wide. If the `clients/client` trait surface stabilises into
   a public API, opt those crates back in to enforce
   `#[non_exhaustive]` on wire types.
3. **`crates/core/src/ui/`** — the existing 6 hang-class lint scripts
   may flag pedantic suggestions as conflicting (e.g. `redundant_clone`
   suggests removing a clone that's load-bearing for guard-drop timing).
   Triage HIGH-impact pedantic suggestions per-file with reference to
   `docs/dev/reactive-state.md`.

## 4.1 Resolved questions (2026-05-01)

- ✅ **`servers/test-*` blanket allow** — DECIDED **per-lint cleanup**.
  Test fixtures get the same scrutiny as production code. Phase 5
  updated.
- ✅ **Safety-critical lint scope** — DECIDED **workspace-wide warn** for
  `arithmetic_side_effects`, `as_conversions`, `default_numeric_fallback`.
  No per-subtree allow carve-outs. §3.2 updated.

---

## 5. Per-crate audit log artifacts

- Workspace clippy: `/tmp/audit/workspace-clippy.log` (5,564 warnings,
  15 errors, 65,148 lines).
- Per-crate clippy + test-build logs (background job `bvftjcr2k`,
  populating ~36 of 47 crates over ~2-3 hours):
  - `/tmp/audit/audit-<name>-clippy.log`
  - `/tmp/audit/audit-<name>-test.log`
  - Summary: `/tmp/audit/per-crate-summary.txt` (one line per crate).
- Lint frequency table (Python script): regenerate with the script in §3.2
  of this plan.

After Phase 1 lands, re-run the workspace clippy and replace the §3.1
table with all-47-crates data.
