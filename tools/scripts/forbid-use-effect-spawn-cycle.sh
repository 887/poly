#!/usr/bin/env bash
#
# forbid-use-effect-spawn-cycle.sh — Phase 5 Track A lint
#
# Scans `crates/core/src/ui/**/*.rs` for the hang-class-#3 triple:
#
#   use_effect(move || {
#       ...
#       spawn(async move {
#           ...
#           signal.batch|write|set|pending_update(...)
#           ...
#       })
#   })
#
# This is the canonical shape of CLAUDE.md "Common WASM-hang causes" #3
# (infinite spawn loop via a `use_effect` that subscribes to the same
# signal an inner spawned future writes). The hook
# `crates/core/src/state/use_spawn_once.rs` codifies the safe keyed
# variant of this pattern; new call-sites MUST use the hook.
#
# On a match the script exits 1 with a pointer to `use_spawn_once` and
# to this file. Genuine exceptions (debounced effects, multi-key cases,
# etc.) go in `tools/scripts/use-effect-spawn-cycle-allowlist.txt` with
# a `#` comment explaining the rationale.
#
# Usage:
#   tools/scripts/forbid-use-effect-spawn-cycle.sh        # scan, exit 1 on violation
#   tools/scripts/forbid-use-effect-spawn-cycle.sh --list # list every triple regardless of allowlist
#
# Heuristic notes:
# - Brace-matching is done on `{` and `}` counts inside the matched
#   block — good enough for well-formatted Rust, imperfect in the
#   presence of `{` inside strings/comments. False positives are
#   acceptable (allowlist them); silent false negatives are not.
# - Matches are considered "same function body" if the spawn appears
#   before the effect's closing `});`. A spawn that follows the
#   effect is not flagged.
# - Only write-family method calls on a `.method_name(` receiver are
#   flagged — `.read()`, `.peek()`, `.cloned()` etc. are ignored.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

SCAN_ROOT="crates/core/src/ui"
ALLOWLIST="tools/scripts/use-effect-spawn-cycle-allowlist.txt"

LIST_ONLY=0
if [ "${1:-}" = "--list" ]; then
    LIST_ONLY=1
fi

if [ ! -d "$SCAN_ROOT" ]; then
    echo "error: $SCAN_ROOT not found (run from repo root)" >&2
    exit 2
fi

# ---- Allowlist loader ------------------------------------------------------
# Format: one `path:line` entry per line, `#`-comments and blanks ignored.
# The `line` is the line of the offending `use_effect(move ||`.
# Matching is exact file-path + exact use_effect start line.

