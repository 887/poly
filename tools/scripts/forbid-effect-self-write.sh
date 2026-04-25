#!/usr/bin/env bash
#
# forbid-effect-self-write.sh — Phase 5 lint for hang class #8.
#
# Scans `crates/core/src/ui/**/*.rs` for `use_effect` bodies that both
# READ a signal `X` (via `X.read()` / `*X.read()` / `&X.read()`) AND
# WRITE that same signal `X` via raw `X.set(` / `X.batch(` — without an
# `_if_changed` guard. This is the canonical shape of CLAUDE.md "Common
# WASM-hang causes" #8: `BatchedSignal::batch` always notifies
# subscribers, so an effect that subscribes-to and writes-to the same
# signal re-fires after its own write. When the body's early-return
# guard has a hole for the steady state (e.g. `messages_loaded == false`
# for an empty channel), the loop pegs the WASM scheduler.
#
# Use `BatchedSignal::set_if_changed(next)` or
# `batch_if_changed(|cur| -> next)` instead — both compare against the
# current value and skip the write when equal, so subscribers don't
# re-notify and self-write effects converge.
#
# Usage:
#   tools/scripts/forbid-effect-self-write.sh         # scan, exit 1 on violation
#   tools/scripts/forbid-effect-self-write.sh --list  # list every match regardless of allowlist
#
# Allowlist:
#   - File: tools/scripts/effect-self-write-allowlist.txt — `path:line` per
#     entry, `#` comments and blanks ignored. `line` is the line of the
#     offending `use_effect(move ||`.
#   - Inline: a comment matching `// poly-lint: allow effect-self-write`
#     on ANY line inside the use_effect body suppresses the violation.
#
# Heuristic notes:
# - Brace-matching is on `{` and `}` counts; good enough for
#   well-formatted Rust, imperfect inside strings/comments.
# - Identity match: same identifier name read AND written in the same
#   effect body. Cross-binding (`let y = x; y.set(...)`) is not caught.
# - `.set_if_changed(` and `.batch_if_changed(` are excluded — those are
#   the safe API.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

SCAN_ROOT="crates/core/src/ui"
ALLOWLIST="tools/scripts/effect-self-write-allowlist.txt"

LIST_ONLY=0
if [ "${1:-}" = "--list" ]; then
    LIST_ONLY=1
fi

if [ ! -d "$SCAN_ROOT" ]; then
    echo "error: $SCAN_ROOT not found (run from repo root)" >&2
    exit 2
fi

