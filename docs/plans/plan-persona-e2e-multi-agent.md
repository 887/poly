# Plan — Persona End-to-End Multi-Agent Bash Harness

## Status: 🚧 IN PROGRESS — Phases A-C + G + D + E shipped; Phase F pending

> **Why this is its own plan, not Phase J on `plan-meta-personalities.md`:**
> the deliverable is a **reusable bash + Playwright harness** that drives
> `poly-test-runner` + `poly-chat-mcp` + `poly-web` + N parallel `claude -p`
> instances. Personas are the first consumer, but every future MCP-driven
> regression test (Phase Q lints, Discord-forum cross-account flows,
> meta-personality v2 council mode) reuses this harness. Putting it on the
> persona plan would cement it as persona-only.

> **Created:** 2026-04-30
> **Depends on:** `plan-meta-personalities.md` Phases A–F shipped, plus
> Phase J (MCP completeness) so every scenario can be expressed via
> `poly-cli` without resorting to raw JSON-RPC.
> **Sibling plans:** `plan-discord-e2e.md` (closed, HTTP-only mock) — this
> plan is the explicit follow-up to its punted "UI-level Playwright tests"
> bullet.
> **Owner directories:** `tests/e2e/` (new), `tools/poly-cli/`,
> `mcp/chat-mcp/tests/`.

---

## 1. Goal

Produce a **single bash entry point** (`tests/e2e/persona-multi-agent.sh`)
that:

1. Boots the full local mock stack: 8 `poly-test-*` mock backends via
   `poly-test-runner`, a `poly-chat-mcp` HTTP server on 3010, and `poly-web`
   (`dx serve --platform web --fullstack`) on 3000 with persistent SQLite
   storage.
2. Spawns N parallel `claude -p "use persona X to do Y"` invocations, each
   pointed at a `--mcp-config` that wires `poly-chat-mcp` (HTTP) +
   `poly-memory` MCP into Claude Code. The N agents act as different
   personas, exercising agent-to-agent interactions across backends.
3. Drives Playwright against `http://localhost:3000` to assert the WASM UI
   reflects MCP-driven state changes **live** (no page reload). This is the
   regression-prevention angle — it proves the reactive subscription chain
   from SQLite → backend events → `poll_events` → `app_state` → `rsx!`
   stays intact across persona-driven activity.
4. Returns a structured pass/fail report (JUnit XML or JSON) suitable for
   CI consumption.

Non-goal: replacing `mcp/chat-mcp/tests/persona_invoke_e2e.rs`. That Rust
integration test stays — it's faster + deterministic. This plan adds the
**multi-process, UI-in-the-loop** layer above it.

---

## 2. Architectural sketch

```
┌────────────────────────────────────────────────────────────┐
│ tests/e2e/persona-multi-agent.sh   (entry point)           │
│                                                            │
│  spawns ─► poly-test-runner  (mock matrix/discord/teams/…) │
│  spawns ─► poly-chat-mcp --port 3010 (HTTP)                │
│  spawns ─► dx serve --fullstack (poly-web on :3000)        │
│  spawns ─► N × claude -p "..." --mcp-config <persona-N.json>│
│  spawns ─► npx playwright test specs/persona-live.spec.ts  │
│  reaps  ─► all of the above on EXIT trap                   │
│                                                            │
│  emits   ─► tests/e2e/results/<timestamp>.{xml,json}       │
└────────────────────────────────────────────────────────────┘
```

Key design choices, captured up-front so phases don't re-litigate:

- **Bash, not pytest / cargo-nextest, for the orchestrator.** The harness
  is procedural process-management (start, wait-for-port, run, kill). Bash
  is the lingua franca for that and lets ops run it from anywhere with no
  extra runtime. Playwright spec is in TypeScript because that's
  Playwright's native language.
- **Each `claude -p` instance is one persona.** The script generates a
  per-persona `.mcp.json` that registers `poly-chat-mcp` (HTTP) and
  `poly-memory` (stdio) so the agent has the full persona surface.
