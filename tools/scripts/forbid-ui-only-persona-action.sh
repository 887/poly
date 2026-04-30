#!/usr/bin/env bash
# forbid-ui-only-persona-action.sh — Phase Q.3 of plan-persona-quality-gates.md.
#
# STUB — Phase D UI has not landed yet.
#
# When Phase D ships (`crates/core/src/ui/agent/persona/` exists and
# contains onclick/onsubmit handlers that mutate persona state), this
# script should grep that directory for direct `MemoryDb::` calls and
# assert they are absent — all persona mutations must go through the MCP
# tool surface to ensure the audit trail is complete.
#
# Full implementation is deferred to the commit that lands Phase D.
# See: docs/plans/plan-persona-quality-gates.md Q.3.

echo "skipped — Phase D UI not landed yet (see plan-persona-quality-gates.md Q.3)"
exit 0
