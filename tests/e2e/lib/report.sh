#!/usr/bin/env bash
# tests/e2e/lib/report.sh — JUnit XML + Markdown reporting for Poly persona e2e.
#
# Source this file; do NOT execute directly.
#
# Public API:
#   emit_junit   <scenario> <results_dir> <agents_summary_json>
#   emit_markdown_summary <scenario> <results_dir> <agents_summary_json>
#
# Both functions are called at the end of each scenario run (Phase F.2).
# Output files:
#   $results_dir/junit-<scenario>.xml     — consumed by GitHub Actions test-summary
#   $results_dir/summary-<scenario>.md    — posted as sticky PR comment
#
# Quarantine (F.3):
#   A scenario.sh that sets  # E2E_QUARANTINE: <reason>  at the top is treated
#   as quarantined.  Its pass/fail does NOT fail the build.  Quarantined results
#   are appended to $results_dir/quarantined.jsonl.
#
#   detect_quarantine <scenario_sh_path>
#     → sets QUARANTINE_REASON to the reason string (empty if not quarantined)

# ---------------------------------------------------------------------------
# detect_quarantine <scenario_sh_path>
#   Read the first 20 lines of scenario.sh; look for  # E2E_QUARANTINE: <reason>
#   Exports QUARANTINE_REASON (empty if not quarantined).
# ---------------------------------------------------------------------------
detect_quarantine() {
    local scenario_sh="$1"
    QUARANTINE_REASON=""
    if [[ ! -f "$scenario_sh" ]]; then
        return 0
    fi
    local reason
    reason=$(head -20 "$scenario_sh" | grep -o '# E2E_QUARANTINE:.*' | sed 's/# E2E_QUARANTINE: *//' | head -1 || true)
    QUARANTINE_REASON="$reason"
}

# ---------------------------------------------------------------------------
# emit_junit <scenario> <results_dir> <agents_summary_json>
#
# Produces a JUnit XML file consumed by GitHub Actions test-summary
# (github-actions-junit-summary or similar).
#
# Schema: one <testsuite> per scenario, one <testcase> per agent.
# Failures are recorded as <failure> elements; quarantined failures are
# tagged with quarantine="true" attribute on <testcase>.
#
# Args:
#   $1  scenario name (e.g. "two-personas-shared-channel")
#   $2  results directory (absolute path)
#   $3  agents-summary.json path (may be absent for no-agent scenarios)
#   $4  scenario_exit_code  — 0 = pass, non-zero = fail (before quarantine)
#   $5  quarantine_reason   — non-empty = quarantined
# ---------------------------------------------------------------------------
emit_junit() {
    local scenario="$1"
    local results_dir="$2"
    local summary_json="${3:-}"
    local exit_code="${4:-0}"
    local quarantine_reason="${5:-}"

    local out_xml="${results_dir}/junit-${scenario}.xml"
    local timestamp
    timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

    # Build per-agent testcases from agents-summary.json (if present)
    python3 - \
        "$scenario" \
        "$out_xml" \
        "$timestamp" \
        "$exit_code" \
        "$quarantine_reason" \
        "${summary_json:-/dev/null}" \
        <<'PYEOF'
import json, sys, os, xml.etree.ElementTree as ET
from xml.dom import minidom

scenario        = sys.argv[1]
out_xml         = sys.argv[2]
timestamp       = sys.argv[3]
exit_code       = int(sys.argv[4])
quarantine      = sys.argv[5]
summary_path    = sys.argv[6]

# Load agents summary if available
agents = []
if os.path.isfile(summary_path) and summary_path != "/dev/null":
    try:
        with open(summary_path) as f:
            d = json.load(f)
        agents = d.get("agents", [])
    except Exception:
        pass

# Build XML tree
suite = ET.Element("testsuite")
suite.set("name", scenario)
suite.set("timestamp", timestamp)
suite.set("classname", f"e2e.persona.{scenario}")

if agents:
    failures = sum(1 for a in agents if not a.get("success"))
    suite.set("tests", str(len(agents)))
    suite.set("failures", str(failures))
    suite.set("errors", "0")

    for agent in agents:
        tc = ET.SubElement(suite, "testcase")
        tc.set("name", agent["slug"])
        tc.set("classname", f"e2e.persona.{scenario}.{agent['slug']}")
        if quarantine:
            tc.set("quarantine", "true")
            tc.set("quarantine_reason", quarantine)

        if not agent.get("success"):
            fail_el = ET.SubElement(tc, "failure")
            fail_el.set("message", agent.get("subtype", "unknown"))
            fail_el.text = agent.get("result", "")

        # System-out: tool call trace
        sys_out = ET.SubElement(tc, "system-out")
        calls = agent.get("tool_calls", [])
        sys_out.text = "\n".join(
            f"{c.get('name','?')}({json.dumps(c.get('input',''))})" for c in calls
        ) or "(no tool calls)"
else:
    # Scenario-level synthetic testcase (no agent data)
    suite.set("tests", "1")
    suite.set("failures", "1" if exit_code != 0 else "0")
    suite.set("errors", "0")
    tc = ET.SubElement(suite, "testcase")
    tc.set("name", scenario)
    tc.set("classname", f"e2e.persona.{scenario}")
    if quarantine:
        tc.set("quarantine", "true")
        tc.set("quarantine_reason", quarantine)
    if exit_code != 0:
        fail_el = ET.SubElement(tc, "failure")
        fail_el.set("message", "scenario failed")
        fail_el.text = f"Scenario '{scenario}' exited with code {exit_code}"

# Wrap in testsuites root
root = ET.Element("testsuites")
root.append(suite)

# Pretty-print
raw = ET.tostring(root, encoding="unicode")
pretty = minidom.parseString(raw).toprettyxml(indent="  ")
# minidom adds an xml declaration; strip it to keep GitHub Actions happy
lines = pretty.split("\n")
if lines[0].startswith("<?xml"):
    lines = lines[1:]
pretty = "\n".join(lines)

os.makedirs(os.path.dirname(out_xml), exist_ok=True)
with open(out_xml, "w") as f:
    f.write('<?xml version="1.0" encoding="UTF-8"?>\n')
    f.write(pretty)

print(f"[F.2] JUnit XML written: {out_xml}")
PYEOF
}

