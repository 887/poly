#!/usr/bin/env bash
# forbid-raw-backend-read.sh — Phase 5 Track A of plan-backend-read-timeout.md.
#
# Scans `crates/core/src/ui/**/*.rs` for raw `backend.read().await` calls.
# Raw `RwLock::read().await` (CLAUDE.md hang class #4) starves readers on the
# single-threaded WASM scheduler when a perpetual writer holds the lock.
# The correct replacement is `BackendHandleExt::read_with_timeout`:
#   crates/core/src/client_manager_timeout.rs
#
# Usage:
#   ./tools/scripts/forbid-raw-backend-read.sh
#   # Override scan root:
#   ROOT=/tmp/some-worktree ./tools/scripts/forbid-raw-backend-read.sh
#
# Regression test — the script must flag this line:
#   let g = backend.read().await;
# Run: echo '    let g = backend.read().await;' | \
#   grep -nP 'backend\.read\(\)\.await' /dev/stdin
# Verify exit 1.
#
# Inline allowlist: append the following comment to the end of an offending
# line (replace <reason> with a short rationale):
#   // poly-lint: allow raw backend.read().await — <reason>
#
# File-level allowlist: tools/scripts/raw-backend-read-allowlist.txt
# Format: path:line_start-line_end # reason   OR   path # whole-file allow
#
# Exit codes:
#   0 — clean (no unallowlisted hits)
#   1 — one or more unallowlisted hits; details printed to stderr
#   2 — misuse / internal error

set -euo pipefail

ROOT="${ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
SCAN_DIR="${ROOT}/crates/core/src/ui"
# Phase Q.4 of plan-persona-quality-gates.md: also scan the persona builder
# in chat-mcp, which issues backend calls on persona's behalf and must use
# the same timeout discipline as the UI crate.
SCAN_DIR_PERSONA="${ROOT}/mcp/chat-mcp/src/persona"
ALLOWLIST="${ROOT}/tools/scripts/raw-backend-read-allowlist.txt"

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

# Scan: emit path<TAB>line<TAB>content for every raw backend.read().await hit.
# A line is skipped (inline-allowlisted) if it contains the magic comment:
#   // poly-lint: allow raw backend.read().await
while IFS= read -r -d '' f; do
    grep -nP 'backend\.read\(\)\.await' "$f" \
        | grep -vF '// poly-lint: allow raw backend.read().await' \
        | while IFS=: read -r lineno content; do
            printf '%s\t%s\t%s\n' "$f" "$lineno" "$content"
          done >> "$hits_file" || true
done < <(
    find "$SCAN_DIR" -type f -name '*.rs' -print0
    if [[ -d "$SCAN_DIR_PERSONA" ]]; then
        find "$SCAN_DIR_PERSONA" -type f -name '*.rs' -print0
    fi
)

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
        # Whole-file allow: exact path match.
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
error: forbidden raw backend.read().await detected at ${rel_path}:${hit_line}
  ${hit_raw}
This is CLAUDE.md hang class #4 (RwLock starvation on WASM). Use
BackendHandleExt::read_with_timeout(Duration::from_secs(5)) instead:
  crates/core/src/client_manager_timeout.rs
  docs/plans/plan-backend-read-timeout.md
If this is a genuine exception, append
  // poly-lint: allow raw backend.read().await — <reason>
to the end of the line.

EOF
done < "$flagged_file"

echo "error: ${unallowed} unallowlisted raw backend.read().await hit(s) in crates/core/src/ui + mcp/chat-mcp/src/persona" >&2
exit 1
