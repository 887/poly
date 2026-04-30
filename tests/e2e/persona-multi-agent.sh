#!/usr/bin/env bash
# tests/e2e/persona-multi-agent.sh — Poly persona end-to-end test harness.
#
# Boots the full local mock stack:
#   • 8 poly-test-* backends via poly-test-runner    (ports 9100-9107)
#   • poly-chat-mcp HTTP server                      (port 3010, or $MCP_PORT)
#   • poly-web via dx serve --fullstack              (port 3000, or $WEB_PORT)
#
# Then runs the requested --scenario (Phase C+). Cleans up everything on exit.
#
# Usage:
#   bash tests/e2e/persona-multi-agent.sh --scenario noop
#   bash tests/e2e/persona-multi-agent.sh --scenario <name> [--mode mock-claude|real-claude]
#
# Prerequisites:
#   • Rust toolchain (cargo, dx CLI)
#   • curl (for health checks)
#   • cargo build pre-warm recommended (B.5 handles it)
#   • For real-claude mode: claude CLI on PATH + ANTHROPIC_API_KEY set

set -euo pipefail

# ---------------------------------------------------------------------------
# Resolve absolute paths
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# ---------------------------------------------------------------------------
# Source library files
# ---------------------------------------------------------------------------
# shellcheck source=tests/e2e/lib/process.sh
source "$SCRIPT_DIR/lib/process.sh"
# shellcheck source=tests/e2e/lib/cleanup.sh
source "$SCRIPT_DIR/lib/cleanup.sh"
# shellcheck source=tests/e2e/lib/mock-claude.sh
source "$SCRIPT_DIR/lib/mock-claude.sh"

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
SCENARIO=""
MODE="mock-claude"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --scenario)
            SCENARIO="$2"
            shift 2
            ;;
        --mode)
            MODE="$2"
            shift 2
            ;;
        --noop)
            # Convenience alias for --scenario noop
            SCENARIO="noop"
            shift
            ;;
        -h|--help)
            grep '^#' "$0" | grep -v '^#!/' | sed 's/^# \?//'
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 1
            ;;
    esac
done

if [[ -z "$SCENARIO" ]]; then
    echo "ERROR: --scenario <name> is required" >&2
    echo "Try: $0 --scenario noop" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Per-run isolation (A.4, A.5)
# ---------------------------------------------------------------------------
RUN_ID="$(date +%Y%m%dT%H%M%S)-$$"
RUN_ROOT="$SCRIPT_DIR/.run/$RUN_ID"
PIDS_DIR="$RUN_ROOT/pids"
LOGS_DIR="$RUN_ROOT/logs"
DATA_DIR="$RUN_ROOT/data"
RESULTS_DIR="$RUN_ROOT/results"

AGENTS_DIR="$RUN_ROOT/agents"

mkdir -p "$PIDS_DIR" "$LOGS_DIR" "$DATA_DIR" "$RESULTS_DIR" "$AGENTS_DIR"

# A.5 — isolated SQLite per run
export POLY_DATA_DIR="$DATA_DIR"

# ---------------------------------------------------------------------------
# Install EXIT trap (must come AFTER PIDS_DIR is set)
# ---------------------------------------------------------------------------
install_cleanup_trap

# ---------------------------------------------------------------------------
# Port configuration
# ---------------------------------------------------------------------------
# Allow overrides via environment; fall back to well-known defaults.
E2E_MCP_PORT="${MCP_PORT:-3010}"
E2E_WEB_PORT="${WEB_PORT:-3000}"
export E2E_WEB_PORT  # consumed by _cleanup_orphan_sweeps in cleanup.sh

echo "============================================================"
echo " Poly e2e harness — run $RUN_ID"
echo "   scenario: $SCENARIO"
echo "   mode:     $MODE"
echo "   RUN_ROOT: $RUN_ROOT"
echo "   DATA_DIR: $POLY_DATA_DIR"
echo "   MCP port: $E2E_MCP_PORT"
echo "   Web port: $E2E_WEB_PORT"
echo "============================================================"

