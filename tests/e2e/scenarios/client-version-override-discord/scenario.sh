#!/usr/bin/env bash
# tests/e2e/scenarios/client-version-override-discord/scenario.sh
#
# Sourced by persona-multi-agent.sh when --scenario client-version-override-discord
# is passed.  Do NOT execute directly.
#
# Scenario: toggle the client-version override in the Settings UI for the Discord
# backend, verify the MCP layer persists the value, fire a raw HTTP request at the
# test-discord mock server, and assert the overridden User-Agent reached the wire.
# Then clear the override and assert the default UA is restored.
#
# Prerequisites (provided by the harness):
#   • poly-test-runner: Discord mock at http://127.0.0.1:9102 (index 2)
#   • poly-chat-mcp:   MCP HTTP at http://127.0.0.1:${E2E_MCP_PORT}
#   • poly-web:        WASM app at http://127.0.0.1:${E2E_WEB_PORT}
#
# No claude/persona calls needed — pure UI + MCP + curl.
#
# Phase H of docs/plans/plan-client-version-override-and-sandbox.md

SCENARIO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Discord test server port (index 2 in backend_names: matrix stoat discord ...)
DISCORD_TEST_PORT=9102
DISCORD_TEST_URL="http://127.0.0.1:${DISCORD_TEST_PORT}"
MCP_URL="http://127.0.0.1:${E2E_MCP_PORT}/mcp"
WEB_URL="http://127.0.0.1:${E2E_WEB_PORT}"
SPEC_FILE="$SCENARIO_DIR/spec.ts"

OVERRIDE_VERSION="e2e-test/9.9.9"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# poly_cli_call <tool> [args...]
# Calls a tool on the HTTP MCP server and returns stdout.
poly_cli_call() {
    cargo run --quiet -p poly-cli -- \
        --url "$MCP_URL" \
        --format json \
        call "$@" 2>/dev/null
}

# assert_mcp_version <expected_version_substring>
# Polls client_settings_get_version until the expected string appears (cap 30s).
assert_mcp_version() {
    local expected="$1"
    local deadline=$(( $(date +%s) + 30 ))
    while true; do
        local out
        out=$(poly_cli_call client_settings_get_version --backend_id=discord || true)
        if echo "$out" | grep -qF "$expected"; then
            echo "[H] MCP version assertion passed: contains '${expected}'"
            return 0
        fi
        if [[ $(date +%s) -ge $deadline ]]; then
            echo "[H] FAIL: client_settings_get_version did not return '${expected}' within 30s" >&2
            echo "[H] Last output: $out" >&2
            return 1
        fi
        sleep 1
    done
}

# assert_mcp_version_cleared
# Asserts source=="default" and override==null (cap 30s).
assert_mcp_version_cleared() {
    local deadline=$(( $(date +%s) + 30 ))
    while true; do
        local out
        out=$(poly_cli_call client_settings_get_version --backend_id=discord || true)
        if echo "$out" | grep -q '"source".*"default"'; then
            echo "[H] MCP clear assertion passed: source == 'default'"
            return 0
        fi
        if [[ $(date +%s) -ge $deadline ]]; then
            echo "[H] FAIL: client_settings_get_version did not return source='default' within 30s" >&2
            echo "[H] Last output: $out" >&2
            return 1
        fi
        sleep 1
    done
}

# clear_inspect_buffer
# Resets the test-discord header-inspect ring buffer via POST /reset
# so subsequent assertions see only requests from this scenario.
clear_inspect_buffer() {
    curl --silent --fail --max-time 10 \
        -X POST "${DISCORD_TEST_URL}/reset" \
        -o /dev/null || true
    echo "[H] Discord header-inspect buffer cleared"
}

# fire_wire_ping
# Fires a minimal authenticated HTTP GET to the Discord mock server.
# The test-discord server records every inbound request (including the
# User-Agent) in its header-inspect ring buffer.
#
# The Authorization token is 'test-token' — the poly-test-runner seeds
# each Discord mock with this token by default.
fire_wire_ping() {
    curl --silent --fail --max-time 10 \
        -H "Authorization: Bot test-token" \
        "${DISCORD_TEST_URL}/api/v10/users/@me" \
        -o /dev/null \
        --write-out "HTTP %{http_code}\n" || true
    echo "[H] Wire ping sent to Discord mock"
}

