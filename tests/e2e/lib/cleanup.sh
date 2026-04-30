#!/usr/bin/env bash
# tests/e2e/lib/cleanup.sh — EXIT-trap cleanup for the Poly e2e harness.
#
# Source this file; do NOT execute it directly.  Call install_cleanup_trap
# once near the top of the entry-point script.
#
# Design: idempotent — re-running is always safe.  Kills by PID file first
# (exact), then falls back to pgrep pattern sweeps so any orphan that escaped
# PID tracking is still reaped.

# ---------------------------------------------------------------------------
# _cleanup_by_pid_dir <pids_dir>
#   Kill every process whose PID is recorded under <pids_dir>/*.pid.
# ---------------------------------------------------------------------------
_cleanup_by_pid_dir() {
    local pids_dir="$1"
    if [[ ! -d "$pids_dir" ]]; then
        return 0
    fi
    for pid_file in "$pids_dir"/*.pid; do
        [[ -e "$pid_file" ]] || continue
        local pid
        pid=$(cat "$pid_file" 2>/dev/null || true)
        if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
            echo "[cleanup] killing PID $pid (from $pid_file)"
            kill "$pid" 2>/dev/null || true
        fi
        rm -f "$pid_file"
    done
    # Give SIGTERM recipients a moment before the pattern sweep below.
    sleep 1
    for pid_file in "$pids_dir"/*.pid; do
        [[ -e "$pid_file" ]] || continue
        local pid
        pid=$(cat "$pid_file" 2>/dev/null || true)
        if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
            echo "[cleanup] SIGKILL PID $pid (SIGTERM timeout)"
            kill -9 "$pid" 2>/dev/null || true
        fi
    done
}

# ---------------------------------------------------------------------------
# _cleanup_orphan_sweeps
#   Belt-and-suspenders: kill anything matching the known process patterns
#   even if it slipped through PID tracking.
# ---------------------------------------------------------------------------
_cleanup_orphan_sweeps() {
    local patterns=(
        "poly-test-matrix"
        "poly-test-stoat"
        "poly-test-discord"
        "poly-test-teams"
        "poly-test-lemmy"
        "poly-test-hackernews"
        "poly-test-forgejo"
        "poly-test-github"
        "poly-test-runner"
        "poly-chat-mcp"
    )
    for pat in "${patterns[@]}"; do
        local pids
        pids=$(pgrep -f "$pat" 2>/dev/null || true)
        if [[ -n "$pids" ]]; then
            echo "[cleanup] orphan sweep: killing '$pat' PIDs: $pids"
            # shellcheck disable=SC2086
            kill $pids 2>/dev/null || true
        fi
    done

    # dx serve — only kill the instance started for THIS run by matching our
    # specific port.  Use ${E2E_WEB_PORT:-3000} so it scopes to our run.
    local web_port="${E2E_WEB_PORT:-3000}"
    local dx_pids
    dx_pids=$(pgrep -f "dx serve.*--port ${web_port}" 2>/dev/null || true)
    if [[ -n "$dx_pids" ]]; then
        echo "[cleanup] orphan sweep: killing dx serve on port ${web_port}: $dx_pids"
        # shellcheck disable=SC2086
        kill $dx_pids 2>/dev/null || true
    fi
}

# ---------------------------------------------------------------------------
# _cleanup_handler
#   The actual function registered with `trap`.
# ---------------------------------------------------------------------------
_cleanup_handler() {
    local exit_code=$?
    echo ""
    echo "[cleanup] EXIT trap fired (exit_code=${exit_code})"
    _cleanup_by_pid_dir "${PIDS_DIR:-/dev/null}"
    _cleanup_orphan_sweeps
    echo "[cleanup] done"
    exit "$exit_code"
}

# ---------------------------------------------------------------------------
# install_cleanup_trap
#   Register _cleanup_handler on EXIT.  Call once near the top of the
#   entry-point script, AFTER sourcing this file and setting PIDS_DIR.
# ---------------------------------------------------------------------------
install_cleanup_trap() {
    trap _cleanup_handler EXIT
    echo "[cleanup] EXIT trap installed (PIDS_DIR=${PIDS_DIR:-<not set>})"
}
