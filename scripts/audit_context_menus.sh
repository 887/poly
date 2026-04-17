#!/usr/bin/env bash
#
# Phase 0 context-menu inventory — plan-context-menu-quality-control.md §1.1.1.
#
# For every `#[component]`-annotated fn in crates/core/src/ui/** and
# clients/*/src/**, emit a CSV row:
#
#   file,line,component_name,has_oncontextmenu,has_ontouchstart,prevent_default_count,decorator
#
# `decorator` is the nearest preceding `#[context_menu(...)]` (or "NONE" if
# missing). Run from the repo root:
#
#   ./scripts/audit_context_menus.sh > docs/plans/context-menu-coverage.csv
#
# Use the CSV to seed the bleeding list in docs/plans/context-menu-coverage.toml.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# Echo the header once.
echo "file,line,component,decorator,has_oncontextmenu,has_ontouchstart,prevent_default_count"

scan_file() {
    local file=$1
    # Match every line carrying `#[component]` and remember its line number.
    # Swallow the grep=1 exit when a file has zero components — set -e would
    # otherwise kill the whole script at the first componentless file.
    (grep -n "^#\[component\]" "$file" 2>/dev/null || true) | while IFS=: read -r line _; do
        local component_name decorator body_end has_ctx has_touch pd_count

        # Component name is on the next line that starts `fn `, `pub fn`, or `pub(crate) fn`.
        component_name=$(
            awk -v start="$line" 'NR>start {
                if (match($0, /fn[[:space:]]+([A-Za-z_][A-Za-z0-9_]*)/, m)) {
                    print m[1]; exit
                }
            }' "$file"
        )
        component_name=${component_name:-UNKNOWN}

        # Nearest preceding #[context_menu(...)] in the 6 lines above #[component].
        # Match `)]` followed by either EOL or a `// ...` trailing comment so
        # the `#[context_menu(inherit)] // TODO(...)` pattern is picked up too.
        decorator=$(
            awk -v start="$line" 'NR>=start-6 && NR<start && /^#\[context_menu\([^)]*\)\]([[:space:]]*\/\/.*)?[[:space:]]*$/ {
                match($0, /^#\[context_menu\(([^)]*)\)\]/, m);
                print m[1]; exit
            }' "$file"
        )
        decorator=${decorator:-NONE}

        # Approximate body span: up to 400 lines after #[component] until a
        # trailing `^}` at column 1. Plenty for the 150-line cap.
        body_end=$(
            awk -v start="$line" 'NR>start && /^}/ { print NR; exit }' "$file"
        )
        body_end=${body_end:-$((line + 400))}

        has_ctx=$(awk -v s="$line" -v e="$body_end" 'NR>s && NR<=e && /oncontextmenu/ { c++ } END { print c+0 }' "$file")
        has_touch=$(awk -v s="$line" -v e="$body_end" 'NR>s && NR<=e && /ontouchstart/ { c++ } END { print c+0 }' "$file")
        pd_count=$(awk -v s="$line" -v e="$body_end" 'NR>s && NR<=e && /prevent_default\(\)/ { c++ } END { print c+0 }' "$file")

        printf '%s,%s,%s,%s,%s,%s,%s\n' \
            "$file" "$line" "$component_name" "$decorator" \
            "$has_ctx" "$has_touch" "$pd_count"
    done
}

while IFS= read -r -d '' file; do
    scan_file "$file"
done < <(
    find crates/core/src/ui clients -type f -name '*.rs' -print0
)