- **Communication path Persona-A → Persona-B is via the chat backend, not
  some out-of-band channel.** Persona A calls `send_message` into
  `test-discord` channel `ch-shared`; Persona B's heartbeat or explicit
  `meta_persona_invoke` reads `ch-shared` via its own source binding and
  reacts. This mirrors how production personas interact.
- **Playwright assertions use refs, not screenshots.** Screenshot diffing
  is too brittle for a 50-message chat surface; instead, assert on DOM
  text content and ARIA roles via `page.locator(...).waitFor(...)`.
- **Live-update assertion is the load-bearing claim.** A successful run
  must show: MCP tool call returns at T0 → DOM text update visible at
  T0+N seconds where N ≤ 5 (configurable). If N > 5, the reactive chain
  is broken — fail loud. This is the regression-detector the plan exists
  for.

---

## 3. Sequenced phases

### Phase A — Process-orchestration skeleton (shipped in commit `<see-below>`)

- [x] **A.1** Create `tests/e2e/` directory. Add `tests/e2e/README.md`
  explaining the harness and its prerequisites (cargo build, npm install
  playwright, `claude` CLI on PATH, optional `ANTHROPIC_API_KEY` for
  non-CI runs).
- [x] **A.2** Write `tests/e2e/lib/process.sh` — small bash library with
  `spawn_bg`, `wait_for_port`, `wait_for_http_200`, `kill_pgrep_pattern`
  (matches the orphan-cleanup pattern from `mcp/electron-devtools-mcp`).
  All wait helpers cap at 60s per `feedback_wait_timeouts`.
- [x] **A.3** Write `tests/e2e/lib/cleanup.sh` — `EXIT` trap that kills
  every process the script spawned, by PID file under
  `tests/e2e/.run/<run_id>/pids/`. Idempotent — re-running always safe.
- [x] **A.4** Write `tests/e2e/persona-multi-agent.sh` skeleton: parses
  `--scenario <name>` flag, sources lib/, sets up `tests/e2e/.run/<run_id>/`,
  installs the EXIT trap, exits 0. No real work yet; just the harness
  scaffolding compiles + cleans up cleanly.
- [x] **A.5** Add a temporary `POLY_DATA_DIR` per run so multiple e2e runs
  in CI don't trample one shared SQLite file.
  `export POLY_DATA_DIR="$RUN_ROOT/data"; mkdir -p "$POLY_DATA_DIR"`.

**Effort:** 0.5 sessions.

### Phase B — Boot the local stack (shipped in commit `<see-below>`)

- [x] **B.1** `start_test_backends` — fork `cargo run -p poly-test-runner
  -- --seed` (note: `--quiet` flag does NOT exist on poly-test-runner;
  the binary has `--seed` and `--verbose` only); wait for `:9100..9107`
  to all return health 200. Cap at 60s; on timeout, dump tail of log + exit 1.
- [x] **B.2** `start_chat_mcp` — fork `cargo run -p poly-chat-mcp --
  --port 3010`; wait for `GET http://localhost:3010/health` 200.
  `--port` is natively supported by poly-chat-mcp; no patching needed.
- [x] **B.3** `start_poly_web` — fork the canonical `dx serve` invocation
  from CLAUDE.md (note the `@server --platform server` flag — required).
  Wait for `:3000` to respond. Skipped for `--scenario noop` to keep
  dry-runs fast.
- [x] **B.4** Smoke check: `poly-cli --url http://localhost:3010/mcp tools`
  lists ≥14 `meta_persona_*` tools (Phase B baseline; Phase J was rescoped
  to NOT add new tools, only the `dry_run` flag on `meta_persona_invoke`).
  Fail loud if fewer. Verified: 14 tools at runtime.
- [x] **B.5** Build cache strategy: we do NOT use per-run `CARGO_TARGET_DIR`.
  Per-run isolation would cause 5-10 min cold rebuilds exceeding the 15-min
  CI budget (Phase F). Use shared workspace `target/` with a pre-warm step
  at script start. Documented in `tests/e2e/README.md`.

**Effort:** 0.5 sessions.

### Phase C — Spawn parallel `claude -p` persona agents (shipped in commit `983616b8`)

