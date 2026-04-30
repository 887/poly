#!/usr/bin/env bash
# tests/e2e/lib/process.sh — Process-management utilities for the Poly e2e harness.
#
# Source this file; do NOT execute it directly.
#
# All wait helpers cap at 60 s per call (feedback_wait_timeouts).

# ---------------------------------------------------------------------------
# spawn_bg <pid_file> <log_file> <cmd> [args…]
#   Fork <cmd> into the background, write its PID to <pid_file>, and redirect
#   stdout+stderr to <log_file>.  Callers are expected to call wait_for_port or
#   wait_for_http_200 after this to confirm the process is healthy.
# ---------------------------------------------------------------------------
spawn_bg() {
    local pid_file="$1"
    local log_file="$2"
    shift 2
    "$@" >"$log_file" 2>&1 &
    local pid=$!
    echo "$pid" >"$pid_file"
    echo "[spawn_bg] PID $pid → $pid_file (cmd: $*)"
}

# ---------------------------------------------------------------------------
# wait_for_port <port> [timeout_s]
#   Poll until a TCP listener appears on <port>.  Default timeout: 60 s.
#   Returns 0 on success, 1 on timeout.
# ---------------------------------------------------------------------------
wait_for_port() {
    local port="$1"
    local timeout="${2:-60}"
    local deadline=$(( $(date +%s) + timeout ))
    echo "[wait_for_port] waiting for :${port} (max ${timeout}s) …"
    while true; do
        if (echo >/dev/tcp/127.0.0.1/"$port") 2>/dev/null; then
            echo "[wait_for_port] :${port} is open"
            return 0
        fi
        if [[ $(date +%s) -ge $deadline ]]; then
            echo "[wait_for_port] TIMEOUT after ${timeout}s — :${port} never opened" >&2
            return 1
        fi
        sleep 1
    done
}

# ---------------------------------------------------------------------------
# wait_for_http_200 <url> [timeout_s]
#   Poll <url> with curl until an HTTP 200 is returned.  Default timeout: 60 s.
#   Returns 0 on success, 1 on timeout.
# ---------------------------------------------------------------------------
wait_for_http_200() {
    local url="$1"
    local timeout="${2:-60}"
    local deadline=$(( $(date +%s) + timeout ))
    echo "[wait_for_http_200] waiting for 200 from ${url} (max ${timeout}s) …"
    while true; do
        local code
        code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 3 "$url" 2>/dev/null || true)
        if [[ "$code" == "200" ]]; then
            echo "[wait_for_http_200] ${url} → 200 OK"
            return 0
        fi
        if [[ $(date +%s) -ge $deadline ]]; then
            echo "[wait_for_http_200] TIMEOUT after ${timeout}s — ${url} never returned 200 (last code: ${code})" >&2
            return 1
        fi
        sleep 1
    done
}

# ---------------------------------------------------------------------------
# kill_pgrep_pattern <pattern>
#   Kill all processes matching <pattern> (SIGTERM, then SIGKILL after 3 s).
#   Mirrors the orphan-cleanup approach used in mcp/electron-devtools-mcp.
#   Safe to call even if nothing matches.
# ---------------------------------------------------------------------------
kill_pgrep_pattern() {
    local pattern="$1"
    local pids
    pids=$(pgrep -f "$pattern" 2>/dev/null || true)
    if [[ -z "$pids" ]]; then
        return 0
    fi
    echo "[kill_pgrep_pattern] SIGTERMing pattern='${pattern}' PIDs: ${pids}"
    # shellcheck disable=SC2086
    kill $pids 2>/dev/null || true
    sleep 1
    # Force-kill any survivors
    local survivors
    survivors=$(pgrep -f "$pattern" 2>/dev/null || true)
    if [[ -n "$survivors" ]]; then
        echo "[kill_pgrep_pattern] SIGKILLing survivors: ${survivors}"
        # shellcheck disable=SC2086
        kill -9 $survivors 2>/dev/null || true
    fi
}

# ---------------------------------------------------------------------------
# find_free_port [start]
#   Echo a free TCP port starting at <start> (default 3010).
# ---------------------------------------------------------------------------
find_free_port() {
    local port="${1:-3010}"
    while true; do
        if ! (echo >/dev/tcp/127.0.0.1/"$port") 2>/dev/null; then
            echo "$port"
            return 0
        fi
        (( port++ ))
    done
}
