#!/usr/bin/env bash
# forbid-signal-write.sh — Phase 5 Track A of plan-batched-signal.md.
#
# Scans crates/core/src/ui/**/*.rs for raw `.write()` calls on signal-like
# receivers. Raw `Signal::write()` is CLAUDE.md hang class #1 (multi-.write()
# cascade) — every guard drop schedules a Dioxus reactive pass, and 5-7
# consecutive writes wedge the single-threaded WASM scheduler.
#
# The migrated hot-path signals (`Signal<ChatData>`, `Signal<AppState>`) are
# now `BatchedSignal<ChatData>` / `BatchedSignal<AppState>`, whose only sync
# mutation verb is `.batch(|v| ...)`. Any reintroduction of `.write()` on
# those signals is a compile error (deprecated shadow method). This script
# guards the remaining plain `Signal<T>` sites against the same cascade
# pattern by failing CI on any unallowlisted `.write()` hit.
#
# Usage:
#   ./tools/scripts/forbid-signal-write.sh
#   # Optional: override the scan root
#   ROOT=/tmp/some-worktree ./tools/scripts/forbid-signal-write.sh
#
# Exit codes:
#   0 — clean (no unallowlisted .write() hits)
#   1 — one or more unallowlisted hits; details printed to stderr
#   2 — misuse / internal error

set -euo pipefail

ROOT="${ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
SCAN_DIR="${ROOT}/crates/core/src/ui"
ALLOWLIST="${ROOT}/tools/scripts/signal-write-allowlist.txt"

if [[ ! -d "$SCAN_DIR" ]]; then
    echo "error: scan dir not found: $SCAN_DIR" >&2
    exit 2
fi

# Build a normalised allowlist (strip comments + blank lines). Each line is
# one of:
#   path                      # whole-file allow
#   path:receiver             # allow .write() with this receiver ident
#   path:line=N               # allow .write() at specific line (drift risk)
if [[ -f "$ALLOWLIST" ]]; then
    ALLOW_ENTRIES="$(grep -vE '^\s*(#|$)' "$ALLOWLIST" | sed -E 's/[[:space:]]*#.*$//' | sed -E 's/^[[:space:]]+|[[:space:]]+$//g')"
else
    ALLOW_ENTRIES=""
fi

# Find all `.write()` call sites in the UI crate.
#
# Heuristic:
#   * Match lines where `.write()` appears (exactly, no args).
#   * Skip lines whose `.write()` is immediately followed by `.await` — those
#     are `RwLock::write().await`, not `Signal::write`.
#   * Skip `.write(SOMETHING)` with args — that's `std::io::Write::write`.
#   * Skip pure-comment lines (`//`, `///`, `//!`).
#
# Receiver extraction (for allowlist matching):
#   * If the match is of the form `<ident>.write()` on the same line, the
#     receiver is `<ident>`.
#   * If `.write()` starts the line (multi-line chain like
#     `client_manager\n    .write()\n    .register(...)`), walk backwards
#     through the file to find the most recent non-blank, non-comment line
#     and take the last identifier on it.

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
hits_file="$tmpdir/hits"
: > "$hits_file"

