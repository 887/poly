#!/usr/bin/env bash
# forbid-unaudited-persona-tool.sh — Phase Q.2 of plan-persona-quality-gates.md.
#
# Greps `mcp/chat-mcp/src/tools.rs` for every `fn handle_meta_persona_*` AND
# `fn handle_client_settings_*` function and asserts each calls the appropriate
# audit helper at least once in its body.
#
# For meta_persona handlers:
#   Acceptable:  audit(mem,   |  record_persona_audit(
# For client_settings handlers:
#   Acceptable:  audit_client_settings(   |  record_client_settings_audit(
#
# Read-only handlers (returning data without mutating state) are allowlisted in
# their respective allowlist files.
#
# Missing an audit call is class P2 — a state-changing MCP handler that skips
# writing an audit row makes the audit trail incomplete and forensically
# unreliable.
#
# Usage:
#   ./tools/scripts/forbid-unaudited-persona-tool.sh
#   # Override scan root:
#   ROOT=/tmp/some-worktree ./tools/scripts/forbid-unaudited-persona-tool.sh
#
# Inline allowlist: add the following comment anywhere in the function body:
#   // poly-lint: allow unaudited-persona-tool — <reason>
#
# File-level allowlists:
#   tools/scripts/unaudited-persona-tool-allowlist.txt        (meta_persona_*)
#   tools/scripts/unaudited-client-settings-tool-allowlist.txt (client_settings_*)
# Format: handler_suffix  # e.g.  _list   or   _get_version
#   (suffix is the part after `handle_meta_persona` / `handle_client_settings`)
#
# Exit codes:
#   0 — clean (all state-changing handlers have audit calls)
#   1 — one or more handlers missing audit; handler names printed to stderr
#   2 — misuse / internal error

set -euo pipefail

ROOT="${ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
TOOLS_FILE="${ROOT}/mcp/chat-mcp/src/tools.rs"
PERSONA_ALLOWLIST="${ROOT}/tools/scripts/unaudited-persona-tool-allowlist.txt"
CLIENT_ALLOWLIST="${ROOT}/tools/scripts/unaudited-client-settings-tool-allowlist.txt"

if [[ ! -f "$TOOLS_FILE" ]]; then
    echo "error: tools file not found: $TOOLS_FILE" >&2
    exit 2
fi

# ── Helper: load allowlist ─────────────────────────────────────────────────────
load_allowlist() {
    local file="$1"
    if [[ -f "$file" ]]; then
        grep -vE '^\s*(#|$)' "$file" \
            | sed -E 's/[[:space:]]*#.*$//' \
            | sed -E 's/^[[:space:]]+|[[:space:]]+$//g' \
            || true
    fi
}

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

PERSONA_FLAGGED="$tmpdir/persona_flagged"
CLIENT_FLAGGED="$tmpdir/client_flagged"

# ── Scan meta_persona handlers ────────────────────────────────────────────────
# Detects fn handle_meta_persona_* and checks for audit( or record_persona_audit(.
# Body collection stops when the next top-level fn/mod/impl/test boundary is hit
# so the last handler isn't contaminated by subsequent definitions.
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
    flush(current_name, current_body)
    match($0, /fn handle_meta_persona_([A-Za-z0-9_]+)/, arr)
    current_name = arr[1]
    current_body = $0 "\n"
    next
}

# Stop body accumulation at section boundaries (top-level fn/mod/impl/#[cfg]).
current_name != "" && /^(fn |async fn |pub fn |pub async fn |mod |impl |#\[cfg)/ {
    flush(current_name, current_body)
    current_name = ""
    current_body = ""
    # Fall through so this line is not lost (it might be a new target fn, handled above).
}

current_name != "" {
    current_body = current_body $0 "\n"
}

END {
    flush(current_name, current_body)
}
' "$TOOLS_FILE" > "$PERSONA_FLAGGED" || true

# ── Scan client_settings handlers ─────────────────────────────────────────────
# Detects fn handle_client_settings_* and checks for audit_client_settings( or
# record_client_settings_audit( on NON-COMMENT lines (commented-out calls do not
# count). Read-only handlers allowlisted below.
# Body collection stops when a top-level boundary is hit.
awk '
function flush(name, audit_found, inline_found) {
    if (name == "") return
    if (!audit_found && !inline_found) {
        print name
    }
}

/fn handle_client_settings_/ {
    flush(current_name, current_audit, current_inline)
    match($0, /fn handle_client_settings_([A-Za-z0-9_]+)/, arr)
    current_name = arr[1]
    current_audit = 0
    current_inline = 0
    next
}

# Stop body accumulation at section boundaries: top-level fn/mod/impl,
# #[cfg blocks, or the section-separator comment style used in this file.
current_name != "" && /^(fn |async fn |pub fn |pub async fn |mod |impl |#\[|\/\/ )/ {
    flush(current_name, current_audit, current_inline)
    current_name = ""
    current_audit = 0
    current_inline = 0
}

# Check each non-comment line for an audit call.
current_name != "" && !/^[[:space:]]*\/\// {
    if ($0 ~ /audit_client_settings\(/ || $0 ~ /record_client_settings_audit\(/) {
        current_audit = 1
    }
}

# Inline allowlist comment (also ok in // lines).
current_name != "" && /poly-lint: allow unaudited-persona-tool/ {
    current_inline = 1
}

END {
    flush(current_name, current_audit, current_inline)
}
' "$TOOLS_FILE" > "$CLIENT_FLAGGED" || true

# ── Filter against allowlists and report ──────────────────────────────────────
unallowed=0
final_file="$tmpdir/final"
: > "$final_file"

PERSONA_ALLOW="$(load_allowlist "$PERSONA_ALLOWLIST")"
CLIENT_ALLOW="$(load_allowlist "$CLIENT_ALLOWLIST")"

check_against_allowlist() {
    local suffix="$1"
    local allow_entries="$2"
    local family_prefix="$3"   # "handle_meta_persona_" or "handle_client_settings_"

    [[ -z "$suffix" ]] && return
    local matched=0
    while IFS= read -r entry; do
        [[ -z "$entry" ]] && continue
        if [[ "$entry" == "_${suffix}" || "$entry" == "${suffix}" ]]; then
            matched=1; break
        fi
    done <<< "$allow_entries"
    if [[ "$matched" -eq 0 ]]; then
        echo "${family_prefix}${suffix}" >> "$final_file"
        unallowed=$((unallowed + 1))
    fi
}

while IFS= read -r suffix; do
    check_against_allowlist "$suffix" "$PERSONA_ALLOW" "handle_meta_persona_"
done < "$PERSONA_FLAGGED"

while IFS= read -r suffix; do
    check_against_allowlist "$suffix" "$CLIENT_ALLOW" "handle_client_settings_"
done < "$CLIENT_FLAGGED"

if [[ "$unallowed" -eq 0 ]]; then
    exit 0
fi

while IFS= read -r handler; do
    cat >&2 <<EOF
error: ${handler} has no audit call
Every state-changing MCP handler must write an audit row so the audit trail
is complete. For meta_persona_* handlers add:
  audit(mem, slug, "invoke", Some("{\"action\":\"...\"}"), "ok", None);
For client_settings_* handlers add:
  audit_client_settings(mem, backend_id, "action", Some(&payload), "ok", None);
Or add the handler suffix to the appropriate allowlist file with a rationale.
See: docs/plans/plan-persona-quality-gates.md Phase Q.2 and
     docs/plans/plan-client-version-override-and-sandbox.md Phase D.6.

EOF
done < "$final_file"

echo "error: ${unallowed} unaudited handler(s) in tools.rs" >&2
exit 1
