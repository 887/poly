#!/usr/bin/env bash
# tests/e2e/scenarios/forum-composer-inline-reply/scenario.sh
#
# D.4 — ForumComposer inline reply end-to-end scenario.
#
# Sourced by persona-multi-agent.sh when --scenario forum-composer-inline-reply.
# Drives the test-lemmy backend via the WASM UI to verify:
#   1. Opening an existing forum post shows comments.
#   2. Clicking the Reply button on the first comment expands the inline composer.
#   3. Filling the reply body and clicking Submit sends the reply.
#   4. The reply appears nested under the parent comment within 5s.
#
# No claude agent is needed — this is a pure UI-driver scenario.
# NEEDS_POLY_WEB=true is set by the harness case block for this scenario.

SCENARIO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "[forum-composer-inline-reply] Running Playwright inline-reply spec …"

REPO_ROOT="$(cd "$SCENARIO_DIR/../../../.." && pwd)"
cd "$REPO_ROOT"

# Run the dedicated inline-reply spec.
# The spec handles navigation to a post, clicking Reply, filling body, and asserting.
E2E_WEB_BASE_URL="http://127.0.0.1:${E2E_WEB_PORT:-3000}" \
npx playwright test tests/e2e/specs/forum-composer-inline-reply.spec.ts \
    --reporter=list \
    --timeout=60000
