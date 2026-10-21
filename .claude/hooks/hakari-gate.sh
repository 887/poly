#!/bin/sh
# PreToolUse(Bash) merge gate: block a push unless cargo-hakari is clean.
# Order matters: manage-deps --dry-run first (cheap, catches a crate missing the
# workspace-hack dep), then verify (catches stale unified features).
# Fix with: cargo hakari generate && cargo hakari manage-deps
input=$(cat)
case "$input" in
  *"jj git push"*|*"git push"*) ;;
  *) exit 0 ;;
esac

cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0

if ! out=$(cargo hakari manage-deps --dry-run 2>&1); then
  printf 'hakari gate: `cargo hakari manage-deps --dry-run` failed.\n%s\nRun: cargo hakari generate && cargo hakari manage-deps\n' "$out" >&2
  exit 2
fi
if ! out=$(cargo hakari verify 2>&1); then
  printf 'hakari gate: `cargo hakari verify` failed.\n%s\nRun: cargo hakari generate && cargo hakari manage-deps\n' "$out" >&2
  exit 2
fi
exit 0
