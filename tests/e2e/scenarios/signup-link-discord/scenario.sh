#!/usr/bin/env bash
# tests/e2e/scenarios/signup-link-discord/scenario.sh
#
# Sourced by persona-multi-agent.sh when --scenario signup-link-discord is passed.
# Requires the poly-web stack (NEEDS_POLY_WEB=true) to be running on port 3000.
# NEEDS_POLY_WEB is auto-set by the harness case block for this scenario name.
#
# Runs the Discord signup-link Playwright spec in mock-mode (href assertion only).
# Set POLY_SIGNUP_E2E_REAL=1 to also click the link and verify the remote page.

SCENARIO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCENARIO_DIR/../../../.." && pwd)"

echo "[signup-link-discord] Running Playwright spec …"
cd "$REPO_ROOT"
npx playwright test tests/e2e/signup/discord-signup.spec.ts --reporter=list
