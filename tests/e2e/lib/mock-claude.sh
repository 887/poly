#!/usr/bin/env bash
# tests/e2e/lib/mock-claude.sh — Mock replacement for `claude -p` in CI.
#
# Source this file; do NOT execute it directly.
#
# Design:
#   In mock mode (the default for CI), the harness calls run_mock_claude
#   instead of the real `claude` CLI.  This avoids Anthropic API calls,
#   keeps the test deterministic, and requires no ANTHROPIC_API_KEY.
#
#   Each scenario supplies a `mock-actions.jsonl` file that lists the exact
#   MCP tool calls the stub should make for each slug.  The stub reads
#   that file, fires the matching tool calls via poly-cli, and writes a
#   synthetic `--output-format json`-style output to the agent's .out.json.
#
# Mock output format (mirrors claude --output-format json):
#   {
#     "type": "result",
#     "subtype": "success",
#     "result": "<text content>",
#     "tool_calls": [ { "name": "<tool>", "args": {…}, "result": "<text>" } ]
#   }
#
# mock-actions.jsonl line shape (one JSON object per line):
#   { "slug": "broker-bob", "tool": "meta_persona_invoke",
#     "args": { "slug": "broker-bob", "user_prompt": "check channel" },
#     "result_grep": "broker-bob"  }
#
# If result_grep is set, the mock asserts the poly-cli output contains that
# substring and fails loudly if it does not.

# ---------------------------------------------------------------------------
# run_mock_claude <slug> <mock_actions_file> <mcp_url> <out_json>
#   Replay the actions for <slug> from <mock_actions_file>.
#   Writes synthetic JSON result to <out_json>.
#   Returns 0 on success, 1 on assertion failure.
# ---------------------------------------------------------------------------
run_mock_claude() {
    local slug="$1"
    local mock_actions_file="$2"
    local mcp_url="$3"
    local out_json="$4"

    echo "[mock-claude:${slug}] starting (actions: ${mock_actions_file})"

    if [[ ! -f "$mock_actions_file" ]]; then
        echo "[mock-claude:${slug}] ERROR: mock-actions file not found: ${mock_actions_file}" >&2
        return 1
    fi

    local tool_calls_json="[]"
    local final_result="mock-claude ${slug}: all actions completed"
    local ok=true

    while IFS= read -r line; do
        # Skip blank lines and comments
        [[ -z "$line" || "$line" == \#* ]] && continue

        local action_slug action_tool action_args result_grep
        action_slug=$(echo "$line" | python3 -c "import sys,json; d=json.loads(sys.stdin.read()); print(d.get('slug',''))" 2>/dev/null || true)

        # Only process lines matching our slug
        if [[ "$action_slug" != "$slug" ]]; then
            continue
        fi

        action_tool=$(echo "$line" | python3 -c "import sys,json; d=json.loads(sys.stdin.read()); print(d.get('tool',''))" 2>/dev/null || true)
        action_args=$(echo "$line" | python3 -c "import sys,json; d=json.loads(sys.stdin.read()); print(json.dumps(d.get('args',{})))" 2>/dev/null || echo '{}')
        result_grep=$(echo "$line" | python3 -c "import sys,json; d=json.loads(sys.stdin.read()); print(d.get('result_grep',''))" 2>/dev/null || true)

        if [[ -z "$action_tool" ]]; then
            echo "[mock-claude:${slug}] skipping line with no tool: ${line}" >&2
            continue
        fi

        echo "[mock-claude:${slug}] calling ${action_tool} with args: ${action_args}"

        # Build poly-cli args from the args JSON object
        local cli_args=()
        while IFS='=' read -r k v; do
            cli_args+=("--${k}" "${v}")
        done < <(echo "$action_args" | python3 -c "
import sys, json
d = json.loads(sys.stdin.read())
for k, v in d.items():
    print(f'{k}={v}')
" 2>/dev/null || true)

        local call_result
        call_result=$(cargo run --quiet -p poly-cli -- \
            --url "$mcp_url" \
            --format json \
            call "$action_tool" "${cli_args[@]}" 2>/dev/null || true)

        echo "[mock-claude:${slug}] ${action_tool} → ${call_result}"

        # Assertion: result_grep must appear in output
        if [[ -n "$result_grep" && -n "$call_result" ]]; then
            if ! echo "$call_result" | grep -q "$result_grep"; then
                echo "[mock-claude:${slug}] ASSERTION FAILED: expected '${result_grep}' in output:" >&2
                echo "$call_result" >&2
                ok=false
            else
                echo "[mock-claude:${slug}] assertion '${result_grep}' ✓"
            fi
        fi

        # Append to tool_calls JSON array (simple concatenation approach)
        local escaped_result
        escaped_result=$(echo "$call_result" | python3 -c "import sys,json; print(json.dumps(sys.stdin.read()))" 2>/dev/null || echo '""')
        local escaped_args
        escaped_args=$(echo "$action_args" | python3 -c "import sys,json; d=json.loads(sys.stdin.read()); print(json.dumps(d))" 2>/dev/null || echo '{}')

        # Append the tool call record
        tool_calls_json=$(echo "$tool_calls_json" | python3 -c "
import sys, json
arr = json.loads(sys.stdin.read())
arr.append({'name': '${action_tool}', 'args': json.loads('${escaped_args}'), 'result': ${escaped_result}})
print(json.dumps(arr))
" 2>/dev/null || echo "$tool_calls_json")

        final_result="mock-claude ${slug}: ${action_tool} succeeded"
    done < "$mock_actions_file"

    # Write the synthetic output JSON
    local exit_status="success"
    if [[ "$ok" != true ]]; then
        exit_status="failure"
        final_result="mock-claude ${slug}: assertion failure — see log"
    fi

    python3 -c "
import json, sys
print(json.dumps({
    'type': 'result',
    'subtype': '${exit_status}',
    'result': '${final_result}',
    'tool_calls': json.loads(sys.argv[1])
}))
" "$tool_calls_json" > "$out_json" 2>/dev/null || \
    echo "{\"type\":\"result\",\"subtype\":\"${exit_status}\",\"result\":\"${final_result}\",\"tool_calls\":[]}" > "$out_json"

    echo "[mock-claude:${slug}] done → ${out_json}"

    if [[ "$ok" != true ]]; then
        return 1
    fi
    return 0
}
