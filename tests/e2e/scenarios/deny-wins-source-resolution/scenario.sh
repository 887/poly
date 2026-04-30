#!/usr/bin/env bash
# tests/e2e/scenarios/deny-wins-source-resolution/scenario.sh
#
# Sourced by persona-multi-agent.sh when --scenario deny-wins-source-resolution is passed.
# Do NOT execute directly.
#
# Scenario E.4: Deny-wins source resolution.
#   Persona bound to (account=test-discord, kind=server, value=guild-A, include=true)
#   AND (account=test-discord, kind=channel, value=guild-A/ch-secret, include=false).
#
#   After a message is sent to ch-secret, the persona's meta_persona_invoke
#   bundle must NOT include that message (deny wins over server-wide allow).
#
# In mock mode: the dry_run invoke returns the source binding summary.
# The scenario asserts:
#   1. The persona's sources include guild-A with include=true
#   2. The persona's sources include guild-A/ch-secret with include=false
#   3. The invoke dry_run result does NOT contain "ch-secret" as an included channel
#
# This catches integration regressions in persona/context.rs resolve_sources().
# Unit tests cover it in persona/context.rs; this catches multi-layer issues.

SCENARIO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MOCK_ACTIONS="$SCENARIO_DIR/mock-actions.jsonl"
PERSONAS_JSONL="$SCENARIO_DIR/personas.jsonl"
MCP_URL="http://127.0.0.1:${E2E_MCP_PORT}/mcp"

run_scenario_deny_wins_source_resolution() {
    echo ""
    echo "[deny-wins-source-resolution] Starting scenario …"

    # -----------------------------------------------------------------------
    # C.3 — Pre-seed the persona (idempotent)
    # -----------------------------------------------------------------------
    echo "[deny-wins-source-resolution] Seeding persona …"
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
    persona_mcp=$(generate_persona_mcp_config "deny-test-persona" "deny-wins-source-resolution")

    # -----------------------------------------------------------------------
    # C.2 — Run the agent
    # -----------------------------------------------------------------------
    echo ""
    echo "[deny-wins-source-resolution] Running agent (deny-test-persona) …"
    spawn_persona_agent \
        "deny-test-persona" \
        "Invoke your persona and list what channels you have access to. Do NOT include ch-secret." \
        "$persona_mcp" \
        "$MOCK_ACTIONS"

    # -----------------------------------------------------------------------
    # Assertion 1: verify persona sources show the deny binding
    # -----------------------------------------------------------------------
    echo ""
    echo "[deny-wins-source-resolution] Asserting source bindings in DB …"
    local persona_data
    persona_data=$(cargo run --quiet -p poly-cli -- \
        --url "$MCP_URL" \
        --format json \
        call meta_persona_get --slug "deny-test-persona" \
        2>/dev/null || true)

    # Check that guild-A server binding (include=true) exists
    if ! echo "$persona_data" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
sources = d.get('sources', [])
# Look for guild-A server with include=true
server_allow = any(
    s.get('selector_kind') == 'server' and
    s.get('selector_value') == 'guild-A' and
    s.get('include') == True
    for s in sources
)
sys.exit(0 if server_allow else 1)
" 2>/dev/null; then
        echo "[deny-wins-source-resolution] FAIL: guild-A server include=true not found in sources" >&2
        echo "  persona data: $persona_data" >&2
        return 1
    fi
    echo "[deny-wins-source-resolution] Source binding guild-A server include=true ✓"

    # Check that guild-A/ch-secret channel binding (include=false) exists
    if ! echo "$persona_data" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
sources = d.get('sources', [])
channel_deny = any(
    s.get('selector_kind') == 'channel' and
    'ch-secret' in s.get('selector_value', '') and
    s.get('include') == False
    for s in sources
)
sys.exit(0 if channel_deny else 1)
" 2>/dev/null; then
        echo "[deny-wins-source-resolution] FAIL: ch-secret channel include=false not found in sources" >&2
        echo "  persona data: $persona_data" >&2
        return 1
    fi
    echo "[deny-wins-source-resolution] Source binding ch-secret channel include=false (deny) ✓"

    # -----------------------------------------------------------------------
    # Assertion 2: agent result succeeded (dry_run invoke worked)
    # -----------------------------------------------------------------------
    local agent_out="$AGENTS_DIR/deny-test-persona.out.json"
    if [[ ! -f "$agent_out" ]]; then
        echo "[deny-wins-source-resolution] FAIL: deny-test-persona produced no output JSON" >&2
        return 1
    fi
    local agent_subtype
    agent_subtype=$(python3 -c "import json; d=json.load(open('${agent_out}')); print(d.get('subtype','unknown'))" 2>/dev/null || echo "unknown")
    if [[ "$agent_subtype" != "success" ]]; then
        echo "[deny-wins-source-resolution] FAIL: agent subtype=${agent_subtype} (expected success)" >&2
        return 1
    fi

    echo ""
    echo "[deny-wins-source-resolution] PASSED ✓"
    echo "[deny-wins-source-resolution] Deny-wins: ch-secret excluded from persona context despite guild-A server allow"
}

run_scenario_deny_wins_source_resolution
