#!/usr/bin/env bash
# forbid-unaudited-persona-tool.sh — Phase Q.2 of plan-persona-quality-gates.md.
#
# Greps `mcp/chat-mcp/src/tools.rs` for every `fn handle_meta_persona_*`
# function and asserts each calls `audit(` or `record_persona_audit(` at
# least once in its body (before the next `fn handle_meta_persona_*`).
#
# Missing an audit call is persona privacy class P2 — a state-changing MCP
# handler that skips writing an audit row makes the persona audit trail
# incomplete and forensically unreliable.
#
# Usage:
#   ./tools/scripts/forbid-unaudited-persona-tool.sh
#   # Override scan root:
#   ROOT=/tmp/some-worktree ./tools/scripts/forbid-unaudited-persona-tool.sh
#
# Inline allowlist: add the following comment anywhere in the function body
# (replace <reason> with a short rationale):
#   // poly-lint: allow unaudited-persona-tool — <reason>
#
# File-level allowlist: tools/scripts/unaudited-persona-tool-allowlist.txt
# Format: handler_suffix  # e.g.  _list   or   _recent_actions
#   (suffix is the part after `handle_meta_persona`)
#
# Exit codes:
#   0 — clean (all state-changing handlers have audit calls)
#   1 — one or more handlers missing audit; handler names printed to stderr
#   2 — misuse / internal error

set -euo pipefail

ROOT="${ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
TOOLS_FILE="${ROOT}/mcp/chat-mcp/src/tools.rs"
ALLOWLIST="${ROOT}/tools/scripts/unaudited-persona-tool-allowlist.txt"

if [[ ! -f "$TOOLS_FILE" ]]; then
    echo "error: tools file not found: $TOOLS_FILE" >&2
    exit 2
fi

# Load allowlist entries (handler suffixes like _list, _recent_actions).
ALLOW_ENTRIES=""
if [[ -f "$ALLOWLIST" ]]; then
    ALLOW_ENTRIES="$(grep -vE '^\s*(#|$)' "$ALLOWLIST" | sed -E 's/[[:space:]]*#.*$//' | sed -E 's/^[[:space:]]+|[[:space:]]+$//g' || true)"
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
flagged_file="$tmpdir/flagged"
: > "$flagged_file"

# Use awk to:
# 1. Detect `fn handle_meta_persona_*` function declarations.
# 2. Accumulate lines until the next such declaration (or EOF).
# 3. Check the accumulated body for `audit(` or `record_persona_audit(`.
# 4. Also check for an inline allowlist comment.
# Emit handler suffixes that are missing both.

awk '
function flush(name, body,    has_audit, has_inline_allow) {
    if (name == "") return
    has_audit = (body ~ /[^_]audit\(mem,/ || body ~ /record_persona_audit\(/)
    has_inline_allow = (body ~ /poly-lint: allow unaudited-persona-tool/)
    if (!has_audit && !has_inline_allow) {
        print name
    }
}

/fn handle_meta_persona_/ {
    # Flush the previous function.
    flush(current_name, current_body)
    # Extract the handler name: fn handle_meta_persona_<suffix>(
    match($0, /fn handle_meta_persona_([A-Za-z0-9_]+)/, arr)
    current_name = arr[1]
    current_body = $0 "\n"
    next
}

current_name != "" {
    current_body = current_body $0 "\n"
}

END {
    flush(current_name, current_body)
}
' "$TOOLS_FILE" > "$flagged_file" || true

unallowed=0
final_file="$tmpdir/final"
: > "$final_file"

while IFS= read -r suffix; do
    [[ -z "$suffix" ]] && continue
    matched=0
    while IFS= read -r entry; do
        [[ -z "$entry" ]] && continue
        if [[ "$entry" == "_${suffix}" || "$entry" == "${suffix}" ]]; then
            matched=1; break
        fi
    done <<< "$ALLOW_ENTRIES"
    if [[ "$matched" -eq 0 ]]; then
        echo "$suffix" >> "$final_file"
        unallowed=$((unallowed + 1))
    fi
done < "$flagged_file"

if [[ "$unallowed" -eq 0 ]]; then
    exit 0
fi

while IFS= read -r suffix; do
    cat >&2 <<EOF
error: handle_meta_persona_${suffix} has no audit() or record_persona_audit() call
This is persona privacy class P2 — every state-changing MCP handler must
write an audit row so the persona audit trail is complete. Add:
  audit(mem, slug, "invoke", Some("{\"action\":\"${suffix}\"}"), "ok", None);
on the success path, or explain why auditing is not needed and add:
  _${suffix}
to tools/scripts/unaudited-persona-tool-allowlist.txt with a rationale.
See: docs/plans/plan-persona-quality-gates.md Phase Q.2.

EOF
done < "$final_file"

echo "error: ${unallowed} unaudited handle_meta_persona_* handler(s) in tools.rs" >&2
exit 1
