#!/usr/bin/env bash
# forbid-stale-effect-capture.sh — Phase 5 Track A of
# docs/plans/plan-use-reactive-effect.md.
#
# FLAGS every `use_effect(move || {` block in crates/core/src/ui/**/*.rs.
#
# Strategy: conservative-default. Every raw `use_effect(move ||` call in
# the UI crate is a potential Hang #6 site (stale non-Signal closure
# capture). Rather than under-flag with an unreliable heuristic, we flag
# ALL occurrences and require an inline allowlist comment for any existing
# site that has been manually verified to be safe. Phase 2 of the plan
# migrates the HIGH-risk sites to use_reactive_effect / use_spawn_once,
# which will shrink the allowlist over time.
#
# Inline allowlist:
#   Add `// poly-lint: allow stale-effect-capture — <reason>` on the
#   `use_effect(move ||` line to suppress a single site.
#
# File-level / line-level allowlist:
#   tools/scripts/stale-effect-capture-allowlist.txt
#   Format: `file:line # reason`
#
# Usage:
#   ./tools/scripts/forbid-stale-effect-capture.sh
#   # Optional: override the scan root
#   ROOT=/tmp/some-worktree ./tools/scripts/forbid-stale-effect-capture.sh
#
# Exit codes:
#   0 — clean (no unallowlisted hits)
#   1 — one or more unallowlisted hits; details printed to stderr
#   2 — misuse / internal error

set -euo pipefail

ROOT="${ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
SCAN_DIR="${ROOT}/crates/core/src/ui"
ALLOWLIST="${ROOT}/tools/scripts/stale-effect-capture-allowlist.txt"

if [[ ! -d "$SCAN_DIR" ]]; then
    echo "error: scan dir not found: $SCAN_DIR" >&2
    exit 2
fi

# Build a normalised allowlist (strip comments + blank lines).
# Each line is: path:line # reason
if [[ -f "$ALLOWLIST" ]]; then
    ALLOW_ENTRIES="$(grep -vE '^\s*(#|$)' "$ALLOWLIST" | sed -E 's/[[:space:]]*#.*$//' | sed -E 's/^[[:space:]]+|[[:space:]]+$//g' || true)"
else
    ALLOW_ENTRIES=""
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
hits_file="$tmpdir/hits"
: > "$hits_file"

# For every `use_effect(move || {` line (the canonical Hang #6 pattern):
#   1. Skip lines with the inline allowlist token.
#   2. Emit a hit record: path \t lineno \t offending_line
while IFS= read -r -d '' f; do
    awk -v path="$f" '
    {
        lines[NR] = $0
    }
    END {
        for (i = 1; i <= NR; i++) {
            raw = lines[i]

            # Must match the use_effect(move || { pattern (with possible
            # leading whitespace and optional trailing text).
            if (raw !~ /use_effect\(move \|\|/) {
                continue
            }

            # Inline allowlist: skip if line contains the magic comment.
            if (raw ~ /\/\/ poly-lint: allow stale-effect-capture/) {
                continue
            }

            printf "%s\t%d\t%s\n", path, i, raw
        }
    }
    ' "$f" >> "$hits_file"
done < <(find "$SCAN_DIR" -type f -name '*.rs' -print0)

# Allowlist check.
unallowed=0
flagged_file="$tmpdir/flagged"
: > "$flagged_file"

while IFS=$'\t' read -r hit_path hit_line hit_raw; do
    [[ -z "$hit_path" ]] && continue
    rel_path="${hit_path#$ROOT/}"
    matched=0
    while IFS= read -r entry; do
        [[ -z "$entry" ]] && continue
        # Match "path:line" form.
        if [[ "$entry" == "$rel_path:$hit_line" ]]; then matched=1; break; fi
        # Match whole-file allow "path".
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

# Report every flagged hit with the standard message.
while IFS=$'\t' read -r rel_path hit_line hit_raw; do
    cat >&2 <<EOF
error: potentially-stale closure capture inside use_effect at ${rel_path}:${hit_line}
  ${hit_raw}
This is CLAUDE.md hang class #6 (use_effect captures non-Signal value
that drifts across re-renders). Use use_reactive_effect<Deps>(deps, body)
or use_spawn_once<K>(key, async_fn) instead. See:
  crates/core/src/state/use_reactive_effect.rs
  crates/core/src/state/use_spawn_once.rs
  docs/plans/plan-use-reactive-effect.md
Inline-allowlist: // poly-lint: allow stale-effect-capture — <reason>

EOF
done < "$flagged_file"

echo "error: ${unallowed} unallowlisted use_effect(move ||) site(s) in crates/core/src/ui" >&2
exit 1
