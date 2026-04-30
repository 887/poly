#!/usr/bin/env bash
# tests/e2e/scenarios/mcp-to-ui-live-update/scenario.sh
#
# Sourced by persona-multi-agent.sh when --scenario mcp-to-ui-live-update is passed.
# Do NOT execute directly.
#
# Scenario E.3 — HEADLINE REGRESSION TEST.
#   Invoke meta_persona_create via poly-cli (same surface as an MCP tool call).
#   Playwright asserts the new persona row appears in PersonaListPanel within 5s
#   and that no full page reload occurred.
#
# This is the load-bearing reactive chain test:
#   SQLite INSERT → backend events → poll_events → app_state BatchedSignal →
#   PersonaListPanel re-render → data-testid="persona-row-live-probe-xyz" visible
#
# If this scenario fails: the reactive subscription from SQLite to the WASM DOM
# is broken. Do not merge any change that breaks E.3.

SCENARIO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MOCK_ACTIONS="$SCENARIO_DIR/mock-actions.jsonl"
PERSONAS_JSONL="$SCENARIO_DIR/personas.jsonl"
MCP_URL="http://127.0.0.1:${E2E_MCP_PORT}/mcp"

run_scenario_mcp_to_ui_live_update() {
    echo ""
    echo "[mcp-to-ui-live-update] Starting scenario — HEADLINE REGRESSION TEST …"

    # -----------------------------------------------------------------------
    # Seed the orchestrator persona (used for mock-claude mode dispatch only)
    # -----------------------------------------------------------------------
    echo "[mcp-to-ui-live-update] Seeding orchestrator persona …"
    while IFS= read -r line; do
        [[ -z "$line" || "$line" == \#* ]] && continue
        local slug name system_prompt sources
        slug=$(echo "$line"          | python3 -c "import sys,json; print(json.loads(sys.stdin.read())['slug'])")
        name=$(echo "$line"          | python3 -c "import sys,json; print(json.loads(sys.stdin.read())['name'])")
        system_prompt=$(echo "$line" | python3 -c "import sys,json; print(json.loads(sys.stdin.read())['system_prompt'])")
        sources=$(echo "$line"       | python3 -c "import sys,json; print(json.dumps(json.loads(sys.stdin.read()).get('sources',[])))")
        seed_persona "$slug" "$name" "$system_prompt" "$sources"
    done < "$PERSONAS_JSONL"

    local orchestrator_mcp
    orchestrator_mcp=$(generate_persona_mcp_config "live-update-test" "mcp-to-ui-live-update")

    # -----------------------------------------------------------------------
    # D.5 — Write manifest BEFORE the MCP action so since_ts is pre-action.
    #        Playwright reads the manifest after agents complete.
    # -----------------------------------------------------------------------
    SCENARIO_ASSERTIONS=(
        '{"kind":"wait_for_visible","locator":"[data-testid=\"persona-row-live-probe-xyz\"]","timeout_ms":5000}'
        '{"kind":"no_full_reload","since_ts":"@@SINCE_TS@@"}'
    )
    write_scenario_manifest "mcp-to-ui-live-update"

    # -----------------------------------------------------------------------
    # Run the agent: creates live-probe-xyz via meta_persona_create
    # -----------------------------------------------------------------------
    echo ""
    echo "[mcp-to-ui-live-update] Running orchestrator agent (will create live-probe-xyz) …"
    spawn_persona_agent \
        "live-update-test" \
        "Create a new persona with slug live-probe-xyz and name 'Live Probe XYZ'. Then verify it was created." \
        "$orchestrator_mcp" \
        "$MOCK_ACTIONS"

    # -----------------------------------------------------------------------
    # Verify the new persona exists in the DB (belt-and-suspenders)
    # -----------------------------------------------------------------------
    echo ""
    echo "[mcp-to-ui-live-update] Verifying live-probe-xyz exists in DB …"
    local probe_result
    probe_result=$(cargo run --quiet -p poly-cli -- \
        --url "$MCP_URL" \
        --format json \
        call meta_persona_get --slug "live-probe-xyz" \
        2>/dev/null || true)

    if ! echo "$probe_result" | grep -q "live-probe-xyz"; then
        echo "[mcp-to-ui-live-update] FAIL: live-probe-xyz not found in DB after creation" >&2
        echo "  meta_persona_get output: $probe_result" >&2
        return 1
    fi

    echo "[mcp-to-ui-live-update] DB assertion: live-probe-xyz exists ✓"
    echo ""
    echo "[mcp-to-ui-live-update] PASSED (DB layer) ✓"
    echo "[mcp-to-ui-live-update] Playwright will assert DOM update within 5s …"
    # Playwright step runs automatically via run_playwright_assertions in main sequence.
}

run_scenario_mcp_to_ui_live_update
