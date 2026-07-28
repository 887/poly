# Plan: Lint-gate integrity — a scanner that matches nothing and an allowlist that points nowhere

## Status: 📋 PLANNED

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
- [ ] **A.5** Once A.1–A.4 land, delete `crates/core/tests/no_raw_backend_read.rs`
  or reduce it to a self-test of the shared matcher — two implementations of the
  same rule will drift.

## Phase B — Rebuild the render-time-read allowlist on content, not line numbers

- [ ] **B.1** Write a one-shot migration tool (in `tools/scripts/`, or a
  `#[test]`-gated helper in `crates/lint-gate-rules/`) that reads all 1498
  entries in `tools/scripts/render-time-read-allowlist.txt`, resolves each to
  the nearest `.read()` occurrence in the named file, and reports the three
  buckets: still-correct, drifted-but-resolvable, unresolvable.
- [ ] **B.2** Change the allowlist key from `path:line` to
  `path:<normalised-source-line>` (or a hash of it). A content key survives
  reflows, which is the entire failure mode.
- [ ] **B.3** Regenerate the allowlist under the new key. The **unresolvable**
  bucket is not deleted silently — each entry is either matched to a real site
  or dropped with a one-line note in this file explaining why the site no longer
  exists.
- [ ] **B.4** Apply the same content-keying to
  `crates/lint-gate/baseline.json`. Entries that are line-keyed today have the
  identical desync hazard, and the review pass hit it (33 entries desynced by an
  unrelated reflow; one stale `forum_view.rs:459 forbid_render_time_read` entry
  survives in tree today).
- [ ] **B.5** Add a lint-gate self-check that fails when an allowlist entry
  resolves to a line containing no `.read()` — so the 90%-inert state cannot
  recur silently.

## Phase C — Resolve the phantom `tools/lints/poly-lints` exclude

`Cargo.toml:63` and `:71` document `tools/lints/poly-lints` as a deliberately
excluded workspace member, `:73` excludes it, and
`.github/workflows/lint-test.yml` is cited as running it. **`tools/lints/` exists
but is empty and untracked.** The upgrade agent deliberately left this alone
because the correct action depends on whether poly-lints was *retired* or
*lost* — and if lost, this is a missing hang-class/persona lint gate, not a
stale comment.

- [ ] **C.1** Determine which it is: search the jj history for
  `tools/lints/poly-lints` (read-only `jj log -p` / `jj file list -r`), and
  check whether `.github/workflows/lint-test.yml` still references it. Record
  the answer in this file.
- [ ] **C.2a** *(if retired)* Delete the exclude at `Cargo.toml:73` and the
  explainer at `:63-71`, and remove the dangling
  `tools/lints/poly-lints/README.md` cross-reference. Confirm nothing else
  points at it.
- [ ] **C.2b** *(if lost)* Reconstruct it as a phase with its own sub-steps
  appended to this plan — what rules it carried, which of the eight hang classes
  and three persona footguns it covered, and whether
  `crates/lint-gate-rules/` already subsumes them. A lost gate is rebuilt, not
  documented away.
- [ ] **C.3** Either way, `.github/workflows/lint-test.yml` must not reference a
  path that does not exist.

## Phase D — Close `plan-cranky-to-lints-migration.md` Phase D

That plan's own verification gate was never run; its Phase D.1/D.2 are still
unticked and it carries an explicit "do not re-mark DONE" warning.

- [ ] **D.1** Run `cargo clippy --workspace --all-targets -- -D warnings`
  (note `--all-targets`, which the review pass's Gate A omitted) and iterate to
  clean. Tick D.1 **in that plan file**, not here.
- [ ] **D.2** Run `cargo check -p poly-lint-gate`, confirm rc=0, tick D.2 there.
- [ ] **D.3** Mark `plan-cranky-to-lints-migration.md` `## Status: ✅ DONE` only
  once both are actually ticked.

## Phase E — Verify (QA gate — iterate until a clean round)

- [ ] **E.1** `cargo check -p poly-lint-gate` rc=0. Compare the (path, rule,
  detail) multiset before/after: entries may be **removed** (real fixes) and
  entries may be **added only for sites Phase A newly discovered and Phase A.3
  chose to fix** — a net add of a grandfathered hang-class violation is a gate
  failure, not a result.
- [ ] **E.2** Deliberately reintroduce, on a scratch working copy, (a) a raw
  `handle.read().await` in `crates/core/src/ui/`, (b) the same in
  `mcp/chat-mcp/src/persona/`, and (c) a render-time `.read()` used as a hook
  key. All three must be caught. This is the only evidence that distinguishes
  "gate works" from "gate is green because it matches nothing".
- [ ] **E.3** Reflow an unrelated file in `crates/core/` by one line and confirm
  **no** allowlist or baseline entry desyncs — the Phase B acceptance test.
- [ ] **E.4** `cargo clippy --workspace --all-targets -- -D warnings` clean and
  `cargo test --workspace` green.
- [ ] **E.5** Re-run E.1–E.4 after the final fix; tick DONE only off a round that
  surfaced nothing new.
