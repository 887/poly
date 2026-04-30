# Poly E2E Test Harness

End-to-end tests for Poly. The `discord/` subdirectory (HTTP-only mock tests,
no running WASM app) predates this harness. The persona multi-agent suite here
is the follow-up with a full UI-in-the-loop Playwright layer.

## Subdirectory layout

```
tests/e2e/
├── README.md                    ← you are here
├── persona-multi-agent.sh       ← single entry point for persona e2e suite
├── lib/
│   ├── process.sh               ← spawn_bg, wait_for_port, wait_for_http_200, kill_pgrep_pattern
│   └── cleanup.sh               ← EXIT trap that reaps all spawned processes
├── scenarios/                   ← one directory per scenario (Phase E)
│   └── <name>/
│       ├── scenario.sh          ← sourced by the entry point
│       ├── personas.jsonl       ← pre-seed input for poly-cli
│       ├── assertions.json.tmpl ← templated manifest for Playwright
│       └── README.md            ← one paragraph: "what regression does this catch?"
├── specs/                       ← Playwright specs (Phase D)
│   └── persona-live.spec.ts
├── playwright.config.ts         ← Playwright config (Phase D)
├── .run/                        ← per-run artefacts (gitignored)
│   └── <run_id>/
│       ├── pids/                ← *.pid files for every spawned process
│       ├── logs/                ← stdout+stderr for each process
│       ├── data/                ← isolated POLY_DATA_DIR (SQLite lives here)
│       └── results/             ← pass/fail JSON + JUnit XML (Phase F)
└── discord/                     ← pre-existing HTTP-only Discord mock tests
```

## Prerequisites

| Tool | Purpose |
|------|---------|
| Rust toolchain (`cargo`) | Build the mock backends and poly-chat-mcp |
| `dx` CLI (Dioxus) | Build + serve poly-web WASM |
| Node.js 18+ | Playwright test runner (Phase D+) |
| `npx playwright install chromium` | Browser binary (Phase D+) |
| `curl` | Health-check polling in the harness |
| `claude` CLI (optional) | `--mode real-claude` only; requires `ANTHROPIC_API_KEY` |

## Quick start

```bash
# Syntax check
bash -n tests/e2e/persona-multi-agent.sh

# Full boot + clean exit (noop scenario — no UI, no claude CLI required)
bash tests/e2e/persona-multi-agent.sh --scenario noop

# Verify cleanup left no orphans
pgrep -af "poly-test-|poly-chat-mcp|dx serve" | grep -v grep
# → should print nothing
```

## Stack overview

The harness boots three process groups:

### 1. Mock backends (`poly-test-runner`)

`poly-test-runner --seed` spawns all 8 mock backends:

| Backend | Port | Test animals |
|---------|------|-------------|
| `poly-test-matrix` | 9100 | Owl, Axolotl |
| `poly-test-stoat` | 9101 | Stoat, Raccoon |
| `poly-test-discord` | 9102 | Koala, Kangaroo |
| `poly-test-teams` | 9103 | Sheep, Walrus |
| `poly-test-lemmy` | 9104 | Beaver, Hedgehog |
| `poly-test-hackernews` | 9105 | (read-only feed) |
| `poly-test-forgejo` | 9106 | Otter, Flamingo |
| `poly-test-github` | 9107 | Penguin, Chameleon |

### 2. `poly-chat-mcp` (HTTP, port 3010)

The MCP server that exposes all 14+ `meta_persona_*` tools.  Both the bash
harness and the per-persona `claude -p` instances (Phase C) connect here.

Override the port: `MCP_PORT=3011 bash tests/e2e/persona-multi-agent.sh …`

### 3. `poly-web` via `dx serve --fullstack` (port 3000)

The WASM app.  Booted by the harness for all UI-asserting scenarios
(Phases D/E).  The `noop` scenario skips it to keep dry-runs fast.

Override the port: `WEB_PORT=3001 bash tests/e2e/persona-multi-agent.sh …`

The `@server --platform server` flag in the dx serve invocation is required —
without it, dx tries to build the server half for `wasm32-unknown-unknown`.

