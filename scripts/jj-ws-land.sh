#!/usr/bin/env bash
# jj-ws-land.sh — safely snapshot a jj workspace's work before integrating it.
#
# THE FAILURE THIS PREVENTS:
#   jj only snapshots a workspace's on-disk changes when a jj command runs with
#   THAT WORKSPACE AS CWD. Agents/tools edit files in a `jj workspace add` dir but
#   never run jj, so the workspace's working-copy commit (@) stays EMPTY. If you
#   then rebase / move / forget that commit FROM THE MAIN repo without snapshotting
#   first, you operate on the empty commit — the real changes are still un-committed
#   on disk. Worse: a cross-workspace `jj edit` marks the workspace stale, and
#   `jj workspace update-stale` checks out the EMPTY commit OVER the on-disk work,
#   silently destroying it. The op log has no snapshot, so it is unrecoverable.
#
# THE GUARD:
#   This script cd's INTO the workspace (so the next jj cmd snapshots it), then
#   ABORTS if the working-copy commit is EMPTY after the snapshot. So you can
#   never rebase/forget un-snapshotted work, and never reach the update-stale
#   clobber path. On success it prints the safely-snapshotted CHANGE ID — rebase
#   THAT from the main repo, then verify, then `jj workspace forget` + rm.
#
# Usage:
#   scripts/jj-ws-land.sh <workspace-dir> ["<commit message>"]
# Output (stdout): the change id. Diagnostics go to stderr.
#
# RULE: this is the ONLY sanctioned way to ready a jj workspace for integration.
#       NEVER `jj rebase -s <ws>@` / `jj workspace forget` a workspace whose @ you
#       have not snapshotted from inside via this script.

set -euo pipefail

WS="${1:?usage: jj-ws-land.sh <workspace-dir> [\"<message>\"]}"
MSG="${2:-}"

[ -d "$WS/.jj" ] || { echo "ABORT: '$WS' is not a jj workspace (no .jj dir)." >&2; exit 2; }

# 1. Snapshot FROM INSIDE — the whole point.
cd "$WS"
jj st >/dev/null

# 2. Refuse to proceed on an EMPTY working-copy commit (no work, or wrong
#    workspace) — forgetting it would risk the update-stale clobber.
if jj log -r @ --no-graph -T 'if(empty, "EMPTY", "OK")' | grep -qx EMPTY; then
  echo "ABORT: $WS @ is EMPTY after snapshot — nothing to land." >&2
  echo "       (No on-disk changes, or wrong workspace. NOT rebasing/forgetting.)" >&2
  exit 3
fi

# 3. Describe (optional) and emit the change id. The work is now safely committed.
[ -n "$MSG" ] && jj describe -r @ -m "$MSG" >/dev/null
CHANGE_ID="$(jj log -r @ --no-graph -T 'change_id.short()')"
echo "SAFE: snapshotted $WS -> change $CHANGE_ID" >&2
echo "$CHANGE_ID"
