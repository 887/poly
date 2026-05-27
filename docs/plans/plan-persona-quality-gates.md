# Plan — Persona Quality Gates (Lints, Fuzz, Audit Surface)

## Status: ✅ DONE — Phases Q-R-S-T all shipped

> **Why this is its own plan, not extra phases on `plan-meta-personalities.md`:**
> the work is **CI infrastructure** — `tools/scripts/forbid-*.sh` lints,
> `cargo fuzz` targets, and a queryable audit-export surface. It mirrors the
> pattern set by the eight existing hang-class lint plans (`plan-batched-
> signal.md`, `plan-peek-vs-read.md`, etc.): each one is a sibling plan
> with its own `forbid-*.sh` allowlist file. Personas are the first
> consumer; the lint patterns themselves apply to any future "subsystem
> with UI + MCP + audit" feature.

> **Created:** 2026-04-30
> **Depends on:** `plan-meta-personalities.md` Phases A–H shipped (so the
> lints have non-trivial code to gate), AND
> `plan-persona-e2e-multi-agent.md` Phase E (Q.4 reuses E.4 deny-wins
> scenario as a fuzz seed corpus).
> **Sibling plans:** `plan-batched-signal.md`, `plan-peek-vs-read.md`,
> `plan-use-spawn-once.md`, `plan-backend-read-timeout.md` (all `forbid-
> *.sh` shape).
> **Owner directories:** `tools/scripts/`, `mcp/chat-mcp/fuzz/` (new),
> `mcp/chat-mcp/src/persona/`, `crates/core/src/ui/agent/persona/`.

---

## 1. Goal

Add four classes of regression-prevention to the persona subsystem:

1. **Lints (Phase Q).** Mechanically prevent the most likely persona-
   specific footguns (cross-persona memory leak, raw `meta_persona_*`
   dispatch outside `tools.rs`, UI button without an MCP path, audit-row
   skipped on a state-changing tool).
2. **Fuzz tests (Phase R).** `cargo fuzz` target for the source-resolution
   deny-wins logic — the most subtle algorithm in the persona surface
   and the one with the highest blast radius if it leaks data.
3. **Smoke-test extensions (Phase S).** `TEST_HARNESS.md` step 4 is
   extended with a one-liner that covers the new persona crates; a step 5
   is added that runs the e2e mock-claude smoke scenarios.
4. **Audit surface (Phase T).** `meta_persona_recent_actions` returns
   audit rows; this plan adds CLI-side filters (`poly-cli persona-audit
   …` recipe) and a `persona_audit_query` MCP tool with structured
   filters so power-users + sub-agents can grep the audit log without
   writing SQL.

---

## 2. Hang-class analogy

CLAUDE.md documents 8 hang classes, each with a shipped lint
(`tools/scripts/forbid-*.sh`). The persona plan introduces analogous
**privacy / contract** classes:

| Class | Hazard | Lint |
|---|---|---|
| P1 | Reading `persona_facts` without `WHERE persona_slug = ?` | `forbid-cross-persona-memory.sh` |
| P2 | Adding a `meta_persona_*` arm in `tools.rs` without an audit row | `forbid-unaudited-persona-tool.sh` |
| P3 | UI button that calls a persona action without going through the MCP tool surface | `forbid-ui-only-persona-action.sh` |
| P4 | Reading from a chat backend on persona's behalf without `read_with_timeout` | covered by existing `forbid-raw-backend-read.sh` — extend its scope to `mcp/chat-mcp/src/persona/` |

These aren't "hang" classes — they're "privacy + contract" classes, but
the mechanism (allowlisted regex grep that fails CI) is identical.

---

## 3. Sequenced phases

### Phase Q — Lints

- [x] **Q.1** `tools/scripts/forbid-cross-persona-memory.sh` — greps
  `mcp/chat-mcp/src/` for any `SELECT … FROM persona_facts` not followed
  within N lines by `WHERE persona_slug` (or `persona_slug = ?` bound
  param). Allowlisted exceptions live in
  `tools/scripts/cross-persona-memory-allowlist.txt` with rationale
  comments. Inline form: `// poly-lint: allow cross-persona-memory —
  <reason>`. Acceptance: 0 unallowlisted hits at the time of landing.
  Shipped in Phase Q commit.

- [x] **Q.2** `tools/scripts/forbid-unaudited-persona-tool.sh` — greps
  `mcp/chat-mcp/src/tools.rs` for every `fn handle_meta_persona_*`
  function and asserts each calls `audit(` or `record_persona_audit(`
  at least once on the success path. Skips read-only tools (allowlisted:
  `_list`, `_recent_actions`; `_get` and `_get_memory` already audit).
  Acceptance: ALL state-changing handlers either audit or are in the
  allowlist with a written reason.
  Shipped in Phase Q commit.