allowlisted() {
    local path=$1 line=$2
    [ -f "$ALLOWLIST" ] || return 1
    awk -v p="$path" -v l="$line" '
        /^[[:space:]]*#/ { next }
        /^[[:space:]]*$/ { next }
        {
            n = split($0, parts, ":")
            if (n < 2) next
            entry_line = parts[n]
            entry_path = parts[1]
            for (i = 2; i < n; i++) entry_path = entry_path ":" parts[i]
            sub(/[[:space:]].*$/, "", entry_line)
            sub(/#.*$/, "", entry_line)
            if (entry_path == p && entry_line == l) { found = 1; exit }
        }
        END { exit (found ? 0 : 1) }
    ' "$ALLOWLIST"
}

scan_file() {
    local file=$1
    awk -v file="$file" '
        BEGIN {
            effect_active = 0
            effect_depth = 0
        }
        function reset_effect() {
            effect_active = 0
            effect_depth = 0
            inline_allow = 0
            delete read_set
            delete write_set
            delete write_lines
        }
        {
            line = $0
            stripped = line
            sub(/\/\/.*$/, "", stripped)

            if (!effect_active) {
                if (match(line, /use_effect\(move[[:space:]]*\|\|/)) {
                    reset_effect()
                    effect_active = 1
                    effect_start_line = NR
                    rest = substr(stripped, RSTART + RLENGTH)
                    process_body(rest)
                    next
                }
            } else {
                # Inline allowlist comment anywhere in body.
                if (match(line, /poly-lint:[[:space:]]*allow[[:space:]]+effect-self-write/)) {
                    inline_allow = 1
                }
                process_body(stripped)
            }
        }
        function process_body(text,    n, opens, closes, i) {
            n = length(text)
            opens = gsub(/\{/, "{", text)
            closes = gsub(/\}/, "}", text)
            # Reset gsub side-effects (we used them only for counting).
            text = $0
            sub(/\/\/.*$/, "", text)

            # Collect reads: ident.read(
            scan_pos = 1
            while (match(substr(text, scan_pos), /([A-Za-z_][A-Za-z0-9_]*)\.read\(/, m)) {
                ident = m[1]
                if (ident != "self") read_set[ident] = 1
                scan_pos += RSTART + RLENGTH - 1
            }

            # Collect writes: ident.set( or ident.batch(  — exclude _if_changed.
            scan_pos = 1
            while (match(substr(text, scan_pos), /([A-Za-z_][A-Za-z0-9_]*)\.(set|batch)(_if_changed)?\(/, m)) {
                ident = m[1]
                method = m[2]
                suffix = m[3]
                if (suffix == "" && ident != "self") {
                    write_set[ident] = method
                    write_lines[ident] = NR
                }
                scan_pos += RSTART + RLENGTH - 1
            }

            effect_depth += opens - closes
            if (effect_depth <= 0 && (opens + closes) > 0) {
                # Effect body ended.
                violation = 0
                detail = ""
                for (w in write_set) {
                    if (w in read_set) {
                        violation = 1
                        if (detail == "") detail = w "." write_set[w] "(" "@line " write_lines[w]
                        else detail = detail "; " w "." write_set[w] "(@line " write_lines[w]
                    }
                }
                if (violation && !inline_allow) {
                    printf "%s:%d\t%s\n", file, effect_start_line, detail
                }
                reset_effect()
            }
        }
    ' "$file"
}

violations=0
total=0

file_list() {
    if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        git ls-files "$SCAN_ROOT" 2>/dev/null | grep '\.rs$' || true
    else
        find "$SCAN_ROOT" -type f -name '*.rs'
    fi
}

while IFS= read -r rs_file; do
    [ -n "$rs_file" ] || continue
    while IFS=$'\t' read -r loc detail; do
        [ -n "${loc:-}" ] || continue
        path="${loc%:*}"
        line="${loc##*:}"
        total=$((total + 1))
        if [ "$LIST_ONLY" = 1 ]; then
            printf '%s:%s\t%s\n' "$path" "$line" "$detail"
            continue
        fi
        if allowlisted "$path" "$line"; then
            continue
        fi
        violations=$((violations + 1))
        cat >&2 <<EOF
error: forbidden effect-self-write detected at $path:$line
  $detail
  This is CLAUDE.md hang class #8: a use_effect body that subscribes to
  a signal AND writes that same signal will re-fire after its own write.
  Use BatchedSignal::set_if_changed(next) or
  batch_if_changed(|cur| -> next) — both compare against the current
  value and skip the write when equal, so subscribers don't re-notify.
  See:
    crates/core/src/state/batched_signal.rs (set_if_changed / batch_if_changed)
    CLAUDE.md "Common WASM-hang causes" #8
  Inline allowlist: add a comment matching
    // poly-lint: allow effect-self-write — <reason>
  inside the use_effect body, OR add the site to
    tools/scripts/effect-self-write-allowlist.txt
  with a rationale comment.
EOF
    done < <(scan_file "$rs_file")
done < <(file_list)

if [ "$LIST_ONLY" = 1 ]; then
    echo "# ${total} effect-self-write candidates found (allowlist not applied)" >&2
    exit 0
fi

if [ "$violations" -gt 0 ]; then
    echo "" >&2
    echo "error: ${violations} effect-self-write violation(s) detected." >&2
    exit 1
fi

exit 0
