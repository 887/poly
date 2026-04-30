#!/usr/bin/env bash
# tests/e2e/scenarios/forum-composer-create-post/scenario.sh
#
# D.3 — ForumComposer "Create Post" end-to-end scenario.
#
# Sourced by persona-multi-agent.sh when --scenario forum-composer-create-post.
# Drives the test-lemmy backend via the WASM UI to verify:
#   1. Navigating to a Lemmy forum channel shows the "Create Post" button.
#   2. Clicking it opens the ForumComposer (data-testid="forum-composer").
#   3. Filling title + body and clicking Submit sends the post.
#   4. The new post title appears in the forum list within 5s.
#
# No claude agent is needed — this is a pure UI-driver scenario.
# NEEDS_POLY_WEB=true is set by the harness case block for this scenario.

SCENARIO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "[forum-composer-create-post] Writing Playwright manifest …"

# Write manifest assertions — the persona-live spec reads these.
SCENARIO_ASSERTIONS=(
    '{"kind":"wait_for_visible","locator":"[data-testid=\"forum-composer-new-post-btn\"]","timeout_ms":10000}'
)

write_scenario_manifest "forum-composer-create-post"

echo "[forum-composer-create-post] Running Playwright create-post spec …"

REPO_ROOT="$(cd "$SCENARIO_DIR/../../../.." && pwd)"
cd "$REPO_ROOT"

# Run the dedicated create-post spec (not the generic persona-live spec).
# The spec handles navigation, form fill, submit, and assertion itself.
E2E_WEB_BASE_URL="http://127.0.0.1:${E2E_WEB_PORT:-3000}" \
npx playwright test tests/e2e/specs/forum-composer-create-post.spec.ts \
    --reporter=list \
    --timeout=60000