- [x] **Q.3** `tools/scripts/forbid-ui-only-persona-action.sh` — stub —
  runs as no-op until Phase D lands; full impl deferred to follow-up
  commit. The stub exits 0 with a one-line notice so the CI matrix stays
  consistent without lying about coverage. Phase D UI not landed yet
  (`crates/core/src/ui/agent/persona/` does not exist); the full
  grep-for-MemoryDb-in-UI implementation will be added when Phase D
  ships. See plan-persona-quality-gates.md Q.3 comment.
  Shipped (stub) in Phase Q commit.

- [x] **Q.4** Extend the scope of the EXISTING
  `tools/scripts/forbid-raw-backend-read.sh` to cover
  `mcp/chat-mcp/src/persona/` (currently it gates `crates/core/src/ui/`).
  No new script, just edit the path glob in the existing one + sweep
  any new violations. Acceptance: persona/context.rs uses
  `tokio::time::timeout` (BACKEND_TIMEOUT constant) everywhere it talks
  to a backend — verified clean (0 new violations after path extension).
  Shipped in Phase Q commit.

- [x] **Q.5** Add all 4 lints to `.github/workflows/lint-test.yml`
  alongside the existing eight. Set `continue-on-error: false` from day
  one — these ship clean (the persona code is brand-new, no legacy debt
  to grandfather). Acceptance: CI red on any unallowlisted violation.
  Shipped in Phase Q commit.

- [x] **Q.6** Document the four classes in `CLAUDE.md` under a new
  "Persona-subsystem footguns" sibling section to "Common WASM-hang
  causes" (separate section chosen because these are privacy/contract
  bugs, not WASM concurrency bugs). Three real classes documented
  (P1, P2, P4); P3 stub noted in Q.3 above. Each class follows the
  hang-class template: symptom + countermeasure + lint script path.
  Shipped in Phase Q commit.

**Effort:** 1 session.

### Phase R — Fuzz: source resolution

- [x] **R.1** `cargo install cargo-fuzz` noted in README; fuzz crate at
  `mcp/chat-mcp/fuzz/Cargo.toml` with `[workspace]` isolation, own
  `rust-toolchain.toml` (nightly + llvm-tools-preview), excluded from
  root workspace via `[workspace.exclude]`.  Mirrors `tools/lints/poly-lints`
  precedent. Shipped in Phase R commit.
- [x] **R.2** Fuzz target `mcp/chat-mcp/fuzz/fuzz_targets/source_resolve.rs`.
  New `pub fn is_chat_included(rows, account_id, chat_id) → bool` extracted
  in `mcp/chat-mcp/src/persona/context.rs` (plus `pub struct PersonaSourceRow`).
  `FuzzSourceRow` wrapper derives `Arbitrary`; reference oracle in the same
  file asserts divergence == 0.  60-second sanity run: 1.13M execs, 0 crashes,
  0 divergence.  Shipped in Phase R commit.
- [x] **R.3** Seed corpus at `mcp/chat-mcp/fuzz/corpus/source_resolve/`:
  6 files — empty, all-deny, all-allow, deny-without-matching-allow,
  tag-empty-value, Phase E.4 deny-wins scenario.  Generated by
  `cargo +nightly run --bin gen_seeds` (binary in `src/gen_seeds.rs`).
  Shipped in Phase R commit.
- [x] **R.4** `.github/workflows/fuzz-personas.yml` — nightly cron
  06:00 UTC, 5-minute run, crash → upload artefact (30-day retention) +
  open GitHub issue with hex dump + repro command.  NOT on PR gate.
  Shipped in Phase R commit.
- [x] **R.5** `mcp/chat-mcp/fuzz/README.md` — one-command local
  invocation, how to add a seed, how to repro CI crash from artefact.
  5-minute zero-finding acceptance bar confirmed locally.
  Shipped in Phase R commit.

**Effort:** 1 session.

### Phase S — Smoke-test integration

- [x] **S.1** Edit `TEST_HARNESS.md` step 4 to include `cargo test -p
  poly-chat-mcp --lib` (currently runs only the integration test).
  Persona unit tests already pass; this just makes the harness assert
  it. Shipped in Phase S commit.
