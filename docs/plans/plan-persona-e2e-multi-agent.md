# Plan — Persona End-to-End Multi-Agent Bash Harness

## Status: 🚧 PLANNED — not started

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

### Phase A — Process-orchestration skeleton

- [ ] **A.1** Create `tests/e2e/` directory. Add `tests/e2e/README.md`
  explaining the harness and its prerequisites (cargo build, npm install
  playwright, `claude` CLI on PATH, optional `ANTHROPIC_API_KEY` for
  non-CI runs).
- [ ] **A.2** Write `tests/e2e/lib/process.sh` — small bash library with
  `spawn_bg`, `wait_for_port`, `wait_for_http_200`, `kill_pgrep_pattern`
  (matches the orphan-cleanup pattern from `mcp/electron-devtools-mcp`).
  All wait helpers cap at 60s per `feedback_wait_timeouts`.
- [ ] **A.3** Write `tests/e2e/lib/cleanup.sh` — `EXIT` trap that kills
  every process the script spawned, by PID file under
  `tests/e2e/.run/<run_id>/pids/`. Idempotent — re-running always safe.
- [ ] **A.4** Write `tests/e2e/persona-multi-agent.sh` skeleton: parses
  `--scenario <name>` flag, sources lib/, sets up `tests/e2e/.run/<run_id>/`,
  installs the EXIT trap, exits 0. No real work yet; just the harness
  scaffolding compiles + cleans up cleanly.
- [ ] **A.5** Add a temporary `POLY_DATA_DIR` per run so multiple e2e runs
  in CI don't trample one shared SQLite file.
  `export POLY_DATA_DIR="$RUN_ROOT/data"; mkdir -p "$POLY_DATA_DIR"`.

**Effort:** 0.5 sessions.

### Phase B — Boot the local stack

- [ ] **B.1** `start_test_backends` — fork `cargo run -p poly-test-runner
  -- --seed --quiet`; wait for `:9100..9107` to all return health 200.
  Cap at 60s; on timeout, dump tail of each log + exit 1.
- [ ] **B.2** `start_chat_mcp` — fork `cargo run -p poly-chat-mcp --
  --port 3010`; wait for `GET http://localhost:3010/health` 200.
  Pre-seed `POLY_DATA_DIR/storage.sqlite3` with a fixed account-id set so
  the personas have stable account IDs to bind to.
- [ ] **B.3** `start_poly_web` — fork the canonical `dx serve` invocation
  from CLAUDE.md (note the `@server --platform server` flag — required).
  Wait for `:3000` to respond.
- [ ] **B.4** Smoke check: `poly-cli --url http://localhost:3010/mcp tools`
  lists ≥14 `meta_persona_*` tools (the Phase B baseline; Phase J was
  rescoped to NOT add new tools, only the `dry_run` flag on
  `meta_persona_invoke`). Fail loud if fewer.
- [ ] **B.5** Persistent build cache — set `CARGO_TARGET_DIR=$RUN_ROOT/target`
  and pre-warm with `cargo build -p poly-chat-mcp -p poly-web -p
  poly-test-runner` at the top of the script so the wait_for_port windows
  measure boot, not compile time.

**Effort:** 0.5 sessions.

### Phase C — Spawn parallel `claude -p` persona agents

- [ ] **C.1** Generate per-persona `.mcp.json` template in
  `tests/e2e/scenarios/<name>/persona-<slug>.mcp.json`:
  ```json
  {
    "mcpServers": {
      "poly-chat": { "command": "/abs/path/poly-chat-mcp", "args": ["--stdio"] },
      "poly-memory": { "command": "/abs/path/poly-memory-mcp", "args": ["mcp"] }
    }
  }
  ```
  Note: the e2e script must point Claude Code at `--stdio`-mode chat-mcp
  bound to the SAME `POLY_DATA_DIR`, so all agents share the persona DB.
- [ ] **C.2** `spawn_persona_agent <slug> "<prompt>"` helper:
  ```
  claude -p "$prompt" \
    --mcp-config "$persona_config" \
    --output-format json \
    > "$RUN_ROOT/agents/$slug.out.json" 2>&1 &
  ```
  Each agent's prompt MUST start with the directive "Use the
  `meta_persona_invoke` tool with slug=$slug to gather context, then
  honour the persona's system prompt." This forces the agent through the
  persona surface rather than freelancing.
- [ ] **C.3** Pre-seed personas via `poly-cli`:
  for each scenario, the script calls `poly-cli call meta_persona_create
  …` and `meta_persona_set_sources …` before launching agents. Idempotent
  — checks `meta_persona_get` first.
- [ ] **C.4** Decide concurrency model: agents run sequentially within a
  scenario (deterministic), parallel only across distinct scenarios. The
  parallel-claude story is "Persona A finishes its work, Persona B runs,
  asserts what A left behind." Document the rationale in the README.
