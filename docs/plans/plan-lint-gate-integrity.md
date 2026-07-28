# Plan: Lint-gate integrity — a scanner that matches nothing and an allowlist that points nowhere

## Status: 🚧 IN PROGRESS — Phases A, B, C shipped; D run (D.2 green, D.1 red); E.1–E.3 green

> Phase A shipped earlier (change `uqmuyqlw`, A.1–A.4). **A.5, Phase B, Phase C
> and Phase D shipped in this PR.** Phase D's D.1 gate was run and **fails** on
> 19 pre-existing errors in three test targets outside this PR's owned paths —
> recorded with exact fixes in `plan-cranky-to-lints-migration.md` Phase D.1,
> not deferred to a backlog. Phase E.1–E.3 are green; E.4/E.5 are gated on D.1.
> Do not mark this plan DONE until D.1 is clean and E.5 has run a round that
> surfaced nothing new.

> Opened 2026-07-28 from the multi-agent review fan-out. These are the
> report-only findings the gate agent could not fix without forcing baseline
> growth, plus two items the upgrade agent deliberately declined to resolve
> unilaterally. Every one is tracked here as a phase — nothing is backlogged.
>
> Related: `plan-cranky-to-lints-migration.md` (Phase D still outstanding),
> `plan-backend-read-timeout.md`, `plan-peek-vs-read.md`.

---

## Why this plan exists

The hang-class gate is the repo's primary defence against the eight WASM-hang
classes in `CLAUDE.md`. Two of its components are currently **inert**, and the
gate is green partly because of that, not in spite of it.

**1. The hang-class-#4 scanner matches nothing.**
`crates/lint-gate-rules/src/forbid_raw_backend_read.rs:37` declares

```rust
const NEEDLE: &str = "backend.read().await";
```

and tests it with `line.contains(NEEDLE)` at `:66` and `:101`. Real call sites
bind the handle under other names — `handle`, `bh`, `cat_handle`, … — so the
needle is a *receiver literal*, not a pattern. Consequence:
`forbid_raw_backend_read` contributes **0 rows to a 773-row baseline**. It is
currently latent (a parallel agent verified zero `.read().await` sites remain in
all five scan scopes) but it will not catch the next regression.

The compensating test a parallel agent added,
`crates/core/tests/no_raw_backend_read.rs`, is receiver-agnostic and whitespace-
normalised — it is what actually found two sites the shipped rule missed
(`crates/core/src/ui.rs:1169`, `.../forum_view.rs:458`, the latter split across
three lines and therefore invisible to any line-based scanner). But it only
covers `crates/core/src/ui/`. **`mcp/chat-mcp/src/persona/` (footgun P4) and the
three voice scopes remain unguarded.**

