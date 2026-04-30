#!/usr/bin/env bash
# tests/e2e/scenarios/two-personas-shared-channel/scenario.sh
#
# Sourced by persona-multi-agent.sh when --scenario two-personas-shared-channel is passed.
# Do NOT execute directly.
#
# Scenario E.1: Two personas share the same Discord channel (ch-shared).
#   Agent A (broker-bob): invokes its persona and "sends" a market update.
#   Agent B (greens-greg): invokes its persona and reads from the same channel.
#   Playwright asserts both persona rows are visible in the UI within 5s.
#
# This is the headline two-agent interaction test. It validates:
#   - Both personas are seeded and visible in the PersonaListPanel
#   - Both agents can invoke their persona via meta_persona_invoke
#   - No page reload occurs while the MCP state changes propagate
#
# What regression does this catch?
#   If the PersonaListPanel reactive subscription breaks — e.g. someone removes
#   the meta_persona_list poll or breaks the signal subscription — neither
#   persona row will appear and Playwright fails immediately.

SCENARIO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MOCK_ACTIONS="$SCENARIO_DIR/mock-actions.jsonl"
PERSONAS_JSONL="$SCENARIO_DIR/personas.jsonl"
ASSERTIONS_TMPL="$SCENARIO_DIR/assertions.json.tmpl"
MCP_URL="http://127.0.0.1:${E2E_MCP_PORT}/mcp"

run_scenario_two_personas_shared_channel() {
    echo ""
    echo "[two-personas-shared-channel] Starting scenario …"

    # -----------------------------------------------------------------------
    # C.3 — Pre-seed both personas (idempotent)
    # -----------------------------------------------------------------------
    echo "[two-personas-shared-channel] Seeding personas …"
    while IFS= read -r line; do
        [[ -z "$line" || "$line" == \#* ]] && continue
        local slug name system_prompt sources
        slug=$(echo "$line"          | python3 -c "import sys,json; print(json.loads(sys.stdin.read())['slug'])")
        name=$(echo "$line"          | python3 -c "import sys,json; print(json.loads(sys.stdin.read())['name'])")
        system_prompt=$(echo "$line" | python3 -c "import sys,json; print(json.loads(sys.stdin.read())['system_prompt'])")
        sources=$(echo "$line"       | python3 -c "import sys,json; print(json.dumps(json.loads(sys.stdin.read()).get('sources',[])))")
        seed_persona "$slug" "$name" "$system_prompt" "$sources"
    done < "$PERSONAS_JSONL"

    # -----------------------------------------------------------------------
    # C.1 — Generate per-persona .mcp.json configs
    # -----------------------------------------------------------------------
    local bob_mcp greg_mcp
    bob_mcp=$(generate_persona_mcp_config  "broker-bob"   "two-personas-shared-channel")
    greg_mcp=$(generate_persona_mcp_config "greens-greg"  "two-personas-shared-channel")

    # -----------------------------------------------------------------------
    # D.5 — Write Playwright manifest BEFORE agents run so since_ts captures
    #        the pre-agent timestamp (no_full_reload window starts here).
    # -----------------------------------------------------------------------
    SCENARIO_ASSERTIONS=(
        '{"kind":"wait_for_visible","locator":"[data-testid=\"persona-row-broker-bob\"]","timeout_ms":5000}'
        '{"kind":"wait_for_visible","locator":"[data-testid=\"persona-row-greens-greg\"]","timeout_ms":5000}'
        '{"kind":"no_full_reload","since_ts":"@@SINCE_TS@@"}'
    )
    write_scenario_manifest "two-personas-shared-channel"

    # -----------------------------------------------------------------------
    # C.2 / C.4 — Run agents SEQUENTIALLY (broker-bob first, then greens-greg)
    # -----------------------------------------------------------------------
    echo ""
    echo "[two-personas-shared-channel] Running Agent A (broker-bob) …"
    spawn_persona_agent \
        "broker-bob" \
        "Post a market update: COIN beat earnings by 12%. Use ch-shared." \
        "$bob_mcp" \
        "$MOCK_ACTIONS"

    echo ""
    echo "[two-personas-shared-channel] Running Agent B (greens-greg) …"
    spawn_persona_agent \
        "greens-greg" \
        "Read ch-shared and respond with environmental impact of COIN earnings." \
        "$greg_mcp" \
        "$MOCK_ACTIONS"

    # -----------------------------------------------------------------------
    # Assertions: both personas visible in the shared DB
    # -----------------------------------------------------------------------
    echo ""
    echo "[two-personas-shared-channel] Asserting both personas in shared DB …"
    local persona_list
    persona_list=$(cargo run --quiet -p poly-cli -- \
        --url "$MCP_URL" \
        --format json \
        call meta_persona_list \
        2>/dev/null || true)

    if ! echo "$persona_list" | grep -q "broker-bob"; then
        echo "[two-personas-shared-channel] FAIL: broker-bob not in persona list" >&2
        return 1
    fi
    if ! echo "$persona_list" | grep -q "greens-greg"; then
        echo "[two-personas-shared-channel] FAIL: greens-greg not in persona list" >&2
        return 1
    fi

    echo "[two-personas-shared-channel] Shared DB assertion: both personas visible ✓"
    echo ""
    echo "[two-personas-shared-channel] PASSED ✓"
}

# Dispatch via the generic case in run_scenario → source this file → call below.
run_scenario_two_personas_shared_channel