- [ ] **C.5** Capture the JSON tool-call trace from each agent (Claude
  Code's `--output-format json` includes tool calls); save to
  `$RUN_ROOT/agents/$slug.trace.json` for post-hoc analysis.

**Effort:** 1 session.

### Phase D — Playwright live-UI assertions

- [ ] **D.1** Add `tests/e2e/playwright.config.ts` and `tests/e2e/specs/`
  directory. Use the existing top-level `playwright` install (no new
  install needed — already in `node_modules/`).
- [ ] **D.2** Write `tests/e2e/specs/persona-live.spec.ts` — single
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
- [ ] **D.3** Implement `no_full_reload` assertion: spec injects a
  `window.__poly_e2e_load_count = (window.__poly_e2e_load_count||0)+1`
  on every full page load. Assertion fails if the counter increments
  during the scenario window.
- [ ] **D.4** Add `data-testid` attributes to the relevant components
  (`channel-row`, `message-row`, `draft-row`, `persona-row`) — this is a
  small UI patch in `crates/core/src/ui/` outside the persona/ directory
  Phase D owns. **Coordinate with Phase D's owner to avoid touching the
  same file simultaneously.**
- [ ] **D.5** Hook the spec into the bash script: after each agent's
  prompt completes, write the manifest, then `npx playwright test
  --config tests/e2e/playwright.config.ts`. Capture
  `playwright-report/` as part of the run artefacts.
- [ ] **D.6** Live-update assertion timing budget: between MCP tool call
  and DOM update, ≤ 5000 ms is "healthy", > 5000 ms ≤ 15000 ms is
  "degraded — warn", > 15000 ms is "broken — fail". Configurable via
  `E2E_LIVE_UPDATE_BUDGET_MS`.

**Effort:** 1 session.

### Phase E — Scenarios

- [ ] **E.1** `scenarios/two-personas-shared-channel/` — Persona A
  ("broker-bob") and Persona B ("greens-greg") both bind to
  `test-discord` channel `ch-shared`. A's agent sends a message via
  `send_message`. B's agent invokes its persona, asserts the new message
  appears in B's `meta_persona_invoke` bundle. Playwright asserts the
  message appears in the WASM UI within 5s.
- [ ] **E.2** `scenarios/fact-handoff/` — Persona A pins a fact via
  `meta_persona_set_memory` with `pinned=true`. Persona B (different
  slug, no source overlap) runs `meta_persona_get_memory` against A's
  slug — must return the fact (cross-persona reads are allowed by the
  current schema; this scenario captures the deliberate decision and
  fails loud if Phase G ever adds a per-persona ACL that breaks it).
- [ ] **E.3** `scenarios/mcp-to-ui-live-update/` — invoke
  `meta_persona_create` via `poly-cli`. Playwright asserts a new row
  appears in `PersonaListPanel` within 5s, with no page reload.
  This is the **headline regression test** — if this fails, the reactive
  chain is broken.
- [ ] **E.4** `scenarios/deny-wins-source-resolution/` — bind Persona A
  to `(account=test-discord, kind=server, value=guild-A, include=true)`
  AND `(account=test-discord, kind=channel, value=guild-A/ch-secret,
  include=false)`. Send a message to `ch-secret`. Persona A's
  `meta_persona_invoke` must NOT include that message in the bundle.
  Asserts deny-wins precedence at the e2e layer (unit tests cover it
  inside `persona/context.rs`; this catches integration regressions).
- [ ] **E.5** `scenarios/heartbeat-tick-via-mcp/` — depends on Phase F
  shipping a heartbeat trigger surface (the original Phase J.5
  `meta_persona_trigger_heartbeat` was descoped; expect Phase F to
  expose either an MCP tool or a `poly-cli` recipe for one-shot
  invocation). Set persona to `proactivity=drafts-only`, populate
  channel with 5 messages, fire the trigger. Assert exactly 1 draft
  appears in `DraftsSidebar` UI within 5s + 1 `heartbeat_run` audit
  row. If Phase F lands without a trigger surface, this scenario
  becomes the prompt to add one.
- [ ] **E.6** `scenarios/rate-limit-respected/` — set
  `rate_limit_per_hour=2`, trigger heartbeat 5 times back-to-back, assert
  exactly 2 audit rows of class `draft_create` and 3 of class
  `rate_limited`.
- [ ] **E.7** Each scenario directory contains: `scenario.sh` (sourced
  by the entry point), `personas.jsonl` (pre-seed input for `poly-cli`),
  `assertions.json.tmpl` (templated by the bash script before being fed
  to Playwright), `README.md` (one paragraph: "what regression does this
  catch?").

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

### Phase G — Cost / Anthropic-API guard

- [ ] **G.1** The bash script supports two modes:
  `--mode mock-claude` (default, for CI) — replaces `claude -p` with a
  small shell stub `tests/e2e/lib/mock-claude.sh` that reads the prompt,
  decides which MCP tools to call by pattern-matching, and exits. No
  real Claude API hit. The "intelligence" is hard-coded per scenario.
  `--mode real-claude` (opt-in, requires `ANTHROPIC_API_KEY`) —
  actually invokes `claude -p` for full E2E. Run nightly only, not on
  every PR.
- [ ] **G.2** Mock-claude stub vocabulary: per-scenario
  `mock-actions.jsonl` lists the exact `(prompt_substring, mcp_tool,
  args)` triples the stub fires. Keeps CI deterministic AND cheap.
- [ ] **G.3** Real-claude budget guard — `--mode real-claude` refuses
  to start if `--budget-tokens` not set; tracks cumulative tokens via
  the trace JSON; aborts with audit row if exceeded. Default budget
  100k tokens per run.
- [ ] **G.4** Document the mock-vs-real trade-off in `tests/e2e/README.md`.
  Mock catches integration glue + UI live-update regressions but not
  Claude-prompt-engineering regressions; real catches the latter at
  Anthropic-API cost.

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