- [x] **C.1** Generate per-persona `.mcp.json` template in
  `tests/e2e/scenarios/<name>/persona-<slug>.mcp.json`.
  Decision: stdio transport (not HTTP). Each `claude -p` gets its own
  `poly-chat-mcp --stdio` process (spawned by claude as per mcp.json).
  All share `POLY_DATA_DIR` → same SQLite → persona writes from agent A
  are visible to agent B. SQLite WAL handles concurrent readers safely.
  The `generate_persona_mcp_config` function in `persona-multi-agent.sh`
  writes the config; falls back to `cargo run` if binary not pre-built.
- [x] **C.2** `spawn_persona_agent <slug> "<prompt>" <mcp_config> <mock_actions>` helper in
  `persona-multi-agent.sh`. In real-claude mode invokes `claude -p`
  synchronously with `--mcp-config`/`--output-format json`/`--dangerously-skip-permissions`.
  In mock mode calls `run_mock_claude` from `lib/mock-claude.sh`.
  Prompt always starts with the mandatory `meta_persona_invoke` directive.
- [x] **C.3** Pre-seed personas via `poly-cli` in `seed_persona` helper.
  Idempotent: checks `meta_persona_get` first, only creates if not found.
  `seed_persona <slug> <name> <system_prompt> <sources_json>` call signature.
  Sources use correct `selector_kind`/`selector_value` schema (not `kind`/`value`).
- [x] **C.4** Concurrency model: agents run SEQUENTIALLY within a scenario.
  Rationale: single SQLite writer, deterministic assertion order (A writes,
  then B reads). Documented in `scenario.sh` comments.
- [x] **C.5** Per-agent `$RUN_ROOT/agents/$slug.out.json` + aggregation via
  `aggregate_agent_results` writing `$RESULTS_DIR/agents-summary.json`.
  Runs automatically after any scenario that produced `.out.json` files.
  Format: `{"total":N,"passed":N,"failed":N,"agents":[...]}`.

**Phase C also includes:**

- [x] **C.6** New scenario `tests/e2e/scenarios/two-personas-handoff/`.
  Two personas sharing the same channel; beta-receiver's mock call to
  `meta_persona_list` asserts alpha-sender is visible in the shared DB.
  In mock mode the "handoff" is DB-level (both personas in same SQLite);
  in real-claude mode the assertion greps for actual message content.
  Includes `personas.jsonl`, `mock-actions.jsonl`, `scenario.sh`, `README.md`.
- [x] **C.7** `two-personas-handoff` wired into the `case "$scenario"` dispatcher
  in `persona-multi-agent.sh`. Also added `NEEDS_POLY_WEB` opt-in flag
  so agent-only scenarios (no DOM assertions) skip the WASM build.
  Also added `lib/mock-claude.sh` sourced at startup.

**Effort:** 1 session.

### Phase D — Playwright live-UI assertions (shipped in commit `<see-below>`)

- [x] **D.1** Add `tests/e2e/playwright.config.ts` and `tests/e2e/specs/`
  directory. Use the existing top-level `playwright` install (no new
  install needed — already in `node_modules/`).
- [x] **D.2** Write `tests/e2e/specs/persona-live.spec.ts` — single
  parameterised spec that reads `process.env.E2E_SCENARIO_MANIFEST`
  (path to JSON written by the bash script) and runs the assertions
  declared therein. Manifest shape:
  ```json
  {
    "base_url": "http://localhost:3000",
    "assertions": [
      { "kind": "wait_for_text", "locator": "[data-testid=channel-ch-shared]",
        "text": "broker-bob: COIN beat", "timeout_ms": 5000 },
      { "kind": "wait_for_dom_count",
        "locator": "[data-testid=draft-row]", "count": 1, "timeout_ms": 8000 },
      { "kind": "no_full_reload",
        "since_ts": "<unix-ms before persona action>" }
    ]
  }
  ```
- [x] **D.3** Implement `no_full_reload` assertion: spec injects a
  `window.__poly_e2e_load_count = (window.__poly_e2e_load_count||0)+1`
  on every full page load. Assertion fails if the counter increments
  during the scenario window.
