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

# Algorithm (implemented in awk, per-file):
#
# For each candidate `let <var> = <sig>.read();` line at line `i`:
#
#   1. Skip if the line has `// poly-lint: allow long-read-guard`.
#   2. Walk forward from line i tracking:
#      - relative brace depth (start at 0; +1 for each `{`, -1 for each `}`,
#        excluding braces inside `//` comments and string literals on that line)
#      - whether `drop(<var>);` has appeared (then `<var>` is dead)
#      - whether `<sig>.(batch|write|set|pending_update)(` has appeared
#   3. STOP walking when ANY of:
#      - relative depth goes negative → enclosing block closed, guard dropped
#        at the `}`. Anything after is safe. NOT A HIT.
#      - `drop(<var>);` seen → guard explicitly dropped. NOT A HIT.
#      - `<sig>.batch/.write/.set/.pending_update(` seen WHILE depth >= 0 AND
#        no drop yet → guard is live across a write on the same signal. HIT.
#      - 60-line window exceeded with neither end-of-scope nor write → not a
#        hit (avoid scanning the whole file).
#
# This catches the real bug pattern (read held live across a write of the
# same signal) and recognises BOTH safe patterns:
#   - `let X = { let g = sig.read(); ...field extraction... };` (block-scoped
#     read, dropped at the `};` of the outer block)
#   - `let g = sig.read(); ... drop(g); ... sig.batch(...);` (explicit drop)
#
# No line-anchored allowlist is needed for either safe pattern.

while IFS= read -r -d '' f; do
    awk -v path="$f" '
    {
        lines[NR] = $0
    }

    # Strip line and block comments + string literals from a line so braces
    # inside them are not counted toward depth. Approximate: handles `// ...`
    # tail comments and naive `"..."` strings; does not handle `/* */` block
    # comments spanning multiple lines (rare in this codebase outside doc
    # comments, which use `///` and `//!`).
    function strip(line,    s) {
        s = line
        # Drop line comments.
        sub(/\/\/.*$/, "", s)
        # Drop simple string literals (non-greedy approximation).
        gsub(/"[^"]*"/, "", s)
        return s
    }

    function count_braces(line,    s, opens, closes, n, ch) {
        s = strip(line)
        opens = 0; closes = 0
        n = length(s)
        for (k = 1; k <= n; k++) {
            ch = substr(s, k, 1)
            if (ch == "{") opens++
            else if (ch == "}") closes++
        }
        return opens - closes
    }

    END {
        for (i = 1; i <= NR; i++) {
            raw = lines[i]
            trimmed = raw
            sub(/^[[:space:]]+/, "", trimmed)

            # Must be a bare `let <var> = <sig>.read();` pattern:
            #   - starts with "let " (allow "let mut ")
            #   - signal-side expression ends with `.read();` (no trailing chain)
            if (trimmed !~ /^let (mut )?[A-Za-z_][A-Za-z0-9_]* = [A-Za-z_][A-Za-z0-9_.]*[.]read[(][)];/) {
                continue
            }

            # Inline allowlist: skip if line carries the magic comment.
            if (raw ~ /\/\/ poly-lint: allow long-read-guard/) {
                continue
            }

            # Extract variable name and signal-receiver name.
            rest = trimmed
            sub(/^let (mut )?/, "", rest)
            split(rest, parts, " = ")
            var_name = parts[1]
            rest2 = parts[2]
            sub(/\.read\(\);.*$/, "", rest2)
            sig_name = rest2

            # Walk forward tracking brace depth + drop + write events.
            depth = 0
            limit = i + 60
            if (limit > NR) limit = NR
            hit = 0
            for (j = i + 1; j <= limit; j++) {
                line_j = lines[j]

                # Explicit drop ends scope cleanly — not a hit.
                if (line_j ~ "drop[(][[:space:]]*" var_name "[[:space:]]*[)]") {
                    break
                }

                # Write on the same signal — only a hit while guard is live
                # (depth >= 0). If depth has already gone negative the enclosing
                # block closed before we got here and `var_name` is gone.
                if (depth >= 0 && line_j ~ sig_name "[.](batch|write|set|pending_update)[(]") {
                    hit = 1
                    break
                }

                depth += count_braces(line_j)

                # Enclosing scope closed — guard dropped at the `}`. Safe.
                if (depth < 0) break
            }

            if (!hit) continue

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