# ---------------------------------------------------------------------------
# emit_markdown_summary <scenario> <results_dir> <agents_summary_json>
#                       <exit_code> <quarantine_reason> <playwright_log>
#
# Produces a Markdown summary file and echoes it to stdout so CI can
# capture it for a sticky PR comment.
#
# Args:
#   $1  scenario name
#   $2  results directory
#   $3  agents-summary.json path (may be absent)
#   $4  exit_code  — 0 = pass, non-zero = fail (before quarantine)
#   $5  quarantine_reason — non-empty = quarantined
#   $6  playwright_log   — path to playwright log (may be absent)
# ---------------------------------------------------------------------------
emit_markdown_summary() {
    local scenario="$1"
    local results_dir="$2"
    local summary_json="${3:-}"
    local exit_code="${4:-0}"
    local quarantine_reason="${5:-}"
    local playwright_log="${6:-}"

    local out_md="${results_dir}/summary-${scenario}.md"

    python3 - \
        "$scenario" \
        "$out_md" \
        "$exit_code" \
        "$quarantine_reason" \
        "${summary_json:-/dev/null}" \
        "${playwright_log:-/dev/null}" \
        <<'PYEOF'
import json, sys, os, re

scenario         = sys.argv[1]
out_md           = sys.argv[2]
exit_code        = int(sys.argv[3])
quarantine       = sys.argv[4]
summary_path     = sys.argv[5]
playwright_log   = sys.argv[6]

status_icon = "✅" if exit_code == 0 else ("⚠️ quarantined" if quarantine else "❌")
status_text = "PASSED" if exit_code == 0 else ("QUARANTINED" if quarantine else "FAILED")

lines = [
    f"## Persona E2E — `{scenario}` — {status_icon} {status_text}",
    "",
]

if quarantine:
    lines += [
        f"> **Quarantined:** {quarantine}",
        "> Failure does NOT block the build. Review weekly.",
        "",
    ]

# Per-agent table
agents = []
if os.path.isfile(summary_path) and summary_path != "/dev/null":
    try:
        with open(summary_path) as f:
            d = json.load(f)
        agents = d.get("agents", [])
    except Exception:
        pass

if agents:
    lines += [
        "### Agent results",
        "",
        "| Agent | Status | Tool calls | Detail |",
        "|-------|--------|-----------|--------|",
    ]
    for a in agents:
        icon = "✅" if a.get("success") else "❌"
        detail = a.get("subtype", "")
        n_calls = a.get("tool_call_count", 0)
        lines.append(f"| `{a['slug']}` | {icon} {detail} | {n_calls} | {a.get('result','')[:80]} |")
    lines.append("")

# Playwright timing from log
if os.path.isfile(playwright_log) and playwright_log != "/dev/null":
    try:
        with open(playwright_log) as f:
            log_text = f.read()
        # Extract timing lines like "live-update: 1234ms"
        timings = re.findall(r'live.update[^:]*:\s*(\d+)\s*ms', log_text, re.IGNORECASE)
        if timings:
            lines += ["### Live-update timing", ""]
            for t in timings:
                ms = int(t)
                if ms <= 5000:
                    tier = "healthy (≤5s)"
                    icon = "✅"
                elif ms <= 15000:
                    tier = "degraded (≤15s)"
                    icon = "⚠️"
                else:
                    tier = "broken (>15s)"
                    icon = "❌"
                lines.append(f"- {icon} `{ms}ms` — {tier}")
            lines.append("")
    except Exception:
        pass

os.makedirs(os.path.dirname(out_md), exist_ok=True)
content = "\n".join(lines)
with open(out_md, "w") as f:
    f.write(content)
    f.write("\n")

print(content)
print(f"\n[F.2] Markdown summary written: {out_md}")
PYEOF
}

