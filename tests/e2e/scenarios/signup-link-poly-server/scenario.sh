#!/usr/bin/env bash
# tests/e2e/scenarios/signup-link-poly-server/scenario.sh
#
# Sourced by persona-multi-agent.sh when --scenario signup-link-poly-server is passed.
# Requires poly-web (auto-set NEEDS_POLY_WEB) on port 3000.
#
# Special: poly-server uses InApp("/signup/poly"), so this test asserts
# in-app navigation rather than an external URL.

SCENARIO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCENARIO_DIR/../../../.." && pwd)"

echo "[signup-link-poly-server] Running Playwright spec …"
cd "$REPO_ROOT"
npx playwright test tests/e2e/signup/poly-server-signup.spec.ts --reporter=list
