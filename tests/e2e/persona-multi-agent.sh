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

mkdir -p "$PIDS_DIR" "$LOGS_DIR" "$DATA_DIR" "$RESULTS_DIR"

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
# Dispatch to scenario
# ---------------------------------------------------------------------------
run_scenario() {
    local scenario="$1"
    echo ""
    echo "[scenario] Running: ${scenario}"

    case "$scenario" in
        noop)
            # Noop: just validates the stack boots correctly and exits 0.
            # Phases C+ will add real scenarios here.
            echo "[scenario] noop — stack is healthy, exiting cleanly"
            return 0
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

# B.3 (poly-web): only boot if the scenario needs the UI.
# noop does NOT boot poly-web to keep the harness fast for CI dry-runs.
if [[ "$SCENARIO" != "noop" ]]; then
    start_poly_web
fi

smoke_check_tools
run_scenario "$SCENARIO"

echo ""
echo "============================================================"
echo " e2e run $RUN_ID PASSED"
echo " Artefacts: $RUN_ROOT"
echo "============================================================"

# EXIT trap fires here and cleans up all PIDs.
