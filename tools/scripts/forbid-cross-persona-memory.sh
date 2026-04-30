#!/usr/bin/env bash
# forbid-cross-persona-memory.sh — Phase Q.1 of plan-persona-quality-gates.md.
#
# Scans `mcp/chat-mcp/src/` for SELECT / DELETE / UPDATE queries against
# any persona-scoped table (persona_facts, persona_audit, persona_sources,
# persona_tool_whitelist, persona_outbound_allowlist) that do NOT include a
# `persona_slug` binding within 10 lines of the query string.
#
# Missing `WHERE persona_slug = ?` is privacy class P1 — a cross-persona
# memory leak: returning or deleting rows belonging to a different persona.
#
# Usage:
#   ./tools/scripts/forbid-cross-persona-memory.sh
#   # Override scan root:
#   ROOT=/tmp/some-worktree ./tools/scripts/forbid-cross-persona-memory.sh
#
# Inline allowlist: append the following comment to the end of an offending
# line (replace <reason> with a short rationale):
#   // poly-lint: allow cross-persona-memory — <reason>
#
# File-level allowlist: tools/scripts/cross-persona-memory-allowlist.txt
# Format: path:line_start-line_end # reason   OR   path # whole-file allow
#
# Exit codes:
#   0 — clean (no unallowlisted hits)
#   1 — one or more unallowlisted hits; details printed to stderr
#   2 — misuse / internal error

set -euo pipefail

ROOT="${ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
SCAN_DIR="${ROOT}/mcp/chat-mcp/src"
ALLOWLIST="${ROOT}/tools/scripts/cross-persona-memory-allowlist.txt"

# Tables that must be accessed with a persona_slug WHERE clause.
PERSONA_TABLES="persona_facts|persona_audit|persona_sources|persona_tool_whitelist|persona_outbound_allowlist"

# Window of lines to check after the query line for persona_slug binding.
SLUG_WINDOW=10

if [[ ! -d "$SCAN_DIR" ]]; then
    echo "error: scan dir not found: $SCAN_DIR" >&2
    exit 2
fi

# Load file-level allowlist entries (path or path:line_start-line_end).
ALLOW_ENTRIES=""
if [[ -f "$ALLOWLIST" ]]; then
    ALLOW_ENTRIES="$(grep -vE '^\s*(#|$)' "$ALLOWLIST" | sed -E 's/[[:space:]]*#.*$//' | sed -E 's/^[[:space:]]+|[[:space:]]+$//g' || true)"
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
hits_file="$tmpdir/hits"
: > "$hits_file"

# Scan each .rs file.
# For every SELECT / DELETE / UPDATE targeting a persona-scoped table,
# look at the 10 lines following (inclusive) for `persona_slug`.
# Skip lines with the inline allowlist comment.
while IFS= read -r -d '' f; do
    awk -v path="$f" \
        -v tables="$PERSONA_TABLES" \
        -v window="$SLUG_WINDOW" '
    {
        lines[NR] = $0
    }
    END {
        # Build a regex for the tables.
        tpat = "SELECT|DELETE|UPDATE"
        for (i = 1; i <= NR; i++) {
            line = lines[i]
            # Skip inline-allowlisted lines.
            if (index(line, "poly-lint: allow cross-persona-memory") > 0) continue
            # Check if this line has a DML verb targeting a persona table.
            if (line !~ (tpat)) continue
            if (line !~ tables) continue
            # Look for persona_slug in [i, i+window].
            found_slug = 0
            last = i + window
            if (last > NR) last = NR
            for (j = i; j <= last; j++) {
                if (index(lines[j], "poly-lint: allow cross-persona-memory") > 0) {
                    found_slug = 1; break
                }
                if (lines[j] ~ /persona_slug/) { found_slug = 1; break }
            }
            if (!found_slug) {
                printf "%s\t%d\t%s\n", path, i, line
            }
        }
    }
    ' "$f" >> "$hits_file" || true
done < <(find "$SCAN_DIR" -type f -name '*.rs' -print0)

# Allowlist check against file-level entries.
unallowed=0
flagged_file="$tmpdir/flagged"
: > "$flagged_file"

while IFS=$'\t' read -r hit_path hit_line hit_raw; do
    [[ -z "$hit_path" ]] && continue
    rel_path="${hit_path#$ROOT/}"
    matched=0
    while IFS= read -r entry; do
        [[ -z "$entry" ]] && continue
        # Whole-file allow.
        if [[ "$entry" == "$rel_path" ]]; then matched=1; break; fi
        # Range allow: path:start-end
        if [[ "$entry" == "$rel_path":* ]]; then
            range="${entry#$rel_path:}"
            if [[ "$range" =~ ^([0-9]+)-([0-9]+)$ ]]; then
                lo="${BASH_REMATCH[1]}"
                hi="${BASH_REMATCH[2]}"
                if (( hit_line >= lo && hit_line <= hi )); then matched=1; break; fi
            elif [[ "$range" == "$hit_line" ]]; then
                matched=1; break
            fi
        fi
    done <<< "$ALLOW_ENTRIES"

    if [[ "$matched" -eq 0 ]]; then
        printf '%s\t%s\t%s\n' "$rel_path" "$hit_line" "$hit_raw" >> "$flagged_file"
        unallowed=$((unallowed + 1))
    fi
done < "$hits_file"

if [[ "$unallowed" -eq 0 ]]; then
    exit 0
fi

# Emit a clear diagnostic for every flagged hit.
while IFS=$'\t' read -r rel_path hit_line hit_raw; do
    cat >&2 <<EOF
error: cross-persona memory access detected at ${rel_path}:${hit_line}
  ${hit_raw}
This is persona privacy class P1 — a SELECT/DELETE/UPDATE against a
persona-scoped table without a nearby \`persona_slug\` binding leaks or
deletes rows belonging to another persona. Add \`WHERE persona_slug = ?\`
to the query, or explain why it is safe and add the site to:
  tools/scripts/cross-persona-memory-allowlist.txt
or add the inline comment:
  // poly-lint: allow cross-persona-memory — <reason>
See: docs/plans/plan-persona-quality-gates.md Phase Q.1.

EOF
done < "$flagged_file"

echo "error: ${unallowed} unallowlisted cross-persona memory access(es) in mcp/chat-mcp/src" >&2
exit 1
