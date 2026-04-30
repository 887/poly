#!/usr/bin/env bash
# tests/e2e/scenarios/signup-link-hackernews/scenario.sh
#
# Sourced by persona-multi-agent.sh when --scenario signup-link-hackernews is passed.
# Requires poly-web (auto-set NEEDS_POLY_WEB) on port 3000.

SCENARIO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCENARIO_DIR/../../../.." && pwd)"

echo "[signup-link-hackernews] Running Playwright spec …"
cd "$REPO_ROOT"
npx playwright test tests/e2e/signup/hackernews-signup.spec.ts --reporter=list