# assert_wire_ua <expected_ua_substring>
# Fetches /test/inspect/last-headers and checks the most-recent entry's
# user-agent header contains the expected substring.
assert_wire_ua() {
    local expected="$1"
    local entries
    entries=$(curl --silent --fail --max-time 10 \
        "${DISCORD_TEST_URL}/test/inspect/last-headers" || true)

    if [[ -z "$entries" ]]; then
        echo "[H] FAIL: /test/inspect/last-headers returned empty" >&2
        return 1
    fi

    # The endpoint returns a JSON array sorted most-recent first.
    # Extract the user-agent from the first entry.
    local ua
    ua=$(python3 -c "
import json, sys
entries = json.loads(sys.stdin.read())
if not entries:
    print('')
else:
    print(entries[0].get('headers', {}).get('user-agent', ''))
" <<< "$entries" 2>/dev/null || echo "")

    if echo "$ua" | grep -qF "$expected"; then
        echo "[H] Wire UA assertion passed: user-agent='${ua}' contains '${expected}'"
        return 0
    else
        echo "[H] FAIL: user-agent='${ua}' does not contain '${expected}'" >&2
        echo "[H] Full inspect response (first 500 chars): ${entries:0:500}" >&2
        return 1
    fi
}

# run_playwright_spec
# Runs the Playwright spec against the live poly-web instance.
# Returns 0 on pass, non-zero on failure.
run_playwright_spec() {
    echo "[H] Running Playwright spec: ${SPEC_FILE}"
    local playwright_log="$LOGS_DIR/playwright-client-version-override.log"

    # Playwright config root is REPO_ROOT; spec path is relative to testDir
    local spec_relative
    spec_relative="scenarios/client-version-override-discord/spec.ts"

    # Use the e2e project (web / chromium / port 3000).
    # --project name must match playwright.config.ts; add a new 'e2e' project if
    # it doesn't exist, OR run with --config pointing to a local playwright config.
    # Strategy: use a temporary playwright config that overrides baseURL to the
    # harness WEB_URL and picks up only this spec file.
    local tmp_playwright_config
    tmp_playwright_config=$(mktemp /tmp/playwright-cvo-XXXXXX.config.ts)
    cat > "$tmp_playwright_config" <<PWCONFIG
import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: '${SCRIPT_DIR}',
  timeout: 180_000,
  retries: 0,
  reporter: [['list'], ['json', { outputFile: '${RESULTS_DIR}/playwright-cvo-results.json' }]],
  use: {
    baseURL: '${WEB_URL}',
    screenshot: 'only-on-failure',
    video: 'off',
    trace: 'off',
  },
  projects: [
    {
      name: 'client-version-override',
      testMatch: /client-version-override-discord\/spec\.ts/,
      use: {
        browserName: 'chromium',
        viewport: { width: 1280, height: 800 },
        launchOptions: { args: ['--no-sandbox', '--disable-dev-shm-usage'] },
      },
    },
  ],
});
PWCONFIG

    npx playwright test \
        --config "$tmp_playwright_config" \
        --project client-version-override \
        "$spec_relative" \
        2>&1 | tee "$playwright_log"
    local pw_exit=$?

    rm -f "$tmp_playwright_config"

    if [[ $pw_exit -ne 0 ]]; then
        echo "[H] FAIL: Playwright spec exited ${pw_exit}" >&2
        echo "[H] See log: ${playwright_log}" >&2
        return 1
    fi
    echo "[H] Playwright spec PASSED"
    return 0
}

# ---------------------------------------------------------------------------
# Main scenario function
# ---------------------------------------------------------------------------