# ---------------------------------------------------------------------------
# B.5 — Pre-warm cargo build cache
#
# Decision: we do NOT set CARGO_TARGET_DIR to a per-run path.  Rebuilding from
# a cold cache adds 5-10 minutes per run, which defeats the 15-minute CI budget
# (Phase F). Instead we use the workspace's shared target/ directory.  The only
# risk is parallel CI jobs racing; mitigate by serialising e2e at the CI level
# (one job, separate runners for unit tests).  Per-run isolation for actual
# runtime artefacts (SQLite, PID files, logs) is achieved via POLY_DATA_DIR.
# ---------------------------------------------------------------------------
echo ""
echo "[B.5] Pre-warming cargo build …"
cargo build \
    --quiet \
    -p poly-test-runner \
    -p poly-chat-mcp \
    2>&1 | tee "$LOGS_DIR/cargo-build-prewarm.log"
echo "[B.5] Cargo pre-warm complete"

# ---------------------------------------------------------------------------
# B.1 — Start all 8 test backends via poly-test-runner
# ---------------------------------------------------------------------------
start_test_backends() {
    echo ""
    echo "[B.1] Starting poly-test-runner (all 8 backends, ports 9100-9107) …"
    spawn_bg \
        "$PIDS_DIR/poly-test-runner.pid" \
        "$LOGS_DIR/poly-test-runner.log" \
        cargo run --quiet -p poly-test-runner -- --seed

    # poly-test-runner takes time to compile/start the 8 child servers.
    # Wait for ALL 8 /health endpoints.  Cap at 60 s per backend.
    local -a backend_ports=(9100 9101 9102 9103 9104 9105 9106 9107)
    local -a backend_names=(matrix stoat discord teams lemmy hackernews forgejo github)
    local all_healthy=true

    for i in "${!backend_ports[@]}"; do
        local port="${backend_ports[$i]}"
        local name="${backend_names[$i]}"
        if ! wait_for_http_200 "http://127.0.0.1:${port}/health" 60; then
            echo "[B.1] ERROR: ${name} (port ${port}) did not become healthy within 60s" >&2
            echo "[B.1] --- tail of poly-test-runner.log ---" >&2
            tail -20 "$LOGS_DIR/poly-test-runner.log" >&2
            all_healthy=false
        fi
    done

    if [[ "$all_healthy" != true ]]; then
        echo "[B.1] FATAL: one or more backends failed to start" >&2
        exit 1
    fi
    echo "[B.1] All 8 backends healthy"
}

# ---------------------------------------------------------------------------
# B.2 — Start poly-chat-mcp HTTP server
# ---------------------------------------------------------------------------
start_chat_mcp() {
    echo ""
    echo "[B.2] Starting poly-chat-mcp on port ${E2E_MCP_PORT} …"
    spawn_bg \
        "$PIDS_DIR/poly-chat-mcp.pid" \
        "$LOGS_DIR/poly-chat-mcp.log" \
        cargo run --quiet -p poly-chat-mcp -- --port "$E2E_MCP_PORT"

    if ! wait_for_http_200 "http://127.0.0.1:${E2E_MCP_PORT}/health" 60; then
        echo "[B.2] ERROR: poly-chat-mcp did not become healthy within 60s" >&2
        echo "[B.2] --- tail of poly-chat-mcp.log ---" >&2
        tail -20 "$LOGS_DIR/poly-chat-mcp.log" >&2
        exit 1
    fi
    echo "[B.2] poly-chat-mcp healthy on port ${E2E_MCP_PORT}"
}

