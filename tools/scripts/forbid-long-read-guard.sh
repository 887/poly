#!/usr/bin/env bash
# forbid-long-read-guard.sh — Phase 5 Track A of plan-read-guard-scoping.md.
#
# Scans crates/core/src/ui/**/*.rs for long-scoped Signal::read() guards:
# bare `let <var> = <sig>.read();` bindings (no trailing field/clone/method
# chain) where the binding is still live when a `.batch(`, `.write(`,
# `.set(`, or `.pending_update(` call appears within 30 lines after the
# `let`, AND `<var>` is referenced more than 6 lines after the `let`.
#
# This is CLAUDE.md hang class #2 (read guard live across a write). A live
# read guard and a write on the same signal in the WASM single-threaded
# scheduler causes a reactive cycle or deadlock.
#
# Inline allowlist:
#   Add `// poly-lint: allow long-read-guard — <reason>` on the `let` line.
#
# File-level / line-level allowlist:
#   tools/scripts/long-read-guard-allowlist.txt — `file:line # reason`
#
# Usage:
#   ./tools/scripts/forbid-long-read-guard.sh
#   # Optional: override the scan root
#   ROOT=/tmp/some-worktree ./tools/scripts/forbid-long-read-guard.sh
#
# Exit codes:
#   0 — clean (no unallowlisted hits)
#   1 — one or more unallowlisted hits; details printed to stderr
#   2 — misuse / internal error

set -euo pipefail

ROOT="${ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
SCAN_DIR="${ROOT}/crates/core/src/ui"
ALLOWLIST="${ROOT}/tools/scripts/long-read-guard-allowlist.txt"

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

# Heuristic (implemented in awk, per-file):
#
# For each `let <var> = <sig>.read();` line (bare guard — no trailing
# `.field` / `.clone()` / `.method()` after the closing paren):
#
#   1. Check that the line does NOT contain `// poly-lint: allow long-read-guard`.
#   2. Scan the next 30 lines for `<sig>.(batch|write|set|pending_update)(`.
#   3. Check that `<var>` appears in any line more than 6 lines after the `let`.
#
# If all three conditions are met, emit a hit record:
#   path \t lineno \t offending_line
#
while IFS= read -r -d '' f; do
    awk -v path="$f" '
    {
        lines[NR] = $0
    }

    END {
        for (i = 1; i <= NR; i++) {
            raw = lines[i]
            trimmed = raw
            sub(/^[[:space:]]+/, "", trimmed)

            # Must be a bare `let <var> = <sig>.read();` pattern:
            #   - starts with "let "
            #   - contains ".read();"  (exactly, no trailing chain)
            #   - the .read() must be at the end of the expression (followed
            #     by semicolon, optional whitespace)
            if (trimmed !~ /^let [A-Za-z_][A-Za-z0-9_]* = [A-Za-z_][A-Za-z0-9_]*[.]read[(][)];/) {
                continue
            }

            # Inline allowlist: skip if line contains the magic comment.
            if (raw ~ /\/\/ poly-lint: allow long-read-guard/) {
                continue
            }

            # Extract variable name and signal name.
            rest = trimmed
            sub(/^let /, "", rest)
            split(rest, parts, " = ")
            var_name = parts[1]

            rest2 = parts[2]
            sub(/\.read\(\);.*$/, "", rest2)
            sig_name = rest2

            # Condition 2: scan next 30 lines for sig.(batch|write|set|pending_update)(
            write_found = 0
            write_line = 0
            limit = i + 30
            if (limit > NR) limit = NR
            for (j = i + 1; j <= limit; j++) {
                if (lines[j] ~ sig_name "[.](batch|write|set|pending_update)[(]") {
                    write_found = 1
                    write_line = j
                    break
                }
            }
            if (!write_found) continue

            # Condition 3: var_name appears more than 6 lines after the let.
            var_late = 0
            for (j = i + 7; j <= NR; j++) {
                if (lines[j] ~ var_name) {
                    # Make sure it is a real reference, not a redeclaration
                    # and not inside a comment-only line.
                    chk = lines[j]
                    sub(/^[[:space:]]+/, "", chk)
                    if (chk ~ /^\/\//) continue
                    var_late = 1
                    break
                }
            }
            if (!var_late) continue

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
error: long-scoped Signal::read() guard detected at ${rel_path}:${hit_line}
  ${hit_raw}
This is CLAUDE.md hang class #2 (read guard live across a write).
Use BatchedSignal::with(|v| ...) for closure-scoped reads, or wrap in
an explicit { let g = sig.read(); ... } block. See:
  crates/core/src/state/batched_signal.rs
  docs/plans/plan-read-guard-scoping.md
Inline-allowlist: // poly-lint: allow long-read-guard — <reason>

EOF
done < "$flagged_file"

echo "error: ${unallowed} unallowlisted long-scoped Signal::read() guard(s) in crates/core/src/ui" >&2
exit 1
