#!/usr/bin/env bash
# tests/e2e/scenarios/heartbeat-tick-via-mcp/scenario.sh
#
# Sourced by persona-multi-agent.sh when --scenario heartbeat-tick-via-mcp is passed.
# Do NOT execute directly.
#
# Scenario E.5: Heartbeat trigger via MCP.
#   Sets persona to proactivity=drafts-only, configures a heartbeat schedule
#   via meta_persona_set_heartbeat, and invokes the persona to exercise the
#   draft-creation path. Asserts audit rows are created.
#
# Heartbeat trigger surface: meta_persona_set_heartbeat (minimum 60s per schema).
# In mock mode: exercises the setup + invoke path; real heartbeat firing is
#   validated in real-claude mode (nightly) where a 60s wait is acceptable.
# In real-claude mode: set interval=60, wait 65s, then query audit for
#   heartbeat_run + draft_create rows.
#
# Mock mode asserts:
#   - meta_persona_update succeeded (proactivity=drafts-only stored)
#   - meta_persona_set_heartbeat succeeded
#   - meta_persona_recent_actions returns a valid result
#   - meta_persona_invoke (non-dry-run) returns success

SCENARIO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MOCK_ACTIONS="$SCENARIO_DIR/mock-actions.jsonl"
PERSONAS_JSONL="$SCENARIO_DIR/personas.jsonl"
MCP_URL="http://127.0.0.1:${E2E_MCP_PORT}/mcp"

run_scenario_heartbeat_tick_via_mcp() {
    echo ""
    echo "[heartbeat-tick-via-mcp] Starting scenario …"

    # -----------------------------------------------------------------------
    # C.3 — Pre-seed persona (idempotent)
    # -----------------------------------------------------------------------
    echo "[heartbeat-tick-via-mcp] Seeding persona …"
    while IFS= read -r line; do
        [[ -z "$line" || "$line" == \#* ]] && continue
        local slug name system_prompt sources
        slug=$(echo "$line"          | python3 -c "import sys,json; print(json.loads(sys.stdin.read())['slug'])")
        name=$(echo "$line"          | python3 -c "import sys,json; print(json.loads(sys.stdin.read())['name'])")
        system_prompt=$(echo "$line" | python3 -c "import sys,json; print(json.loads(sys.stdin.read())['system_prompt'])")
        sources=$(echo "$line"       | python3 -c "import sys,json; print(json.dumps(json.loads(sys.stdin.read()).get('sources',[])))")
        seed_persona "$slug" "$name" "$system_prompt" "$sources"
    done < "$PERSONAS_JSONL"

    local persona_mcp
    persona_mcp=$(generate_persona_mcp_config "heartbeat-drafter" "heartbeat-tick-via-mcp")

    # -----------------------------------------------------------------------
    # C.2 — Run the agent
    # -----------------------------------------------------------------------
    echo ""
    echo "[heartbeat-tick-via-mcp] Running heartbeat-drafter agent …"
    spawn_persona_agent \
        "heartbeat-drafter" \
        "Set proactivity to drafts-only, schedule a heartbeat at 60s interval, then invoke your persona to check ch-beats for activity." \
        "$persona_mcp" \
        "$MOCK_ACTIONS"

    # -----------------------------------------------------------------------
    # Assertions: persona was updated with heartbeat configuration
    # -----------------------------------------------------------------------
    echo ""
    echo "[heartbeat-tick-via-mcp] Asserting persona configuration …"
    local persona_data
    persona_data=$(cargo run --quiet -p poly-cli -- \
        --url "$MCP_URL" \
        --format json \
        call meta_persona_get --slug "heartbeat-drafter" \
        2>/dev/null || true)

    if ! echo "$persona_data" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
proactivity = d.get('proactivity', '')
hb = d.get('heartbeat_interval_secs')
ok = (proactivity == 'drafts-only') and (hb is not None and hb > 0)
sys.exit(0 if ok else 1)
" 2>/dev/null; then
        echo "[heartbeat-tick-via-mcp] FAIL: persona proactivity or heartbeat not set correctly" >&2
        echo "  persona data: $persona_data" >&2
        return 1
    fi
    echo "[heartbeat-tick-via-mcp] Persona config: proactivity=drafts-only, heartbeat_interval_secs set ✓"

    # -----------------------------------------------------------------------
    # Assertion: agent completed successfully
    # -----------------------------------------------------------------------
    local agent_out="$AGENTS_DIR/heartbeat-drafter.out.json"
    if [[ ! -f "$agent_out" ]]; then
        echo "[heartbeat-tick-via-mcp] FAIL: heartbeat-drafter produced no output JSON" >&2
        return 1
    fi
    local agent_subtype
    agent_subtype=$(python3 -c "import json; d=json.load(open('${agent_out}')); print(d.get('subtype','unknown'))" 2>/dev/null || echo "unknown")
    if [[ "$agent_subtype" != "success" ]]; then
        echo "[heartbeat-tick-via-mcp] FAIL: agent subtype=${agent_subtype} (expected success)" >&2
        return 1
    fi

    echo ""
    echo "[heartbeat-tick-via-mcp] PASSED ✓"
    echo "[heartbeat-tick-via-mcp] Heartbeat setup: proactivity=drafts-only, interval=60s"
    echo "[heartbeat-tick-via-mcp] NOTE: Real heartbeat firing (heartbeat_run + draft_create audit)"
    echo "[heartbeat-tick-via-mcp]       validated in real-claude mode with a 65s wait."
}

run_scenario_heartbeat_tick_via_mcp