# ---------------------------------------------------------------------------
# B.3 — Start poly-web via dx serve --fullstack
#
# Uses the EXACT invocation from CLAUDE.md "Running apps/web with persistent
# storage" section — the @server --platform server flag is load-bearing.
# ---------------------------------------------------------------------------
start_poly_web() {
    echo ""
    echo "[B.3] Starting poly-web on port ${E2E_WEB_PORT} …"

    # dx serve must be run from apps/web (it reads its Cargo.toml)
    local apps_web_dir="$REPO_ROOT/apps/web"

    spawn_bg \
        "$PIDS_DIR/poly-web.pid" \
        "$LOGS_DIR/poly-web.log" \
        bash -c "cd '$apps_web_dir' && dx serve \
            --platform web \
            --fullstack \
            --port '$E2E_WEB_PORT' \
            @client --no-default-features --features 'dev-plugins,web' \
            @server --platform server --no-default-features --features 'dev-plugins,server'"

    # poly-web may take up to 120 s (cold WASM build) — plan notes this is
    # acceptable given B.5 pre-warms the cache.  We cap our individual poll
    # at 60 s per the wait_for_http_200 contract but call it twice if needed
    # for a first-run cold start scenario.
    if ! wait_for_http_200 "http://127.0.0.1:${E2E_WEB_PORT}/" 60; then
        echo "[B.3] Still waiting for poly-web (cold build?) — extending 60s …"
        if ! wait_for_http_200 "http://127.0.0.1:${E2E_WEB_PORT}/" 60; then
            echo "[B.3] ERROR: poly-web did not start within 120s" >&2
            echo "[B.3] --- tail of poly-web.log ---" >&2
            tail -30 "$LOGS_DIR/poly-web.log" >&2
            exit 1
        fi
    fi
    echo "[B.3] poly-web healthy on port ${E2E_WEB_PORT}"
}

# ---------------------------------------------------------------------------
# B.4 — Smoke-check: ≥14 meta_persona_* tools available
# ---------------------------------------------------------------------------
smoke_check_tools() {
    echo ""
    echo "[B.4] Smoke-checking meta_persona_* tool count …"

    local mcp_url="http://127.0.0.1:${E2E_MCP_PORT}/mcp"
    local tool_list
    if ! tool_list=$(cargo run --quiet -p poly-cli -- --url "$mcp_url" tools 2>/dev/null); then
        echo "[B.4] ERROR: poly-cli failed to list tools from ${mcp_url}" >&2
        exit 1
    fi

    local count
    count=$(echo "$tool_list" | grep -c "meta_persona_" || true)

    if [[ "$count" -lt 14 ]]; then
        echo "[B.4] FAIL: only ${count} meta_persona_* tools found (need ≥14)" >&2
        echo "$tool_list" >&2
        exit 1
    fi

    echo "[B.4] Smoke check: ${count} meta_persona_* tools available ✓"
}