# ---------------------------------------------------------------------------
# record_quarantine <scenario> <quarantine_reason> <results_dir>
#
# Appends a line to $results_dir/quarantined.jsonl for CI to publish.
# ---------------------------------------------------------------------------
record_quarantine() {
    local scenario="$1"
    local reason="$2"
    local results_dir="$3"
    local ts
    ts=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

    python3 -c "
import json, sys
with open('${results_dir}/quarantined.jsonl', 'a') as f:
    f.write(json.dumps({'scenario': '${scenario}', 'reason': '${reason}', 'timestamp': '${ts}'}) + '\n')
print('[F.3] Recorded quarantined scenario: ${scenario}')
" 2>/dev/null || true
}

# ---------------------------------------------------------------------------
# run_and_report <scenario> <scenario_sh> <results_dir> <agents_summary_json>
#                <playwright_log>
#
# High-level helper: runs detect_quarantine, then calls emit_junit +
# emit_markdown_summary + record_quarantine (if needed).
#
# Intended to be called from the main harness AFTER a scenario completes.
#
# Returns 0 unless scenario failed AND is NOT quarantined.
# ---------------------------------------------------------------------------
run_and_report() {
    local scenario="$1"
    local scenario_sh="$2"
    local results_dir="$3"
    local agents_summary="${4:-}"
    local playwright_log="${5:-}"
    local exit_code="${6:-0}"

    detect_quarantine "$scenario_sh"
    local quarantine_reason="$QUARANTINE_REASON"

    emit_junit \
        "$scenario" \
        "$results_dir" \
        "$agents_summary" \
        "$exit_code" \
        "$quarantine_reason"

    emit_markdown_summary \
        "$scenario" \
        "$results_dir" \
        "$agents_summary" \
        "$exit_code" \
        "$quarantine_reason" \
        "$playwright_log"

    if [[ -n "$quarantine_reason" && "$exit_code" -ne 0 ]]; then
        record_quarantine "$scenario" "$quarantine_reason" "$results_dir"
        echo "[F.3] Quarantined failure suppressed for scenario: ${scenario}"
        return 0
    fi

    return "$exit_code"
}