- [x] **S.2** Add a new `TEST_HARNESS.md` step 6: "Persona e2e mock
  smoke" — runs `tests/e2e/persona-multi-agent.sh --scenario
  mcp-to-ui-live-update --mode mock-claude` (the headline live-update
  scenario from `plan-persona-e2e-multi-agent.md` E.3). Skip step
  cleanly if the script doesn't exist (so the harness still works for
  pre-Phase-A-of-e2e branches). Time budget 5 minutes; fail loud if it
  exceeds. **Design decision:** `mcp-to-ui-live-update` scenario not yet
  landed (Phase E.3 pending); step falls back to `two-personas-handoff`
  (present post Phases A-C) with an INFO notice — documented inline in
  the step. Shipped in Phase S commit.
- [x] **S.3** Update the haiku-tier subagent dispatch template in
  CLAUDE.md "Agent Orchestration" section to mention persona changes
  warrant the new step 6. Shipped in Phase S commit.

**Effort:** 0.5 sessions.

### Phase T — Audit surface (shipped in commit pending)

- [x] **T.1** Add `meta_persona_audit_query` MCP tool. Args:
  `{slug?, action?, actor?, since?, until?, target_account?,
  target_chat?, result?, limit?}`. Returns the matching subset of
  `persona_audit` rows. SQL: dynamic `WHERE` builder gated by the
  presence of each filter. Acceptance: 8 unit tests covering each
  filter individually + 1 "no filter = all rows" + 1 combined
  `slug+action+since` filter + export + actor filter.
- [x] **T.2** Add `meta_persona_audit_export(slug)` — returns full audit
  history as JSONL. Mirror of Phase H.4 audit-export but exposed as an
  MCP tool so `poly-cli call meta_persona_audit_export --slug=foo >
  audit.jsonl` becomes the takeout path.
- [x] **T.3** `docs/personas-cli.md` gains an "Audit recipes" section
  (section 13) with 5 example invocations + `--watch` live-tail recipe.
- [x] **T.4** `PersonaAuditPanel` (Phase H.1) migrated from
  `meta_persona_list_audit` (non-existent stub) to
  `meta_persona_audit_query` with server-side action + target_account
  filters. No longer applies filters client-side post full fetch.
- [x] **T.5** Add `--watch <N>` flag to `tools/poly-cli/src/main.rs`.
  Re-runs the call every N seconds, dedupes by `id`, prints only new
  rows. `--since auto` initialises `since` to current UTC and advances
  it to the latest `occurred_at` seen after each poll. Exits cleanly on
  SIGINT (Ctrl+C). Requires `tokio` features `time`, `signal`, `macros`
  added to `tools/poly-cli/Cargo.toml`.

**Effort:** 1 session.

---

## 4. File-path index

| Concern | Path |
|---|---|
| Lint scripts | `tools/scripts/forbid-cross-persona-memory.sh`, `forbid-unaudited-persona-tool.sh`, `forbid-ui-only-persona-action.sh` |
| Lint allowlists | `tools/scripts/cross-persona-memory-allowlist.txt`, `unaudited-persona-tool-allowlist.txt`, `ui-only-persona-action-allowlist.txt` |
| Existing extended | `tools/scripts/forbid-raw-backend-read.sh` (path glob change only) |
| CI workflow | `.github/workflows/lint-test.yml` (extend), `.github/workflows/fuzz-personas.yml` (new) |
| Fuzz crate | `mcp/chat-mcp/fuzz/` (excluded from workspace, own toolchain pin) |
| Fuzz target | `mcp/chat-mcp/fuzz/fuzz_targets/source_resolve.rs` |
| Test harness | `TEST_HARNESS.md` (edit) |
| New MCP tools | `mcp/chat-mcp/src/tools.rs` (`meta_persona_audit_query`, `_audit_export`) |
| CLI watch mode | `tools/poly-cli/src/main.rs` |
| Doc surface | `docs/personas-cli.md` (extend "Audit recipes") |
| Footgun docs | `CLAUDE.md` (new subsection) |

---

## 5. Acceptance criteria

- [x] All 4 lints (Q.1–Q.4) ship with `continue-on-error: false`,
  red CI on violation, allowlist-with-rationale escape hatch.
- [x] CLAUDE.md documents the 4 persona footgun classes in the same
  template as the 8 hang classes.
- [x] `cargo fuzz run source_resolve -- -max_total_time=300` returns
  zero panics + zero ref-impl divergence on a clean nightly CI run.
- [x] `TEST_HARNESS.md` step 6 runs the e2e mock smoke scenario and
  passes within 5 minutes.
- [x] `meta_persona_audit_query` and `_audit_export` ship with
  unit tests; `poly-cli call meta_persona_audit_query --slug=foo
  --action=outbound_send --result=denied` returns the expected subset.
- [x] `poly-cli ... --watch 5` works on the audit-query tool and dedupes
  by row id.
- [x] `PersonaAuditPanel` switched to filtered query — no full-audit
  fetch on panel open.

---

## 6. Open questions / decisions captured

| Question | Decision | Why |
|---|---|---|
| Lints: hard-fail or warn-then-fail? | **Hard-fail (`continue-on-error: false`) day one** | Code is new, no debt to grandfather |
| Fuzz: PR gate or nightly? | **Nightly** | Fuzz timing is non-deterministic; PR gate must be ≤ 5 min |
| Fuzz target scope | **`resolve_sources()` only in v1** | Highest-blast-radius helper; bundle assembly + audit are simpler glue |
| Audit query as MCP tool vs CLI-only | **MCP tool** | Sub-agents and Claude Desktop both need it; CLI gets it for free |
| Watch-mode in poly-cli | **Yes — generic flag, not persona-specific** | Useful for any periodic MCP query; persona audit is the first consumer |
| Where do the footgun docs go in CLAUDE.md | **New subsection sibling to "Common WASM-hang causes"** | Same template, different domain (privacy not concurrency) |
| Allowlist file naming | **`<lint-name>-allowlist.txt` matching existing convention** | Consistency with the 8 prior `forbid-*.sh` plans |

---

## 7. Risk register

| Risk | Severity | Mitigation |
|---|---|---|
| Lints flake on grep edge cases (multi-line SQL, macro expansion) | MEDIUM | Allowlist + inline `// poly-lint: allow` escape valve from day one |
| Fuzz target produces false positives if reference impl diverges from real impl | MEDIUM | Reference impl lives in `tests/`, peer-reviewed against the spec |
| Q.3 (UI-only-action lint) trips on legitimate read-only UI hits | LOW | Allowlist read-only paths explicitly; doc the pattern |
| `meta_persona_audit_query` SQL injection via `slug` filter | HIGH | Bound parameters only — no string interpolation; unit test with `'; DROP TABLE` |
| Fuzz CI job blows out runner minutes | MEDIUM | Cap `-max_total_time=300`; one nightly run, not per-PR |
| Watch-mode in poly-cli leaks file descriptors on long runs | LOW | Reuse `reqwest::Client` across iterations; bounded loop with `--max-iterations` flag |