- [x] **D.4** Add `data-testid` attributes to the relevant components
  (`channel-row`, `message-row`, `draft-row`, `persona-row`) — patched
  `channel_list.rs` (ChannelItemRow + DMChannelItem), `chat_view.rs`
  (render_message_row), `draft_banner.rs` (DraftBannerRow + DraftsSidebarRow),
  `list_panel.rs` (PersonaListRow).
- [x] **D.5** Hook the spec into the bash script: `write_scenario_manifest`
  writes the JSON manifest; `run_playwright_assertions` invokes
  `npx playwright test --config tests/e2e/playwright.config.ts`. Report
  captured under `$RUN_ROOT/results/playwright-<scenario>/`.
- [x] **D.6** Live-update assertion timing budget: ≤ 5000 ms healthy,
  ≤ 15000 ms degraded (warn), > 15000 ms broken (fail). Configurable via
  `E2E_LIVE_UPDATE_BUDGET_MS`. Implemented in `assertWaitForText`,
  `assertWaitForDomCount`, `assertWaitForVisible` helpers.

**Effort:** 1 session.

### Phase E — Scenarios (shipped in commit `<see-below>`)

- [x] **E.1** `scenarios/two-personas-shared-channel/` — broker-bob + greens-greg
  both bind to `test-discord` ch-shared. Mock: both invoke personas via
  meta_persona_invoke dry_run; Playwright asserts both persona-row-* visible.
- [x] **E.2** `scenarios/fact-handoff/` — fact-alice pins a fact via
  `meta_persona_set_memory(pinned=true)`; fact-bob reads it via
  `meta_persona_get_memory(slug=fact-alice)`. Cross-persona reads deliberately
  allowed at v1; scenario fails loud if a future ACL breaks them.
- [x] **E.3** `scenarios/mcp-to-ui-live-update/` — `meta_persona_create` creates
  `live-probe-xyz`. Playwright asserts `data-testid="persona-row-live-probe-xyz"`
  appears within 5s + no_full_reload. Headline regression test for reactive chain.
- [x] **E.4** `scenarios/deny-wins-source-resolution/` — persona bound to
  guild-A server (include=true) + guild-A/ch-secret (include=false). Asserts
  deny-wins at source-binding level via persona source verification.
- [x] **E.5** `scenarios/heartbeat-tick-via-mcp/` — trigger surface used:
  `meta_persona_set_heartbeat` (minimum 60s per schema). Mock mode validates
  setup + invoke path; real heartbeat_run + draft_create audit rows validated
  in real-claude mode (nightly, 65s wait). Heartbeat audit action string:
  `"heartbeat_run"` (from `heartbeat.rs` `record_persona_audit` calls).
- [x] **E.6** `scenarios/rate-limit-respected/` — sets `rate_limit_per_hour=2`,
  verifies configuration via `meta_persona_get`. Full 5× heartbeat + 2
  draft_create + 3 rate_limited audit row assertion in real-claude mode.
- [x] **E.7** All 6 scenario directories contain: `scenario.sh`, `personas.jsonl`,
  `mock-actions.jsonl`, `assertions.json.tmpl`, `README.md`.

**Effort:** 1.5 sessions.

### Phase F — CI integration + reporting

- [ ] **F.1** Add `.github/workflows/persona-e2e.yml` (or extend an
  existing workflow) that runs the harness on a Linux runner. Time
  budget per run ≤ 15 minutes; if it grows, parallelise scenarios across
  jobs.
- [ ] **F.2** Output adapter — convert per-scenario pass/fail tables to
  JUnit XML (consumed by GitHub Actions test-summary plugin) AND to a
  human-readable Markdown summary posted as a sticky comment on PRs.
- [ ] **F.3** Flake quarantine — scenarios marked
  `# E2E_QUARANTINE: <reason>` in `scenario.sh` run but their failure
  doesn't fail the job. Used as the escape valve for known-flaky
  Playwright timing issues. Quarantine list reviewed weekly.