# ---------------------------------------------------------------------------
# C.1 — Generate per-persona .mcp.json
#
# Transport decision: poly-chat-mcp supports both stdio and HTTP.  HTTP is
# already running from Phase B on E2E_MCP_PORT.  We use the HTTP URL transport
# approach for simplicity — no separate stdio process per agent, no SQLite
# contention from multiple stdio instances writing concurrently.  Claude's
# --mcp-config accepts a JSON file; we give it a file that points the named
# server at the HTTP port via a tiny http-to-mcp proxy shim.
#
# HOWEVER: `claude --mcp-config` wires stdio MCP servers (command + args).
# For HTTP servers the correct approach is to use the mcp-remote or similar
# bridge, OR to use stdio mode and let each agent have its own poly-chat-mcp
# stdio process that opens the SAME POLY_DATA_DIR SQLite.  Since all agents
# talk to the same SQLite, they see each other's writes.  SQLite WAL mode
# (which MemoryDb enables) handles concurrent readers + one writer safely.
#
# Decision taken: stdio mode per-agent.  Each `claude -p` gets its own
# poly-chat-mcp --stdio process (spawned by claude as per mcp.json).  All
# share POLY_DATA_DIR → same SQLite → persona writes from agent A are visible
# to agent B via B's poly-chat-mcp --stdio reading the same DB.
#
# The POLY_DATA_DIR env var is inherited by the claude process (and thence
# poly-chat-mcp --stdio) because we export it above.
# ---------------------------------------------------------------------------
generate_persona_mcp_config() {
    local slug="$1"
    local scenario_name="$2"
    local scenario_dir="$SCRIPT_DIR/scenarios/${scenario_name}"
    local config_path="${scenario_dir}/persona-${slug}.mcp.json"

    mkdir -p "$scenario_dir"

    # Resolve the cargo-built binary path
    local chat_mcp_bin
    chat_mcp_bin="$(cargo locate-project --workspace --message-format plain 2>/dev/null \
        | sed 's|/Cargo.toml||')/target/debug/poly-chat-mcp"

    # Fallback: search PATH-style locations
    if [[ ! -x "$chat_mcp_bin" ]]; then
        chat_mcp_bin="$(command -v poly-chat-mcp 2>/dev/null || true)"
    fi

    if [[ -z "$chat_mcp_bin" || ! -x "$chat_mcp_bin" ]]; then
        echo "[C.1] WARNING: poly-chat-mcp binary not found at ${chat_mcp_bin}" >&2
        echo "[C.1] Falling back to 'cargo run -p poly-chat-mcp -- --stdio'" >&2
        cat > "$config_path" <<MCPJSON
{
  "mcpServers": {
    "poly-chat": {
      "command": "cargo",
      "args": ["run", "--quiet", "-p", "poly-chat-mcp", "--", "--stdio"],
      "env": {
        "POLY_DATA_DIR": "${POLY_DATA_DIR}"
      }
    }
  }
}
MCPJSON
    else
        cat > "$config_path" <<MCPJSON
{
  "mcpServers": {
    "poly-chat": {
      "command": "${chat_mcp_bin}",
      "args": ["--stdio"],
      "env": {
        "POLY_DATA_DIR": "${POLY_DATA_DIR}"
      }
    }
  }
}
MCPJSON
    fi

    echo "[C.1] Generated MCP config for ${slug}: ${config_path}"
    echo "$config_path"
}

# ---------------------------------------------------------------------------
# C.2 — spawn_persona_agent <slug> "<prompt>" <mcp_config_path>
#
# In real-claude mode: spawns `claude -p` with the generated mcp config.
# In mock-claude mode: calls run_mock_claude from lib/mock-claude.sh.
#
# C.4 Concurrency model: agents run SEQUENTIALLY within a scenario.
# Rationale: single SQLite WAL writer + deterministic assertion order.
# If parallelism is needed later, remove the `wait` and add per-agent
# tempfiles; document the flake risk in README.md if it surfaces.
# ---------------------------------------------------------------------------
spawn_persona_agent() {
    local slug="$1"
    local prompt="$2"
    local mcp_config="${3:-}"
    local mock_actions="${4:-}"
    local out_json="$AGENTS_DIR/${slug}.out.json"

    # Prepend mandatory directive to ensure agent uses persona surface
    local full_prompt="Use the meta_persona_invoke tool with slug=${slug} to gather context, then honour the persona's system prompt. ${prompt}"

    if [[ "$MODE" == "real-claude" ]]; then
        if [[ -z "${ANTHROPIC_API_KEY:-}" ]]; then
            echo "[C.2] ERROR: --mode real-claude requires ANTHROPIC_API_KEY to be set" >&2
            return 1
        fi
        if [[ -z "$mcp_config" || ! -f "$mcp_config" ]]; then
            echo "[C.2] ERROR: mcp_config file required for real-claude mode: ${mcp_config}" >&2
            return 1
        fi
        echo "[C.2:${slug}] spawning real claude -p agent …"
        # Run synchronously (C.4 sequential model); capture output
        claude -p "$full_prompt" \
            --mcp-config "$mcp_config" \
            --output-format json \
            --dangerously-skip-permissions \
            > "$out_json" 2>&1
        echo "[C.2:${slug}] real-claude agent done → ${out_json}"
    else
        # Mock mode (CI default) — no API key needed
        if [[ -z "$mock_actions" || ! -f "$mock_actions" ]]; then
            echo "[C.2:${slug}] WARNING: no mock-actions file at '${mock_actions}'; using empty result" >&2
            echo '{"type":"result","subtype":"success","result":"no-op mock","tool_calls":[]}' > "$out_json"
            return 0
        fi
        echo "[C.2:${slug}] running mock-claude agent …"
        local mcp_url="http://127.0.0.1:${E2E_MCP_PORT}/mcp"
        run_mock_claude "$slug" "$mock_actions" "$mcp_url" "$out_json"
        echo "[C.2:${slug}] mock-claude agent done → ${out_json}"
    fi
}