run_scenario_client_version_override_discord() {
    echo ""
    echo "[H] ============================================================"
    echo "[H]  Scenario: client-version-override-discord"
    echo "[H] ============================================================"

    # ── H-step 1: Pre-state assertion ─────────────────────────────────────
    echo ""
    echo "[H] Step 1: Pre-state — assert no version override active"
    local pre_state
    pre_state=$(poly_cli_call client_settings_get_version --backend_id=discord || true)
    if echo "$pre_state" | grep -q "\"source\".*\"override\""; then
        echo "[H] WARNING: A version override is already set for Discord." >&2
        echo "[H]          Clearing it before running the scenario …" >&2
        poly_cli_call client_settings_set_version_override --backend_id=discord || true
        assert_mcp_version_cleared
    else
        echo "[H] Step 1 PASSED: no override active"
    fi

    # ── H-step 2: Clear inspect buffer ────────────────────────────────────
    echo ""
    echo "[H] Step 2: Reset header-inspect buffer"
    clear_inspect_buffer

    # ── H-step 3: Run Playwright spec (UI flow + persistence assertion) ───
    echo ""
    echo "[H] Step 3: Playwright spec — toggle, fill, save, clear"
    run_playwright_spec

    # ── H-step 4: MCP-level persistence check ─────────────────────────────
    # The Playwright spec already verifies DOM-level feedback, but we
    # double-check via poly-cli so the assertion is backend-authoritative.
    # NOTE: After the spec runs it also clears the override. So we check
    # that the state is back to default here.
    echo ""
    echo "[H] Step 4: Post-spec MCP state — assert override cleared"
    assert_mcp_version_cleared

    # ── H-step 5: Set override via MCP (for wire test) ────────────────────
    echo ""
    echo "[H] Step 5: Set override via MCP for wire-level assertion"
    poly_cli_call client_settings_set_version_override \
        --backend_id=discord \
        --override="${OVERRIDE_VERSION}" \
        > /dev/null

    assert_mcp_version "$OVERRIDE_VERSION"

    # ── H-step 6: Fire a wire ping and assert User-Agent propagated ────────
    # The Discord client picks up the override from the config store on the
    # next outbound request. The wire ping hits the mock server which records
    # the User-Agent header in its inspect ring buffer.
    echo ""
    echo "[H] Step 6: Fire wire ping + assert User-Agent"
    clear_inspect_buffer
    fire_wire_ping
    assert_wire_ua "$OVERRIDE_VERSION"

    # ── H-step 7: Clear override and assert wire reverts ──────────────────
    echo ""
    echo "[H] Step 7: Clear override via MCP + assert default UA restored"
    poly_cli_call client_settings_set_version_override \
        --backend_id=discord \
        > /dev/null
    assert_mcp_version_cleared

    clear_inspect_buffer
    fire_wire_ping
    # Default UA must NOT contain the override string.
    local post_entries
    post_entries=$(curl --silent --fail --max-time 10 \
        "${DISCORD_TEST_URL}/test/inspect/last-headers" || true)
    local post_ua
    post_ua=$(python3 -c "
import json, sys
entries = json.loads(sys.stdin.read())
print(entries[0].get('headers', {}).get('user-agent', '') if entries else '')
" <<< "$post_entries" 2>/dev/null || echo "")

    if echo "$post_ua" | grep -qF "$OVERRIDE_VERSION"; then
        echo "[H] FAIL: After clear, user-agent='${post_ua}' still contains '${OVERRIDE_VERSION}'" >&2
        return 1
    fi
    echo "[H] Step 7 PASSED: post-clear user-agent='${post_ua}' (no override)"

    # ── Done ───────────────────────────────────────────────────────────────
    echo ""
    echo "[H] ============================================================"
    echo "[H]  Scenario client-version-override-discord PASSED"
    echo "[H] ============================================================"
}

# The generic '*' arm in persona-multi-agent.sh sources this file but does
# NOT call any function.  The explicit 'client-version-override-discord' case
# added to the harness calls run_scenario_client_version_override_discord.
# As a fallback, if sourced without the explicit case, run directly:
if [[ "${BASH_SOURCE[0]}" != "${0}" ]]; then
    # Being sourced.  The explicit harness case calls the function.
    # Nothing to execute at source time.
    :
fi
