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

## Cleanup guarantee

An `EXIT` trap in `lib/cleanup.sh` reaps every process spawned by the harness,
even on SIGINT or error exit.  After the script exits, run:

```bash
pgrep -af "poly-test-|poly-chat-mcp|dx serve" | grep -v grep
```

The output should be empty.  If it is not, `lib/cleanup.sh`'s orphan-sweep
section needs updating (add the escaped pattern to the `patterns` array).