allowlisted() {
    local path=$1 line=$2
    [ -f "$ALLOWLIST" ] || return 1
    awk -v p="$path" -v l="$line" '
        /^[[:space:]]*#/ { next }
        /^[[:space:]]*$/ { next }
        {
            # Split at the last colon so paths containing colons still work.
            n = split($0, parts, ":")
            if (n < 2) next
            entry_line = parts[n]
            entry_path = parts[1]
            for (i = 2; i < n; i++) entry_path = entry_path ":" parts[i]
            # Strip trailing whitespace / inline comments from entry_line.
            sub(/[[:space:]].*$/, "", entry_line)
            sub(/#.*$/, "", entry_line)
            if (entry_path == p && entry_line == l) { found = 1; exit }
        }
        END { exit (found ? 0 : 1) }
    ' "$ALLOWLIST"
}

# ---- Core scanner ----------------------------------------------------------
# For each `use_effect(move ||` occurrence, compute the matching closing
# brace line. Then inside that span look for `spawn(async move {` (or
# `tokio::spawn(async move {`), find that inner block's closing brace,
# and look for `.batch(`, `.write(`, `.set(`, `.pending_update(` inside
# the inner block.
#
# Output per violation: PATH:LINE
# `--list` mode prints every triple; default mode prints only the
# non-allowlisted ones.

scan_file() {
    local file=$1
    awk -v file="$file" '
        BEGIN { effect_active = 0; effect_depth = 0; spawn_depth = 0; spawn_active = 0 }
        {
            line = $0
            # Strip // line comments to reduce false-positive brace counts.
            # (Block comments /* ... */ are not handled — rare in this codebase.)
            gsub(/\/\/[^\n]*$/, "", line)

            if (!effect_active) {
                # Look for the start of a use_effect(move || block.
                if (match(line, /use_effect\(move[[:space:]]*\|\|/)) {
                    effect_active = 1
                    effect_start_line = NR
                    effect_depth = 0
                    spawn_active = 0
                    spawn_depth = 0
                    found_spawn_and_write = 0
                    found_spawn = 0
                    found_write = 0
                    # Process the remainder of this line AFTER the match
                    # so an inline `use_effect(move || { ... });` works.
                    rest = substr(line, RSTART + RLENGTH)
                    process_line(rest)
                    next
                }
            } else {
                process_line(line)
            }
        }
        function process_line(text,    i, c, inside_inner) {
            # Walk characters tracking brace depth. Detect spawn-open.
            # We treat the effect as a single counter: first `{` after
            # `use_effect(move ||` opens depth 1; matching `}` closes
            # back to 0 → effect ends.
            # Inside the effect, if we see `spawn(async move {` we mark
            # spawn_active=1 at depth spawn_depth_start = current depth+1.
            n = length(text)
            i = 1
            while (i <= n) {
                c = substr(text, i, 1)
                if (c == "{") {
                    effect_depth++
                    if (spawn_active && !spawn_initialized) {
                        spawn_depth_start = effect_depth
                        spawn_initialized = 1
                    }
                    i++
                    continue
                }
                if (c == "}") {
                    effect_depth--
                    if (spawn_active && spawn_initialized && effect_depth < spawn_depth_start) {
                        # Inner spawn block closed.
                        spawn_active = 0
                        spawn_initialized = 0
                    }
                    if (effect_depth <= 0) {
                        # use_effect block ended.
                        if (found_spawn && found_write) {
                            printf "%s:%d\n", file, effect_start_line
                        }
                        effect_active = 0
                        spawn_active = 0
                        spawn_initialized = 0
                        return
                    }
                    i++
                    continue
                }
                # Look for spawn-open keyword starting at i.
                tail = substr(text, i)
                if (!spawn_active) {
                    if (match(tail, /^spawn\([[:space:]]*async[[:space:]]+move[[:space:]]*\{/) ||
                        match(tail, /^tokio::spawn\([[:space:]]*async[[:space:]]+move[[:space:]]*\{/)) {
                        found_spawn = 1
                        spawn_active = 1
                        spawn_initialized = 0
                        # Advance past everything up to (but not including) the `{`
                        # so the brace-counter picks it up on the next iteration.
                        brace_off = index(tail, "{")
                        i = i + brace_off - 1
                        continue
                    }
                }
                if (spawn_active && spawn_initialized) {
                    # Inside spawn body — look for write-family calls.
                    # We require a leading `.` or a direct receiver to
                    # reduce false positives from unrelated tokens like
                    # `set(` as a free function name. Matches like:
                    #   signal.batch(...)
                    #   signal.write()
                    #   signal.set(...)
                    #   signal.pending_update(...)
                    if (match(tail, /\.batch\(/) && RSTART == 1) found_write = 1
                    else if (match(tail, /\.write\(\)/) && RSTART == 1) found_write = 1
                    else if (match(tail, /\.write\([^)]/) && RSTART == 1) found_write = 1
                    else if (match(tail, /\.set\(/) && RSTART == 1) found_write = 1
                    else if (match(tail, /\.pending_update\(/) && RSTART == 1) found_write = 1
                }
                i++
            }
        }
    ' "$file"
}

# ---- Main loop -------------------------------------------------------------

violations=0
total=0

# git ls-files falls back to find if not in a git repo.
file_list() {
    if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        git ls-files "$SCAN_ROOT/**/*.rs" "$SCAN_ROOT/*.rs" 2>/dev/null
    else
        find "$SCAN_ROOT" -type f -name '*.rs'
    fi
}

while IFS= read -r rs_file; do
    [ -n "$rs_file" ] || continue
    while IFS=: read -r path line; do
        [ -n "${line:-}" ] || continue
        total=$((total + 1))
        if [ "$LIST_ONLY" = 1 ]; then
            printf '%s:%s\n' "$path" "$line"
            continue
        fi
        if allowlisted "$path" "$line"; then
            continue
        fi
        violations=$((violations + 1))
        cat >&2 <<EOF
error: forbidden use_effect+spawn+signal-write pattern detected at $path:$line
  This is CLAUDE.md hang class #3 (infinite spawn loop). Use
  use_spawn_once<K>(key, async_fn) instead. See:
    crates/core/src/state/use_spawn_once.rs
    docs/plans/plan-use-spawn-once.md
  If this is a genuine exception, add the site to
    tools/scripts/use-effect-spawn-cycle-allowlist.txt
  with a rationale comment.
EOF
    done < <(scan_file "$rs_file")
done < <(file_list)

if [ "$LIST_ONLY" = 1 ]; then
    echo "# ${total} use_effect+spawn+signal-write triples found (allowlist not applied)" >&2
    exit 0
fi

if [ "$violations" -gt 0 ]; then
    echo "" >&2
    echo "$violations unallowlisted use_effect+spawn+signal-write violation(s). Fix or allowlist." >&2
    exit 1
fi

exit 0