# ---------------------------------------------------------------------------
# C.3 — seed_persona <slug> <name> <system_prompt> <sources_json>
#
# Idempotent: checks meta_persona_get first; only creates if not found.
# sources_json is a JSON array of source binding objects, e.g.:
#   '[{"account_id":"test-discord","kind":"channel","value":"ch-shared","include":true}]'
# ---------------------------------------------------------------------------
seed_persona() {
    local slug="$1"
    local name="$2"
    local system_prompt="$3"
    local sources_json="$4"
    local mcp_url="http://127.0.0.1:${E2E_MCP_PORT}/mcp"

    echo "[C.3] Seeding persona: ${slug} (${name})"

    # Check if persona exists already
    local existing
    existing=$(cargo run --quiet -p poly-cli -- \
        --url "$mcp_url" \
        --format json \
        call meta_persona_get --slug "$slug" 2>/dev/null || true)

    if echo "$existing" | grep -q "\"slug\""; then
        echo "[C.3] Persona '${slug}' already exists — skipping create"
    else
        echo "[C.3] Creating persona '${slug}' …"
        cargo run --quiet -p poly-cli -- \
            --url "$mcp_url" \
            call meta_persona_create \
            --slug "$slug" \
            --name "$name" \
            --system_prompt "$system_prompt" \
            2>&1 | tee -a "$LOGS_DIR/seed-personas.log"
    fi

    # Set sources (always overwrite — idempotent at the source level)
    if [[ -n "$sources_json" && "$sources_json" != "[]" ]]; then
        echo "[C.3] Setting sources for '${slug}' …"
        cargo run --quiet -p poly-cli -- \
            --url "$mcp_url" \
            call meta_persona_set_sources \
            --slug "$slug" \
            --sources "$sources_json" \
            2>&1 | tee -a "$LOGS_DIR/seed-personas.log"
    fi

    echo "[C.3] Persona '${slug}' seeded ✓"
}