# Use awk for per-file scanning: tracks previous non-blank lines to resolve
# multi-line chains. Emits one record per hit: `path<TAB>line<TAB>receiver<TAB>content`.
while IFS= read -r -d '' f; do
    awk -v path="$f" '
        # Strip leading whitespace for anchor checks but keep original line
        # in `raw` for error reporting.
        {
            raw = $0
            line = raw
            sub(/^[[:space:]]+/, "", line)
        }

        # Track the most recent non-blank, non-pure-comment "carrier" line
        # so that when `.write()` appears on its own continuation line we
        # can still recover the receiver identifier.
        function remember_carrier(l,    trimmed) {
            trimmed = l
            sub(/^[[:space:]]+/, "", trimmed)
            sub(/[[:space:]]+$/, "", trimmed)
            if (trimmed == "") return
            # Skip pure-comment carrier lines.
            if (trimmed ~ /^\/\//) return
            last_carrier = trimmed
        }

        # Skip pure comment lines.
        line ~ /^\/\// { remember_carrier($0); next }

        # Only care about lines that contain `.write()` with nothing inside
        # the parens and NOT followed by `.await`.
        {
            # Is there a `.write()` somewhere?
            if (match(line, /\.write\(\)/) == 0) {
                remember_carrier($0)
                next
            }
            # Exclude `.write().await` / `.write() .await`.
            if (match(line, /\.write\(\)[[:space:]]*\.await/) > 0) {
                remember_carrier($0)
                next
            }

            # Good — count as a hit.
            receiver = ""

            # Case 1: inline  <ident>.write()
            if (match(line, /[A-Za-z_][A-Za-z0-9_]*\.write\(\)/) > 0) {
                # The matched substring is e.g. `client_manager.write()` —
                # the receiver is everything up to `.write()`.
                inline = substr(line, RSTART, RLENGTH)
                sub(/\.write\(\)$/, "", inline)
                receiver = inline
            }

            # Case 2: `.write()` at (or very near) start of line — this is a
            # chained continuation. Use the last carrier line as context.
            if (receiver == "") {
                # Take the last identifier on the carrier line (handles
                # `client_manager` alone, `foo.bar` → bar, etc.).
                carrier = last_carrier
                # Strip trailing `.` if present.
                sub(/\.$/, "", carrier)
                # Extract last identifier-like token.
                while (match(carrier, /[A-Za-z_][A-Za-z0-9_]*/) > 0) {
                    receiver = substr(carrier, RSTART, RLENGTH)
                    carrier = substr(carrier, RSTART + RLENGTH)
                }
            }

            if (receiver == "") {
                receiver = "<unknown>"
            }

            # Emit hit record: path \t line \t receiver \t raw
            printf "%s\t%d\t%s\t%s\n", path, NR, receiver, raw
            remember_carrier($0)
            next
        }
    ' "$f" >> "$hits_file"
done < <(find "$SCAN_DIR" -type f -name '*.rs' -print0)

# Allowlist check. Each hit is compared against three allowlist forms:
#   1. exact path            (file-level allow)
#   2. path:receiver         (per-ident allow)
#   3. path:line=N           (per-line allow, discouraged — line drift)
# A hit is flagged unless at least one form matches.

unallowed=0
flagged_file="$tmpdir/flagged"
: > "$flagged_file"

while IFS=$'\t' read -r hit_path hit_line hit_recv hit_raw; do
    [[ -z "$hit_path" ]] && continue
    # Normalise path to repo-relative for allowlist matching.
    rel_path="${hit_path#$ROOT/}"
    matched=0
    while IFS= read -r entry; do
        [[ -z "$entry" ]] && continue
        if [[ "$entry" == "$rel_path" ]]; then matched=1; break; fi
        if [[ "$entry" == "$rel_path:$hit_recv" ]]; then matched=1; break; fi
        if [[ "$entry" == "$rel_path:line=$hit_line" ]]; then matched=1; break; fi
    done <<< "$ALLOW_ENTRIES"

    if [[ "$matched" -eq 0 ]]; then
        printf '%s\t%s\t%s\t%s\n' "$rel_path" "$hit_line" "$hit_recv" "$hit_raw" >> "$flagged_file"
        unallowed=$((unallowed + 1))
    fi
done < "$hits_file"

if [[ "$unallowed" -eq 0 ]]; then
    # Report nothing on clean runs — CI logs should stay quiet.
    exit 0
fi

# Report every flagged hit with the standard message.
while IFS=$'\t' read -r rel_path hit_line hit_recv hit_raw; do
    cat >&2 <<EOF
error: forbidden Signal::write() detected at ${rel_path}:${hit_line}
  ${hit_raw}
This is CLAUDE.md hang class #1 (multi-.write() cascade). Use
BatchedSignal::batch(|v| ...) or PendingUpdate instead. See:
  crates/core/src/state/batched_signal.rs
  docs/plans/plan-batched-signal.md
If this is a genuine exception (non-hot-path local Signal, intentional
one-off), add the site to tools/scripts/signal-write-allowlist.txt
with a rationale comment. Allowlist formats:
  ${rel_path}                    # whole-file allow
  ${rel_path}:${hit_recv}        # per-receiver allow (preferred)
  ${rel_path}:line=${hit_line}   # per-line allow (line-drift risk)

EOF
done < "$flagged_file"

echo "error: ${unallowed} unallowlisted Signal::write() hit(s) in crates/core/src/ui" >&2
exit 1