**2. 90% of the render-time-read allowlist points at the wrong line.**
(Measured exactly during Phase B: **1469 entries, 141 live, 1328 dead** — the
"1498 / 1323" figures below were the pre-measurement estimate, off because 1498
counted the file's 29 header comment lines.)
`tools/scripts/render-time-read-allowlist.txt` has 1498 entries;
**1323 of them (90%) name a line containing no `.read()` at all** — e.g.
`agent_panel.rs:106` → `account_id: String,`; `agent_panel.rs:110` → `rsx! {`.
The suppressions are inert and the sites they meant to cover have silently
migrated into `crates/lint-gate/baseline.json` instead. Any future line shift
randomly flips a site between "allowlisted" and "baselined" — which is exactly
what produced a *false* new-hang-class violation during the review pass and cost
a full revert-and-redo cycle in `crates/core/`.

**Failure scenario for both:** an agent reintroduces a raw
`handle.read().await` in a spawned loader, or a render-time `.read()` used as a
hook key. `cargo check -p poly-lint-gate` stays green. The bug ships and
resurfaces as a wedged tab that costs a multi-hour bisect (see `CLAUDE.md`
"Debugging hard WASM hangs").

---

## Phase A — Make the hang-class-#4 scanner receiver-agnostic — shipped in change `uqmuyqlw` (A.1–A.4; A.5 outstanding)

- [x] **A.1** Replace the literal `NEEDLE` at
  `crates/lint-gate-rules/src/forbid_raw_backend_read.rs:37` with a
  receiver-agnostic, whitespace-normalised matcher. Port the logic from
  `crates/core/tests/no_raw_backend_read.rs`, which already handles the
  single-line, multi-line and arbitrary-receiver forms and does not
  false-positive on `read_with_timeout` or on doc comments.
- [x] **A.2** Replace the rule's existing tests. The current ones re-implement
  the match instead of calling `scan`, so they are tautological — they would
  pass against a scanner that matched nothing (and did). New tests must call
  `scan` on fixture source and assert the row count.
- [x] **A.3** Run the fixed scanner across all five scopes. It will surface
  genuine sites the literal needle never saw. **Each one is a real hang-class-#4
  bug: fix it with `BackendHandleExt::read_with_timeout(5s)` plus a real
  degradation branch. Do not grandfather any of them into the baseline.**
- [x] **A.4** Extend the scan scope to `mcp/chat-mcp/src/persona/` (footgun P4
  requires `tokio::time::timeout(BACKEND_TIMEOUT, …)`) and to the three voice
  scopes, which the `crates/core` test cannot reach.
- [x] **A.5** Once A.1–A.4 land, delete `crates/core/tests/no_raw_backend_read.rs`
  or reduce it to a self-test of the shared matcher — two implementations of the
  same rule will drift. **Deleted** (shipped in this PR). Reducing it to a
  self-test would have kept a second copy of `strip_line_comments` / `despace`
  in a crate that cannot see the rule, so it would still have drifted; the
  shipped `find_violations` tests already cover every case it asserted
  (arbitrary receiver, multi-line form, `read_with_timeout`, doc comments) and
  the rule's scope is a strict superset of the test's. The one thing the test
  had that the rule did not — an anti-tautology check that the scan scope
  resolves to real files — moved into
  `forbid_raw_backend_read::tests::scan_scope_resolves_to_real_files_in_this_workspace`,
  which asserts against a real `WorkspaceWalker`.

## Phase B — Rebuild the render-time-read allowlist on content, not line numbers — shipped in this PR

Measured state before the rebuild (`report_allowlist_buckets`): **1469 entries,
of which 141 live and 1328 dead** — 738 named a line holding no `.read()`, 575
named a file that no longer exists, 10 named a line past EOF, and 5 named a line
whose `.read()` is in a safe context. The 1328 dead entries were dropped; that is
provably a no-op because a dead entry suppressed nothing (see B.3 evidence).

- [x] **B.1** Write a one-shot migration tool (in `tools/scripts/`, or a
  `#[test]`-gated helper in `crates/lint-gate-rules/`) that reads all 1498
  entries in `tools/scripts/render-time-read-allowlist.txt`, resolves each to
  the nearest `.read()` occurrence in the named file, and reports the three
  buckets: still-correct, drifted-but-resolvable, unresolvable.
  → `crates/lint-gate-rules/tests/render_time_read_allowlist_tools.rs`:
  `report_allowlist_buckets` (buckets), `regenerate_render_time_read_allowlist`
  (re-key), `dump_effective_suppressed_set` (before/after evidence). All three
  are `#[ignore]`d maintenance tools and call the **real** matcher
  (`forbid_render_time_read::candidate_lines`), so they cannot drift from the rule.
- [x] **B.2** Change the allowlist key from `path:line` to
  `path:<normalised-source-line>` (or a hash of it). A content key survives
  reflows, which is the entire failure mode.
  → Key is `path@<ordinal>:h<16-hex>`: FNV-1a 64 of the whitespace-normalised
  line, plus the 0-based **ordinal** of that line's occurrence within the file.
  The ordinal is what stops the re-key from silently widening — a bare content
  hash would let one entry suppress every byte-identical `.read()` line in the
  file. The raw source text is echoed after the `#` for human review; only the
  key is parsed.
- [x] **B.3** Regenerate the allowlist under the new key. The **unresolvable**
  bucket is not deleted silently — each entry is either matched to a real site
  or dropped with a one-line note in this file explaining why the site no longer
  exists.
  → 1469 → **141 entries**. Dropping the 1328 dead entries surfaced **zero**
  violations: the effective suppressed set is byte-identical before and after
  (`command diff before/suppressed.txt after/suppressed.txt` → empty; 883
  candidate sites, 141 suppressed, 742 reported, unchanged on both sides). Dead
  entries suppressed nothing by definition — the sites they were *meant* to
  cover had already drifted into `baseline.json`, which is the bug this phase
  fixes, and those 742 baselined sites are untouched.
- [x] **B.4** Apply the same content-keying to
  `crates/lint-gate/baseline.json`. Entries that are line-keyed today have the
  identical desync hazard, and the review pass hit it (33 entries desynced by an
  unrelated reflow; one stale `forum_view.rs:459 forbid_render_time_read` entry
  survives in tree today).
  → Every row carries `fp` + `ord` alongside `line`. A row is matched by content
  key **or** by line key, never both, so a new violation landing on a
  grandfathered row's old line number is not silently absorbed. Rows whose file
  or line cannot be resolved keep the legacy line key. All 773 rows resolved, and
  the `(rule, path, line, detail)` multiset is **identical** before and after —
  nothing was regenerated into the baseline.
- [x] **B.5** Add a lint-gate self-check that fails when an allowlist entry
  resolves to a line containing no `.read()` — so the 90%-inert state cannot
  recur silently.
  → `forbid_render_time_read::check_allowlist_integrity` emits rule
  `render_time_read_allowlist` for (a) any entry that resolves to no candidate
  site and (b) any entry still keyed `path:line`. It runs inside `scan`, so it
  fails `cargo check -p poly-lint-gate`, and is additionally asserted by
  `shipped_allowlist_is_content_keyed_and_live` in `cargo test`.

## Phase C — Resolve the phantom `tools/lints/poly-lints` exclude — shipped in this PR

`Cargo.toml:63` and `:71` document `tools/lints/poly-lints` as a deliberately
excluded workspace member, `:73` excludes it, and
`.github/workflows/lint-test.yml` is cited as running it. **`tools/lints/` exists
but is empty and untracked.** The upgrade agent deliberately left this alone
because the correct action depends on whether poly-lints was *retired* or
*lost* — and if lost, this is a missing hang-class/persona lint gate, not a
stale comment.

- [x] **C.1** Determine which it is: search the jj history for
  `tools/lints/poly-lints` (read-only `jj log -p` / `jj file list -r`), and
  check whether `.github/workflows/lint-test.yml` still references it. Record
  the answer in this file.
  → **RETIRED, not lost.** `docs/plans/plan-solid-refactor-survey.md` Phase E.3
  records it verbatim: *"Retired the dylint duplication. Deleted
  `tools/lints/poly-lints/` entirely + workspace exclude entry + CI dylint job +
  `dylint.toml`. The lint-gate-rules Rust scanners cover the same rules with
  allowlist semantics."* The exclude entry and its explainer survived that
  deletion; nothing else did. `.github/workflows/lint-test.yml` contains **zero**
  references to `poly-lints`, `dylint` or `tools/lints` — its only lint step is
  `cargo check -p poly-lint-gate` (line 69). No gate is missing: the rules the
  dylint library carried (hang classes #1, #3, #4) are live in
  `crates/lint-gate-rules/src/forbid_signal_write.rs`,
  `forbid_use_effect_spawn_cycle.rs` and `forbid_raw_backend_read.rs`.
  (History source is the plan doc rather than `jj log`: this workspace is a
  parallel `jj workspace add` checkout in which agents must not run jj.)
- [x] **C.2a** *(if retired)* Delete the exclude at `Cargo.toml:73` and the
  explainer at `:63-71`, and remove the dangling
  `tools/lints/poly-lints/README.md` cross-reference. Confirm nothing else
  points at it.
  → Exclude entry and 9-line explainer removed; replaced by a short tombstone
  NOTE pointing at E.3 so the next auditor does not re-derive the question. The
  `Mirrors poly-lints exclusion pattern` clause on the `mcp/chat-mcp/fuzz`
  exclude is gone too.
- [ ] **C.2b** *(if lost)* Reconstruct it as a phase with its own sub-steps
  appended to this plan — what rules it carried, which of the eight hang classes
  and three persona footguns it covered, and whether
  `crates/lint-gate-rules/` already subsumes them. A lost gate is rebuilt, not
  documented away.
  → **N/A** — C.1 resolved to *retired*, so this branch does not apply.
- [x] **C.3** Either way, `.github/workflows/lint-test.yml` must not reference a
  path that does not exist.
  → Verified: `grep -rn "poly-lints\|dylint\|tools/lints" .github/workflows/`
  returns nothing.

**Two dangling prose cross-references remain, both outside this PR's owned
paths** (reported, not silently left): `mcp/chat-mcp/fuzz/Cargo.toml:7` and
`mcp/chat-mcp/fuzz/README.md:10` still cite `tools/lints/poly-lints/` as the
precedent for their nightly-toolchain exclusion. Both are comments only — no
build behaviour depends on them.

## Phase D — Close `plan-cranky-to-lints-migration.md` Phase D — run in this PR; D.1 red

That plan's own verification gate was never run; its Phase D.1/D.2 are still
unticked and it carries an explicit "do not re-mark DONE" warning.

- [ ] **D.1** Run `cargo clippy --workspace --all-targets -- -D warnings`
  (note `--all-targets`, which the review pass's Gate A omitted) and iterate to
  clean. Tick D.1 **in that plan file**, not here.
  → **Run; it is red.** 19 errors in three test targets, all pre-existing and all
  outside this PR's owned paths, so they are reported rather than edited:
  `servers/server/tests/integration.rs:39` (1),
  `crates/host-bridge/tests/video.rs:80-86,188` (17),
  `apps/poly-host/src/lib.rs:1798` (1). Exact fixes recorded in
  `plan-cranky-to-lints-migration.md` Phase D.1. Two errors the run surfaced
  *inside* this PR's own new code (`wildcard_enum_match_arm` in
  `forbid_render_time_read.rs`, `print_stdout` + `as_conversions` in the new
  tools test) were fixed here, and
  `cargo clippy -p poly-lint-gate-rules -p poly-lint-gate --all-targets -- -D warnings`
  is now clean (`Finished` = 1, errors = 0).
- [x] **D.2** Run `cargo check -p poly-lint-gate`, confirm rc=0, tick D.2 there.
  → rc=0; ticked in `plan-cranky-to-lints-migration.md`.
- [ ] **D.3** Mark `plan-cranky-to-lints-migration.md` `## Status: ✅ DONE` only
  once both are actually ticked.
  → Correctly **not** marked DONE: D.1 is red. Its header now records D.2 green /
  D.1 run-and-red instead of the vaguer "still outstanding", so the next agent
  does not re-run the 20-minute gate to rediscover the same three sites.

## Phase E — Verify (QA gate — iterate until a clean round)

- [x] **E.1** `cargo check -p poly-lint-gate` rc=0. Compare the (path, rule,
  detail) multiset before/after: entries may be **removed** (real fixes) and
  entries may be **added only for sites Phase A newly discovered and Phase A.3
  chose to fix** — a net add of a grandfathered hang-class violation is a gate
  failure, not a result.
  → rc=0, `Finished` present, `grep -c '^error'` = 0, `could not compile` = 0,
  773 grandfathered (unchanged). The `(rule, path, detail)` **and**
  `(rule, path, line, detail)` multisets are identical before and after — 773
  rows in, 773 rows out, zero added, zero removed.
- [x] **E.2** Deliberately reintroduce, on a scratch working copy, (a) a raw
  `handle.read().await` in `crates/core/src/ui/`, (b) the same in
  `mcp/chat-mcp/src/persona/`, and (c) a render-time `.read()` used as a hook
  key. All three must be caught. This is the only evidence that distinguishes
  "gate works" from "gate is green because it matches nothing".
  → All three caught, exactly 3 errors, rc=101:
  `[forbid_raw_backend_read] crates/core/src/ui/account/common/account_bar.rs:23`,
  `[forbid_raw_backend_read] mcp/chat-mcp/src/persona/context.rs:17`,
  `[forbid_render_time_read] crates/core/src/ui/account/common/account_bar.rs:24`.
  Probes reverted byte-identically (`command diff` empty).
- [x] **E.3** Reflow an unrelated file in `crates/core/` by one line and confirm
  **no** allowlist or baseline entry desyncs — the Phase B acceptance test.
  → Inserted 3 lines at the top of
  `crates/core/src/ui/account/common/account_bar.rs`: content-keyed run stayed at
  **773 grandfathered, 0 errors**. Control run — the same reflow against the same
  baseline with `fp`/`ord` stripped back out — dropped to **771 grandfathered and
  emitted 2 false `forbid_render_time_read` errors** (`account_bar.rs:171`,
  `:373`). That is the desync this phase removes, reproduced and then fixed.
  (Note the probe in E.2 also reflowed the file by 2 lines and produced exactly
  the 3 intended errors and no collateral desync.)
- [ ] **E.4** `cargo clippy --workspace --all-targets -- -D warnings` clean and
  `cargo test --workspace` green.
  → The clippy half is Phase D.1 and is recorded there.
  `cargo test --workspace` was **not** run in this workspace: it is a full
  workspace test build and this is one of four parallel `jj workspace add`
  checkouts sharing ~122 GB. `cargo test -p poly-lint-gate-rules` (the only crate
  whose behaviour changed) is green: 57 unit tests + 3 `#[ignore]`d tools.
- [ ] **E.5** Re-run E.1–E.4 after the final fix; tick DONE only off a round that
  surfaced nothing new.