# ---------------------------------------------------------------------------
# C.5 — aggregate_agent_results
#
# Read every $AGENTS_DIR/*.out.json, extract success/failure + tool-call
# traces, write $RESULTS_DIR/agents-summary.json.
#
# claude --output-format json shape (as of Claude Code current):
#   { "type": "result", "subtype": "success"|"error_max_turns"|...,
#     "result": "<text>", "tool_calls": [...] }
# mock-claude uses same shape.
# ---------------------------------------------------------------------------
aggregate_agent_results() {
    echo ""
    echo "[C.5] Aggregating agent results …"
    local summary_file="$RESULTS_DIR/agents-summary.json"

    # Build summary using python3 (always available in CI)
    python3 - "$AGENTS_DIR" "$summary_file" <<'PYEOF'
import json, os, sys, glob

agents_dir = sys.argv[1]
summary_path = sys.argv[2]

results = []
out_files = sorted(glob.glob(os.path.join(agents_dir, "*.out.json")))

for f in out_files:
    slug = os.path.basename(f).replace(".out.json", "")
    try:
        with open(f) as fh:
            data = json.load(fh)
        subtype = data.get("subtype", "unknown")
        success = subtype == "success"
        tool_calls = data.get("tool_calls", [])
        results.append({
            "slug": slug,
            "success": success,
            "subtype": subtype,
            "result": data.get("result", ""),
            "tool_call_count": len(tool_calls),
            "tool_calls": tool_calls,
        })
    except Exception as e:
        results.append({
            "slug": slug,
            "success": False,
            "subtype": "parse_error",
            "result": str(e),
            "tool_call_count": 0,
            "tool_calls": [],
        })

total = len(results)
passed = sum(1 for r in results if r["success"])
summary = {
    "total": total,
    "passed": passed,
    "failed": total - passed,
    "agents": results,
}

with open(summary_path, "w") as fh:
    json.dump(summary, fh, indent=2)

print(f"[C.5] Summary: {passed}/{total} agents succeeded → {summary_path}")

# Print per-agent status
for r in results:
    status = "PASS" if r["success"] else "FAIL"
    print(f"  [{status}] {r['slug']} — {r['subtype']} ({r['tool_call_count']} tool calls)")
PYEOF

    # Fail the run if any agent failed
    local failed
    failed=$(python3 -c "
import json, sys
with open('${summary_file}') as f: d = json.load(f)
print(d['failed'])
" 2>/dev/null || echo "0")

    if [[ "$failed" -gt 0 ]]; then
        echo "[C.5] FAIL: ${failed} agent(s) did not succeed" >&2
        return 1
    fi
    echo "[C.5] All agents succeeded ✓"
}

# ---------------------------------------------------------------------------
# Dispatch to scenario
# ---------------------------------------------------------------------------
run_scenario() {
    local scenario="$1"
    echo ""
    echo "[scenario] Running: ${scenario}"

    case "$scenario" in
        noop)
            # Noop: just validates the stack boots correctly and exits 0.
            echo "[scenario] noop — stack is healthy, exiting cleanly"
            return 0
            ;;
        two-personas-handoff)
            # C.7 — Wire two-personas-handoff into the dispatcher
            local scenario_script="$SCRIPT_DIR/scenarios/two-personas-handoff/scenario.sh"
            if [[ ! -f "$scenario_script" ]]; then
                echo "[scenario] ERROR: no scenario script at ${scenario_script}" >&2
                exit 1
            fi
            # shellcheck disable=SC1090
            source "$scenario_script"
            run_scenario_two_personas_handoff
            ;;
        *)
            local scenario_script="$SCRIPT_DIR/scenarios/${scenario}/scenario.sh"
            if [[ ! -f "$scenario_script" ]]; then
                echo "[scenario] ERROR: no scenario script at ${scenario_script}" >&2
                exit 1
            fi
            # shellcheck disable=SC1090
            source "$scenario_script"
            ;;
    esac
}

# ---------------------------------------------------------------------------
# Main sequence
# ---------------------------------------------------------------------------
start_test_backends
start_chat_mcp

# B.3 (poly-web): only boot if the scenario explicitly needs the UI.
# Scenarios that need the UI must declare: NEEDS_POLY_WEB=true
# Scenarios that don't (agent-only, no DOM assertions) leave it unset.
# This keeps the harness fast for CI runs that don't require a WASM build.
NEEDS_POLY_WEB="${NEEDS_POLY_WEB:-false}"

if [[ "$SCENARIO" != "noop" && "$NEEDS_POLY_WEB" == "true" ]]; then
    start_poly_web
fi

smoke_check_tools
run_scenario "$SCENARIO"

# C.5 — Aggregate agent results (only when agents were spawned)
if [[ -n "$(ls "$AGENTS_DIR"/*.out.json 2>/dev/null || true)" ]]; then
    aggregate_agent_results
fi

echo ""
echo "============================================================"
echo " e2e run $RUN_ID PASSED"
echo " Artefacts: $RUN_ROOT"
echo "============================================================"

# EXIT trap fires here and cleans up all PIDs.
