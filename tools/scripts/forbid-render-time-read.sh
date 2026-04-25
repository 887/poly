#!/usr/bin/env bash
# forbid-render-time-read.sh — Phases 1+2 of
# docs/plans/plan-peek-vs-read.md.
#
# FLAGS every `.read()` call in crates/core/src/ui/**/*.rs that is NOT
# clearly inside a safe closure or async body on the SAME LINE.
#
# Safe same-line patterns (subscription or async context — not a hang risk):
#   .read().await          — backend arc lock, covered by forbid-raw-backend-read
#   use_effect(move ||     — subscription IS the intent
#   use_resource(          — subscription IS the intent
#   use_memo(              — subscription IS the intent
#   spawn(async move       — inside an async body
#   onclick:               — event handler
#   on*: move |            — event handler
#
# For patterns that can't be distinguished by single-line regex (e.g., a
# `.read()` that happens to appear AFTER a `use_resource(` closure on a
# previous line), add a file:line entry to the allowlist instead.
#
# Strategy: conservative-default / over-flag.  Flag every `.read()` that
# is not clearly safe on the same line, then let the allowlist absorb
# legitimate render-time subscription sites (MEDIUM/LOW).  Over-flagging
# is fine; under-flagging is the bug.
#
# Inline allowlist:
#   `// poly-lint: allow render-time-read — <reason>` on the `.read()` line.
#
# File-level / line-level allowlist:
#   tools/scripts/render-time-read-allowlist.txt
#   Format: `file:line # reason`
#
# Usage:
#   ./tools/scripts/forbid-render-time-read.sh
#   ROOT=/tmp/some-worktree ./tools/scripts/forbid-render-time-read.sh
#
# Exit codes:
#   0 — clean (no unallowlisted hits)
#   1 — one or more unallowlisted hits; details printed to stderr
#   2 — misuse / internal error

set -euo pipefail

ROOT="${ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
SCAN_DIR="${ROOT}/crates/core/src/ui"
ALLOWLIST="${ROOT}/tools/scripts/render-time-read-allowlist.txt"

if [[ ! -d "$SCAN_DIR" ]]; then
    echo "error: scan dir not found: $SCAN_DIR" >&2
    exit 2
fi

# Build a normalised allowlist (strip comments + blank lines).
if [[ -f "$ALLOWLIST" ]]; then
    ALLOW_ENTRIES="$(grep -vE '^\s*(#|$)' "$ALLOWLIST" | sed -E 's/[[:space:]]*#.*$//' | sed -E 's/^[[:space:]]+|[[:space:]]+$//g' || true)"
else
    ALLOW_ENTRIES=""
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
hits_file="$tmpdir/hits"
: > "$hits_file"

# Scan: for every .read() line, emit a hit unless it is clearly safe on
# the same line (see safe-pattern list above).
while IFS= read -r -d '' f; do
    awk -v path="$f" '
    {
        raw = $0

        # Must contain .read()
        if (raw !~ /\.read\(\)/) { next }

        # Skip .read().await — backend arc, covered by separate lint.
        if (raw ~ /\.read\(\)\.await/) { next }

        # Skip inline allowlist token.
        if (raw ~ /\/\/ poly-lint: allow render-time-read/) { next }

        # Skip lines that are clearly inside a safe closure on the same line.
        # These patterns indicate the .read() is either intentionally reactive
        # (use_effect / use_resource / use_memo) or inside an async/event body.
        if (raw ~ /use_effect\(move \|\|/) { next }
        if (raw ~ /use_resource\(/) { next }
        if (raw ~ /use_memo\(/) { next }
        if (raw ~ /spawn\(async/) { next }

        # Skip event handlers (onclick:, oninput:, onchange:, etc.)
        if (raw ~ /\bon[a-z_]+:[[:space:]]*move[[:space:]]*\|/) { next }
        if (raw ~ /\bon[a-z_]+:[[:space:]]*\|/) { next }

        printf "%s\t%d\t%s\n", path, NR, raw
    }
    ' "$f" >> "$hits_file"
done < <(find "$SCAN_DIR" -type f -name '*.rs' -print0)

# Allowlist filter.
unallowed=0
flagged_file="$tmpdir/flagged"
: > "$flagged_file"

while IFS=$'\t' read -r hit_path hit_line hit_raw; do
    [[ -z "$hit_path" ]] && continue
    rel_path="${hit_path#$ROOT/}"
    matched=0
    while IFS= read -r entry; do
        [[ -z "$entry" ]] && continue
        if [[ "$entry" == "$rel_path:$hit_line" ]]; then matched=1; break; fi
        if [[ "$entry" == "$rel_path" ]]; then matched=1; break; fi
    done <<< "$ALLOW_ENTRIES"

    if [[ "$matched" -eq 0 ]]; then
        printf '%s\t%s\t%s\n' "$rel_path" "$hit_line" "$hit_raw" >> "$flagged_file"
        unallowed=$((unallowed + 1))
    fi
done < "$hits_file"

if [[ "$unallowed" -eq 0 ]]; then
    exit 0
fi

# Report every flagged hit.
while IFS=$'\t' read -r rel_path hit_line hit_raw; do
    cat >&2 <<EOF
error: render-time .read() at ${rel_path}:${hit_line}
  ${hit_raw}
This is CLAUDE.md hang class #7. A .read() at the top of a render body or
hook-setup function subscribes the parent component to every write of this
signal. If the value drives a use_spawn_once / use_reactive_effect key or a
.batch() call in the same body, you have the exact perpetual-rerender loop
that wedged Teams on server-switch (1408x re-render for 1 load_server_data).
Fix: use .peek() if you only need a snapshot, not a reactive subscription.
See docs/dev/reactive-state.md §"When to use .peek() vs .read()".
Inline-allowlist: // poly-lint: allow render-time-read — <reason>
File-level allowlist: tools/scripts/render-time-read-allowlist.txt

EOF
done < "$flagged_file"

echo "error: ${unallowed} unallowlisted render-time .read() site(s) in crates/core/src/ui" >&2
exit 1