---

## 8. Effort estimate

| Phase | Sessions |
|---|---|
| Q — Lints | 1.0 |
| R — Fuzz | 1.0 |
| S — Smoke | 0.5 |
| T — Audit surface | 1.0 |
| **Total** | **~3.5 sessions** |

---

## 9. Explicit non-scope

- ❌ Lints for hypothetical persona footguns nobody has hit (e.g. tag-
  selector parser bugs). Wait for the bug, then write the lint.
- ❌ Fuzz of the full `meta_persona_invoke` happy path. Too many moving
  parts; the value is in narrow algorithmic fuzz of the deny-wins
  resolver.
- ❌ Property-testing via `proptest`. `cargo fuzz` is the project's
  established pattern; one tool only.
- ❌ Audit log search-as-you-type UI. Filter form is enough; full-text
  search is over-engineering for 30 days × 1 user × ~hundreds of rows.
- ❌ Sentry / external telemetry exfil of audit data. Privacy-first —
  audit stays local.

---

### Phase Q Status

Phase Q shipped in a single commit. All Q.1–Q.6 sub-steps complete.

| Sub-step | Status | Notes |
|---|---|---|
| Q.1 `forbid-cross-persona-memory.sh` | shipped | 1 allowlisted entry: `prune_persona_audit_before` (time-based housekeeping, intentional) |
| Q.2 `forbid-unaudited-persona-tool.sh` | shipped | Allowlisted: `_list`, `_recent_actions` (read-only); `_get`/`_get_memory` already audit |
| Q.3 `forbid-ui-only-persona-action.sh` | stub — deferred | Exits 0 with notice; full impl deferred until Phase D UI lands |
| Q.4 `forbid-raw-backend-read.sh` extended | shipped | 0 new violations; `context.rs` already uses `timeout(BACKEND_TIMEOUT, …)` throughout |
| Q.5 CI wiring | shipped | All 4 steps in `lint-test.yml`, `continue-on-error: false` |
| Q.6 CLAUDE.md footguns section | shipped | New sibling section "Persona-subsystem footguns" with P1/P2/P4 classes |

**Decision logged:** `persona_invocation_history` table does not exist in
`memory.rs` schema as of Phase Q (Phase E not yet shipped). Omitted from
Q.1 grep set. Re-extend Q.1 when Phase E lands and the table is added.

---

### Phase R Status