- [ ] **F.4** Local-dev convenience target: `make e2e-personas` (or
  `cargo make e2e-personas` if that crate's already in use) wraps the
  bash entry point with sane defaults.
- [ ] **F.5** Run-artefact retention — keep `tests/e2e/.run/<run_id>/`
  for the last 5 runs locally, all runs in CI artefacts (7 day retention).

**Effort:** 0.5 sessions.

### Phase G — Cost / Anthropic-API guard (shipped in commit `<see-below>`)

- [x] **G.1** The bash script supports two modes:
  `--mode mock-claude` (default, for CI) — replaces `claude -p` with a
  small shell stub `tests/e2e/lib/mock-claude.sh` that reads the prompt,
  decides which MCP tools to call by pattern-matching, and exits. No
  real Claude API hit. The "intelligence" is hard-coded per scenario.
  `--mode real-claude` (opt-in, requires `ANTHROPIC_API_KEY`) —
  actually invokes `claude -p` for full E2E. Run nightly only, not on
  every PR. Formalized: `--mode real-claude` without `ANTHROPIC_API_KEY`
  or without `--budget-tokens` both fail immediately with clear error + example.
- [x] **G.2** Mock-claude stub vocabulary: per-scenario
  `mock-actions.jsonl` lists the exact `(slug, tool, args, result_grep)`
  triples the stub fires. Format documented in `lib/mock-claude.sh` header.
  Canonical example: `scenarios/two-personas-handoff/mock-actions.jsonl` (4
  actions across 2 agents). All new scenarios must ship their own
  `mock-actions.jsonl`.
- [x] **G.3** Real-claude budget guard — `--mode real-claude` refuses
  to start if `--budget-tokens` not set (exits code 1 with actionable
  error). After each agent call, parses `claude --output-format json`
  `usage.input_tokens + usage.output_tokens`, accumulates into
  `_TOKENS_USED`. If `_TOKENS_USED >= BUDGET_TOKENS`: kills all processes
  via EXIT trap, writes `$RESULTS_DIR/budget-exceeded.json`, exits code 2.
  Running total persisted to `$RESULTS_DIR/token-usage.json` after each
  agent. Default budget: 100k tokens per run (documented in README + flag
  error message).
- [x] **G.4** Documented mock-vs-real trade-off in `tests/e2e/README.md`
  under "Mock-claude vs real-claude: trade-offs and cost (Phase G)" section.
  Covers: what each mode catches, cost (~$0 mock / ~$0.10–$1 real), and
  the recommendation table (mock every PR; real nightly with budget cap).

**Effort:** 0.5 sessions.

---

## 4. File-path index

| Concern | Path |
|---|---|
| Entry point | `tests/e2e/persona-multi-agent.sh` |
| Bash libraries | `tests/e2e/lib/{process,cleanup,mock-claude}.sh` |
| Scenario dirs | `tests/e2e/scenarios/<name>/{scenario.sh,personas.jsonl,assertions.json.tmpl,README.md}` |
| Playwright config | `tests/e2e/playwright.config.ts` |
| Playwright spec | `tests/e2e/specs/persona-live.spec.ts` |
| CI workflow | `.github/workflows/persona-e2e.yml` |
| Artefacts | `tests/e2e/.run/<run_id>/{pids,agents,data,target,playwright-report,results}/` |
| Pre-existing Rust e2e | `mcp/chat-mcp/tests/persona_invoke_e2e.rs` (untouched) |

---

## 5. Acceptance criteria

- [ ] `tests/e2e/persona-multi-agent.sh --scenario two-personas-shared-channel
  --mode mock-claude` exits 0 from a clean checkout in ≤ 5 minutes.
- [ ] All 6 scenarios (E.1–E.6) pass mock-claude mode in CI.
- [ ] `--mode real-claude` works locally with a paid `ANTHROPIC_API_KEY`
  for at least scenario E.3 (UI live-update).
- [ ] `data-testid` attributes added to the 4 named components without
  regressing component-lint or component-size budgets.
- [ ] Cleanup trap leaves zero stray `poly-test-*`, `poly-chat-mcp`,
  `dx serve`, or `claude` processes after exit (verified by `pgrep -f`).
- [ ] CI run posts a sticky PR comment with per-scenario pass/fail and
  live-update timing histogram.

---

## 6. Open questions / decisions captured

| Question | Decision | Why |
|---|---|---|
| Bash vs pytest vs Rust harness | **Bash** | Process-orchestration is bash's home turf; reusable for non-persona MCP work later |
| Mock-claude default vs real-claude | **Mock for CI, real for nightly opt-in** | Cost + determinism for PR gating |
| Parallel personas vs sequential | **Sequential within scenario, parallel across scenarios** | Avoids race-condition flakes on shared SQLite |
| Where do `data-testid` attrs go | **In existing components, separate small commits** | Coordinate with Phase D agent to avoid file collisions |
| Live-update timing budget | **5s healthy / 15s degraded / >15s fail** | 5s matches the user's mental "live" threshold; 15s matches `poll_events` tick + scheduler slop |
| `claude -p` is the right CLI flag | **Yes — `--mcp-config` + `--output-format json` give us config injection + tool-call trace** | Documented Claude Code surface today |
| `dry_run=true` on invoke | **Use it for the bundle-shape sanity check in B.4** | Avoids audit-log noise on every CI run |

---

## 7. Risk register

| Risk | Severity | Mitigation |
|---|---|---|
| Playwright flake from WASM render-tick variance | HIGH | Use locator `waitFor` not `expect.toBeVisible` w/o timeout; quarantine path in F.3 |
| `dx serve` startup taking > 60s | MEDIUM | Pre-warm cargo cache (B.5); raise wait_for_port budget to 120s for poly-web specifically |
| Claude Code CLI evolves and breaks `--mcp-config` | LOW | Mock-claude path keeps CI green even if real-claude breaks; nightly job catches the regression |
| Parallel test runs collide on port 3010 | MEDIUM | Per-run dynamic port allocation (`MCP_PORT=$(find_free_port)`) before B.2 |
| SQLite contention between in-process chat-mcp and stdio claude-side chat-mcp | MEDIUM | All persona-write agents go through ONE `poly-chat-mcp` HTTP instance; UI talks to it too. Stdio mode for claude only when scenarios specifically test stdio dispatch. |
| Anthropic API cost explosion in real-claude mode | HIGH | G.3 budget guard; nightly cron not per-PR |
| Persona test data leaks into developer SQLite | LOW | A.5 isolated `POLY_DATA_DIR` per run |
| `data-testid` attrs collide with Phase D commits | MEDIUM | Coordinate ownership in stand-up before Phase D.x ships; rebase strategy documented |

---

## 8. Effort estimate

| Phase | Sessions |
|---|---|
| A — Skeleton | 0.5 |
| B — Boot stack | 0.5 |
| C — Persona agents | 1.0 |
| D — Playwright | 1.0 |
| E — 6 scenarios | 1.5 |
| F — CI integration | 0.5 |
| G — Cost guard | 0.5 |
| **Total** | **~5.5 sessions** |

---

## 9. Explicit non-scope

- ❌ Replacing the Rust integration test
  `mcp/chat-mcp/tests/persona_invoke_e2e.rs`. Both layers stay.
- ❌ Real OAuth flows against real Discord/Matrix/Teams. Mock backends
  via `poly-test-runner` are the contract.
- ❌ Cross-shell coverage (Wry, Electron). poly-web only in v1; the
  reactive chain is the same code path.
- ❌ Visual regression / screenshot diffing. DOM-text + ARIA assertions
  only.
- ❌ Load testing (10+ personas, 1k+ messages). Single-digit personas
  exercising correctness, not performance.
- ❌ Claude Code prompt-engineering regression detection in mock mode.
  That's what the opt-in real-claude nightly is for.

---

### Phase A Status

Shipped in this worktree commit. Files added:
- `tests/e2e/lib/process.sh` — `spawn_bg`, `wait_for_port`, `wait_for_http_200`, `kill_pgrep_pattern`, `find_free_port`
- `tests/e2e/lib/cleanup.sh` — `install_cleanup_trap`, `_cleanup_handler`, `_cleanup_by_pid_dir`, `_cleanup_orphan_sweeps`
- `tests/e2e/persona-multi-agent.sh` — entry point with `--scenario` / `--mode` / `--noop` flags, per-run `RUN_ROOT`, `PIDS_DIR`, `POLY_DATA_DIR` isolation, EXIT trap
- `tests/e2e/README.md` — prerequisites, stack overview, env vars, build-cache rationale, how to add scenarios

All wait helpers cap at 60 s per `feedback_wait_timeouts`.

### Phase B Status

Shipped in same commit. Key findings vs plan text:

- **`poly-test-runner --quiet` does not exist.** The binary (in `servers/test-runner/src/main.rs`) supports `--seed` and `--verbose` only. The plan's `--seed --quiet` invocation would fail. Fixed: use `--seed` only.
- **`poly-chat-mcp --port` works natively.** No patching needed (clap arg `--port <u16>`, default 3010).
- **B.4 tool count: 14 confirmed.** `poly-cli tools` lists exactly 14 `meta_persona_*` tools at runtime. The harness checks `≥14`.
- **B.5 build cache: shared, not per-run.** Per-run `CARGO_TARGET_DIR` would cold-rebuild on every CI run (~5-10 min), breaking the 15-min CI budget. Decision: shared workspace `target/` with pre-warm. Documented in `tests/e2e/README.md`.
- **B.3 poly-web skipped for noop.** `--scenario noop` skips `start_poly_web` so the dry-run completes without a WASM build. Scenarios that need the UI must call `start_poly_web` explicitly (or the harness detects via scenario metadata in Phase C/D).

Acceptance verified: `bash tests/e2e/persona-multi-agent.sh --scenario noop` exits 0, prints "Smoke check: 14 meta_persona_* tools available ✓", and `pgrep -af "poly-test-|poly-chat-mcp|dx serve"` returns empty after exit.

### Phase C Status

Shipped in this worktree commit. Files added/modified:

- `tests/e2e/lib/mock-claude.sh` — NEW. `run_mock_claude <slug> <actions> <mcp_url> <out_json>` stub that replays `mock-actions.jsonl` via `poly-cli` and writes a synthetic `--output-format json`-shaped result. No ANTHROPIC_API_KEY required.
- `tests/e2e/persona-multi-agent.sh` — MODIFIED. Added: `source lib/mock-claude.sh`, `AGENTS_DIR`, `generate_persona_mcp_config`, `spawn_persona_agent`, `seed_persona`, `aggregate_agent_results`, `NEEDS_POLY_WEB` opt-in flag, `two-personas-handoff` case in dispatcher.
- `tests/e2e/scenarios/two-personas-handoff/scenario.sh` — NEW. Defines `run_scenario_two_personas_handoff` that seeds personas, runs agents sequentially, and asserts shared DB visibility.
- `tests/e2e/scenarios/two-personas-handoff/personas.jsonl` — NEW. Seed data for alpha-sender and beta-receiver with correct `selector_kind`/`selector_value` schema.
- `tests/e2e/scenarios/two-personas-handoff/mock-actions.jsonl` — NEW. 4 deterministic tool calls (2 per agent).
- `tests/e2e/scenarios/two-personas-handoff/README.md` — NEW.

Key decisions vs plan:

- **stdio transport chosen.** `poly-chat-mcp --stdio` is supported natively (confirmed in `mcp/chat-mcp/src/main.rs`). Each `claude -p` gets its own stdio process; all share POLY_DATA_DIR SQLite. HTTP transport also works (already running on 3010) but stdio avoids needing an http-to-mcp bridge.
- **`NEEDS_POLY_WEB` opt-in.** Rather than defaulting poly-web for all non-noop scenarios, scenarios that need DOM assertions set `NEEDS_POLY_WEB=true`. Agent-only scenarios (C.6) skip the WASM build.
- **`meta_persona_set_sources` uses `selector_kind`/`selector_value`**, not `kind`/`value`. Corrected in personas.jsonl after first run failure.
- **Mock mode asserts DB-level handoff** (both personas visible in shared SQLite via `meta_persona_list`). Real-claude mode would additionally grep message content. This split is documented in `scenario.sh`.
- **shellcheck not installed** in this environment — bash -n verified instead.

Acceptance verified: `bash tests/e2e/persona-multi-agent.sh --scenario two-personas-handoff` exits 0, both agents PASS (2 tool calls each), `pgrep -af "poly-test-|poly-chat-mcp|dx serve|claude -p"` returns empty after exit.

### Phase D Status

Shipped in commit `<see-below>`. Files added/modified:

- `tests/e2e/playwright.config.ts` — NEW. Separate config for persona live-UI spec. Uses `E2E_WEB_BASE_URL`, `E2E_SCENARIO_MANIFEST`, `E2E_LIVE_UPDATE_BUDGET_MS` env vars. Single `persona-live` project with headless Chromium.
- `tests/e2e/specs/persona-live.spec.ts` — NEW. Parametrised spec reads `E2E_SCENARIO_MANIFEST` JSON and runs `wait_for_text`, `wait_for_dom_count`, `wait_for_visible`, `no_full_reload` assertion kinds. Assertion dispatcher factored directly in spec (no separate helper file needed). D.6 timing budget: healthy ≤5s warn ≤15s fail >15s.
- `tests/e2e/persona-multi-agent.sh` — MODIFIED. Added `write_scenario_manifest` (SCENARIO_ASSERTIONS bash array → JSON), `run_playwright_assertions` (invokes npx playwright), `NEEDS_POLY_WEB` detection for new D+E scenarios.
- `crates/core/src/ui/account/common/channel_list.rs` — MODIFIED. Added `data-testid="channel-row-{channel_id}"` to `ChannelItemRow` and `DMChannelItem`.
- `crates/core/src/ui/account/common/chat_view.rs` — MODIFIED. Added `data-testid="message-row-{msg_id}"` to `render_message_row`.
- `crates/core/src/ui/account/common/draft_banner.rs` — MODIFIED. Added `data-testid="draft-row-{draft_id}"` to `DraftBannerRow` and `DraftsSidebarRow`.
- `crates/core/src/ui/agent/persona/list_panel.rs` — MODIFIED. Added `data-testid="persona-row-{persona.slug}"` to `PersonaListRow`.

No raw `Signal::write()` added; no `use_effect` with stale captures. Lints pass for the changed files.

### Phase E Status

Shipped in commit `<see-below>`. 6 scenario directories added:

- `tests/e2e/scenarios/two-personas-shared-channel/` — E.1. broker-bob + greens-greg, ch-shared, Playwright asserts both persona rows visible.
- `tests/e2e/scenarios/fact-handoff/` — E.2. Cross-persona memory read (fact-alice → fact-bob). Documents v1 cross-persona read allowance.
- `tests/e2e/scenarios/mcp-to-ui-live-update/` — E.3. HEADLINE TEST. `meta_persona_create` → `persona-row-live-probe-xyz` visible in DOM within 5s.
- `tests/e2e/scenarios/deny-wins-source-resolution/` — E.4. guild-A server allow + ch-secret channel deny. Asserts deny-wins at source-binding level.
- `tests/e2e/scenarios/heartbeat-tick-via-mcp/` — E.5. Heartbeat surface: `meta_persona_set_heartbeat` (60s min). Mock validates setup; real-claude nightly validates `heartbeat_run` audit row. Audit action strings: `"heartbeat_run"`, `"draft_create"`, `"rate_limited"` (from `heartbeat.rs`).
- `tests/e2e/scenarios/rate-limit-respected/` — E.6. `rate_limit_per_hour=2` stored; 5× back-to-back heartbeat + audit row count in real-claude mode.

Each directory has: `scenario.sh`, `personas.jsonl`, `mock-actions.jsonl`, `assertions.json.tmpl`, `README.md`. All bash scripts pass `bash -n` syntax check. Playwright `--list` returns 1 test (stub mode, no manifest). All 6 scenarios runnable in mock-claude mode (no ANTHROPIC_API_KEY needed) with `cargo` + `poly-cli` as the tool invocation layer.