## Environment variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `MCP_PORT` | `3010` | Port for poly-chat-mcp HTTP server |
| `WEB_PORT` | `3000` | Port for poly-web dx serve |
| `POLY_DATA_DIR` | `tests/e2e/.run/<run_id>/data` | Isolated SQLite per run |
| `ANTHROPIC_API_KEY` | — | Required for `--mode real-claude` only |
| `E2E_LIVE_UPDATE_BUDGET_MS` | `5000` | Playwright live-update timing budget (Phase D) |

For `--mode real-claude`, pass `--budget-tokens N` on the command line (not an
env var) — the harness refuses to start without it.  See the
[Mock-claude vs real-claude](#mock-claude-vs-real-claude-trade-offs-and-cost-phase-g)
section for cost guidance.

## Build cache strategy (B.5)

The harness does NOT isolate `CARGO_TARGET_DIR` per run.  Per-run isolation
would cause 5-10 minute cold rebuilds on every run, exceeding the 15-minute CI
budget (Phase F).  Instead we use the shared workspace `target/` directory and
pre-warm it at the start of the script.

The only risk is parallel CI jobs racing on the same `target/`.  Mitigate at
the CI level by running the persona e2e job sequentially (not in parallel with
other jobs that write to `target/`).

## `poly-test-runner` flags used

| Flag | Effect |
|------|--------|
| `--seed` | Pre-populate demo data on startup (idempotent) |

Note: the plan's `--quiet` flag does NOT exist on `poly-test-runner`; the
binary has `--seed` and `--verbose` only (no quiet mode; default is not
verbose).

## `poly-chat-mcp` flags used

| Flag | Effect |
|------|--------|
| `--port <n>` | HTTP listen port (default 3010) |

`--port` is natively supported; no patching required.

## Adding a new scenario

1. Create `tests/e2e/scenarios/<name>/scenario.sh` (will be sourced by the
   entry point).
2. Add `personas.jsonl`, `assertions.json.tmpl`, and a one-paragraph `README.md`
   explaining what regression the scenario catches.
3. Test: `bash tests/e2e/persona-multi-agent.sh --scenario <name>`.

## Mock-claude vs real-claude: trade-offs and cost (Phase G)

The harness supports two modes selectable via `--mode`:

### `--mode mock-claude` (default — use for every PR)

Replaces `claude -p` with `tests/e2e/lib/mock-claude.sh`.  No Anthropic API
call is made; no `ANTHROPIC_API_KEY` is required.

Each scenario ships a `mock-actions.jsonl` file listing the exact
`(slug, tool, args, result_grep)` triples the stub fires.  The stub calls
those tools via `poly-cli` and asserts each response contains the expected
substring.

**What mock catches:**
- Integration glue: are the MCP tools wired to the right handlers?
- Scenario-level workflow: does agent A's write show up in agent B's read?
- UI live-update regressions (when combined with Phase D Playwright assertions):
  does the WASM reactive chain propagate MCP-driven state changes to the DOM?
- Deterministic regression: the same tool calls, same assertions, every run.

**What mock does NOT catch:**
- Claude prompt-engineering quality (does the LLM choose the right tool?).
- Tool-choice correctness under ambiguous user prompts.
- Latency-sensitive flows where Claude's planning latency matters.

**Cost:** ~$0 per run.

### `--mode real-claude` (opt-in — nightly CI or manual QA only)

Invokes `claude -p` with the real Anthropic API.  Requires:
1. `ANTHROPIC_API_KEY` set in the environment.
2. `--budget-tokens N` flag (hard requirement — the harness refuses to start
   without it, preventing runaway API cost from a misconfigured CI run).

**What real-claude adds:**
- Validates that Claude actually picks `meta_persona_invoke` unprompted.
- Catches prompt-engineering regressions when persona system prompts change.
- Exercises tool-choice correctness on the full Claude model surface.
- Validates latency-sensitive flows end-to-end.

**Budget guard (G.3):**
After each agent call, the harness parses `claude --output-format json`'s
`usage.input_tokens + usage.output_tokens` and accumulates the total.  If
the cumulative total reaches or exceeds `--budget-tokens`, the harness:
1. Kills all remaining processes via the EXIT trap.
2. Writes `tests/e2e/.run/<run_id>/results/budget-exceeded.json`.
3. Exits with code 2 (distinct from code 1 for scenario failures).

Running total is also persisted to `results/token-usage.json` after each
agent so post-mortem analysis is possible.

**Cost:** approximately $0.10–$1.00 per run, depending on scenario size and
model tier.  With `--budget-tokens 100000` the worst-case cost is roughly
$0.30 using Sonnet-tier pricing (input + output combined).

### Recommendation

| Context | Mode | Why |
|---------|------|-----|
| Every PR | `mock-claude` (default) | Zero cost, deterministic, fast |
| Nightly CI | `real-claude --budget-tokens 100000` | Catches prompt regressions |
| Local QA after persona prompt changes | `real-claude --budget-tokens 50000` | Quick sanity check |
| Cost estimation without running | Review `mock-actions.jsonl` | No API call needed |

Example nightly invocation:

```bash
bash tests/e2e/persona-multi-agent.sh \
  --scenario two-personas-handoff \
  --mode real-claude \
  --budget-tokens 100000
```

The harness exits non-zero (code 2) if the budget is exceeded mid-run, so
the CI job fails loudly and the `budget-exceeded.json` artefact explains why.

## Matrix-parallel upgrade path (Phase F.4)

The CI workflow (`.github/workflows/persona-e2e.yml`) currently runs all
scenarios **sequentially** in one job.  Sequential is correct as long as total
runtime stays under 15 minutes.

### When to switch to matrix-parallel

If `bash tests/e2e/persona-multi-agent.sh` for all 7 scenarios exceeds
~12 minutes on CI (leaving <3 minutes of headroom), switch to
`strategy: matrix`.

### How to switch

Replace the sequential scenario steps in `persona-e2e.yml` with:

```yaml
jobs:
  persona-e2e:
    strategy:
      fail-fast: false
      matrix:
        scenario:
          - two-personas-handoff
          - two-personas-shared-channel
          - fact-handoff
          - mcp-to-ui-live-update
          - deny-wins-source-resolution
          - heartbeat-tick-via-mcp
          - rate-limit-respected

    steps:
      # ... (checkout, toolchain, cache, build are the same) ...

      - name: Run scenario — ${{ matrix.scenario }}
        run: |
          bash tests/e2e/persona-multi-agent.sh \
            --scenario ${{ matrix.scenario }} \
            --mode mock-claude
```

Key considerations before switching:

1. **Port collisions** — each matrix job runs on its own runner, so ports
   (3010, 3000, 9100-9107) do not collide. No change needed.
2. **SQLite isolation** — each job writes to its own `POLY_DATA_DIR` (per
   run, not shared). Already correct.
3. **Cargo cache** — parallel jobs share the `actions/cache` key. If two
   jobs restore the same cache simultaneously, one will get a cache miss and
   rebuild from scratch. This is safe but may add ~3 min to one job.
   Workaround: pre-build in a separate `build` job and upload `target/` as
   an artefact; download it in each matrix job.
4. **JUnit aggregation** — update the `EnricoMi/publish-unit-test-result-action`
   step to run after all matrix jobs complete using `needs: [persona-e2e]`.
5. **Quarantine sticky comment** — move the "Collect quarantine list" and
   "Post sticky PR comment" steps to a separate `report` job that runs
   `needs: [persona-e2e]` with `if: always()`.

The `fail-fast: false` in the matrix ensures a flaky scenario doesn't cancel
other scenarios mid-run.

### Estimated parallel speedup

| Configuration | Estimated CI time |
|---|---|
| Sequential (current) | ~8-12 min (7 scenarios × ~90s each) |
| Matrix-parallel (7 jobs) | ~4-6 min (longest scenario + runner startup) |

Switch threshold: if sequential exceeds 12 minutes on two consecutive main
branch runs, open a PR to convert to matrix.

## Cleanup guarantee

An `EXIT` trap in `lib/cleanup.sh` reaps every process spawned by the harness,
even on SIGINT or error exit.  After the script exits, run:

```bash
pgrep -af "poly-test-|poly-chat-mcp|dx serve" | grep -v grep
```

The output should be empty.  If it is not, `lib/cleanup.sh`'s orphan-sweep
section needs updating (add the escaped pattern to the `patterns` array).