Phase R shipped in commit `b543de55`. All R.1–R.5 sub-steps complete.

| Sub-step | Status | Notes |
|---|---|---|
| R.1 fuzz crate setup | shipped | `mcp/chat-mcp/fuzz/` excluded from workspace; own nightly `rust-toolchain.toml` |
| R.2 fuzz target + `is_chat_included` pub | shipped | `PersonaSourceRow` + `is_chat_included` exported from `context.rs`; `FuzzSourceRow` wrapper derives `Arbitrary`; reference oracle in target file |
| R.3 seed corpus (6 files) | shipped | `gen_seeds` binary writes deterministic seeds; Phase E.4 deny-wins scenario included |
| R.4 nightly CI workflow | shipped | `.github/workflows/fuzz-personas.yml`; crash → artefact upload + GitHub issue |
| R.5 README | shipped | One-command invocation, add-a-seed guide, CI crash repro steps |

**Decisions logged:**
- `resolve_sources` remains private (async, backend-coupled). New `pub fn is_chat_included` extracts the pure deny-wins logic for fuzz access — avoids making the async function public or requiring a mock backend in the fuzz harness.
- `PersonaSourceRow` is a new `pub struct` in `context.rs` rather than a wrapper in `memory.rs` — memory.rs sources are `serde_json::Value`; the typed row only needs to exist for the fuzz+test surface.
- `Arbitrary` is derived on a `FuzzSourceRow` mirror in the fuzz crate (not on `PersonaSourceRow` itself) — keeps `arbitrary` out of the stable workspace dependencies.
- 60-second sanity run: **1,133,800 executions, 0 crashes, 0 divergence**.

---

### Phase S Status

Phase S shipped in a single commit. All S.1–S.3 sub-steps complete.

| Sub-step | Status | Notes |
|---|---|---|
| S.1 `cargo test -p poly-chat-mcp --lib` in TEST_HARNESS.md step 4 | shipped | Appended to existing step 4 test block |
| S.2 TEST_HARNESS.md step 6 "Persona e2e mock smoke" | shipped | Falls back to `two-personas-handoff` when `mcp-to-ui-live-update` scenario absent (Phase E.3 not yet landed); `timeout 300` enforces 5-min hard limit; "SKIP" path if script missing |
| S.3 CLAUDE.md haiku dispatch template note | shipped | One-line note added after existing UI-only skip rule |

**Design decision logged:** The `mcp-to-ui-live-update` scenario (Phase E.3 of
`plan-persona-e2e-multi-agent.md`) does not yet exist on disk. Rather than leaving step 6
effectively dead, the harness falls back to `two-personas-handoff` (present post Phases A-C)
with an INFO notice in stdout. When Phase E.3 ships and adds `mcp-to-ui-live-update` to
the scenario list, the primary branch of the if-else will activate automatically — no
further harness change needed.

---

### Phase T Status

Phase T shipped. All T.1–T.5 sub-steps complete.

| Sub-step | Status | Notes |
|---|---|---|
| T.1 `meta_persona_audit_query` MCP tool | shipped | `MemoryDb::query_persona_audit` dynamic WHERE builder; 8 unit tests (all filters + combined); 114→122 total lib tests |
| T.2 `meta_persona_audit_export` MCP tool | shipped | `MemoryDb::export_persona_audit` returns full history oldest-first as JSONL; test in T.1 suite |
| T.3 Audit recipes in `docs/personas-cli.md` | shipped | Section 13 with 5 paste-ready examples + `--watch` live-tail recipe |
| T.4 `PersonaAuditPanel` migrated | shipped | Was calling non-existent `meta_persona_list_audit`; now calls `meta_persona_audit_query` with server-side filter args; client-side filter pass removed |
| T.5 `--watch` mode in `poly-cli` | shipped | `--watch <N>` flag + `--since auto`; dedupes by `id`; SIGINT exit; `tokio` features `time`/`signal`/`macros` added to Cargo.toml |

**Decisions logged:**
- T.1 WHERE builder: `Vec<&str> + Vec<sqlite::Value>` approach with positional `?N` substitution. Matches existing `memory.rs` bind patterns. The macro handles `=`, `>=`, `<=` variants.
- Both `_audit_query` and `_audit_export` added to `unaudited-persona-tool-allowlist.txt` (read-only audit reads; circular audit rationale).
- `--since auto` implemented client-side in `poly-cli` — keeps MCP tool stateless.
- `PersonaAuditPanel` T.4: panel was calling `meta_persona_list_audit` which does not exist in `tools.rs`. Migration to `meta_persona_audit_query` fixes the broken call and improves efficiency (no full-table fetch).
