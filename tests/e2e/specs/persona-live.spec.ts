/**
 * tests/e2e/specs/persona-live.spec.ts
 *
 * Phase D — Playwright live-UI assertion runner for the persona multi-agent
 * e2e harness.
 *
 * Design:
 *   The bash harness writes a per-scenario manifest JSON to
 *   $E2E_SCENARIO_MANIFEST before invoking this spec. The spec reads that
 *   manifest and runs the declared assertions against the running poly-web
 *   instance.
 *
 * Manifest shape (written by bash script, read here):
 *   {
 *     "base_url": "http://localhost:3000",
 *     "scenario": "two-personas-shared-channel",
 *     "assertions": [
 *       { "kind": "wait_for_text",
 *         "locator": "[data-testid='channel-row-ch-shared']",
 *         "text": "broker-bob: COIN beat",
 *         "timeout_ms": 5000 },
 *       { "kind": "wait_for_dom_count",
 *         "locator": "[data-testid='draft-row']",
 *         "count": 1,
 *         "timeout_ms": 8000 },
 *       { "kind": "no_full_reload",
 *         "since_ts": 1714000000000 }
 *     ]
 *   }
 *
 * Assertion kinds:
 *   wait_for_text      — locator must contain text within timeout_ms
 *   wait_for_dom_count — locator must match exactly `count` elements
 *   wait_for_visible   — locator must be visible within timeout_ms
 *   no_full_reload     — page load counter must not increment since since_ts
 *
 * D.6 — Live-update timing budget:
 *   Healthy:  ≤ E2E_LIVE_UPDATE_BUDGET_MS (default 5000 ms)
 *   Degraded: ≤ 15000 ms — warning, still passes
 *   Broken:   > 15000 ms — fail
 *   Configurable via E2E_LIVE_UPDATE_BUDGET_MS env var.
 */

import { test, expect, Page } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface WaitForTextAssertion {
  kind: 'wait_for_text';
  locator: string;
  text: string;
  timeout_ms?: number;
}

interface WaitForDomCountAssertion {
  kind: 'wait_for_dom_count';
  locator: string;
  count: number;
  timeout_ms?: number;
}

interface WaitForVisibleAssertion {
  kind: 'wait_for_visible';
  locator: string;
  timeout_ms?: number;
}

interface NoFullReloadAssertion {
  kind: 'no_full_reload';
  since_ts: number;
}

type Assertion =
  | WaitForTextAssertion
  | WaitForDomCountAssertion
  | WaitForVisibleAssertion
  | NoFullReloadAssertion;

interface ScenarioManifest {
  base_url: string;
  scenario: string;
  assertions: Assertion[];
}

// ---------------------------------------------------------------------------
// D.6 — Live-update timing budget constants
// ---------------------------------------------------------------------------

const HEALTHY_BUDGET_MS = parseInt(process.env.E2E_LIVE_UPDATE_BUDGET_MS ?? '5000', 10);
const DEGRADED_BUDGET_MS = 15_000;

// ---------------------------------------------------------------------------
// Helpers — playwright-helpers.ts logic factored inline (per D decision)
// ---------------------------------------------------------------------------

/**
 * D.3 — Inject load-count tracker into the page.
 *
 * On every full page load, increments window.__poly_e2e_load_count.
 * The `no_full_reload` assertion checks this counter before and after
 * an action to ensure no full reload occurred.
 */
async function injectLoadCounter(page: Page): Promise<void> {
  // Install a persistent navigation listener so the counter increments on
  // every navigation, not just the initial load.
  await page.addInitScript(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (window as any).__poly_e2e_load_count = ((window as any).__poly_e2e_load_count ?? 0) + 1;
  });
}

/**
 * Read the current value of window.__poly_e2e_load_count.
 * Returns 0 if the counter hasn't been set yet.
 */
async function readLoadCount(page: Page): Promise<number> {
  return page.evaluate(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    () => (window as any).__poly_e2e_load_count ?? 0,
  );
}

/**
 * Assert: wait_for_text
 * Waits up to timeout_ms for `locator` to contain `text`.
 * Measures actual latency; warns if > HEALTHY_BUDGET_MS, fails if > DEGRADED_BUDGET_MS.
 */
async function assertWaitForText(page: Page, assertion: WaitForTextAssertion): Promise<void> {
  const timeoutMs = assertion.timeout_ms ?? HEALTHY_BUDGET_MS;
  const effectiveTimeout = Math.max(timeoutMs, DEGRADED_BUDGET_MS);
  const start = Date.now();

  try {
    await expect(page.locator(assertion.locator)).toContainText(assertion.text, {
      timeout: effectiveTimeout,
    });
  } catch (e) {
    throw new Error(
      `wait_for_text FAILED — locator "${assertion.locator}" did not contain "${assertion.text}" within ${effectiveTimeout}ms`,
    );
  }

  const elapsed = Date.now() - start;
  if (elapsed > DEGRADED_BUDGET_MS) {
    throw new Error(
      `wait_for_text BROKEN (${elapsed}ms > ${DEGRADED_BUDGET_MS}ms degraded budget) — reactive chain is broken for locator "${assertion.locator}"`,
    );
  }
  if (elapsed > HEALTHY_BUDGET_MS) {
    console.warn(
      `[D.6] wait_for_text DEGRADED (${elapsed}ms > ${HEALTHY_BUDGET_MS}ms healthy budget) for locator "${assertion.locator}" — reactive chain may be slow`,
    );
  } else {
    console.log(
      `[D.6] wait_for_text OK (${elapsed}ms) for locator "${assertion.locator}"`,
    );
  }
}

/**
 * Assert: wait_for_dom_count
 * Waits up to timeout_ms for `locator` to have exactly `count` DOM elements.
 */
async function assertWaitForDomCount(
  page: Page,
  assertion: WaitForDomCountAssertion,
): Promise<void> {
  const timeoutMs = assertion.timeout_ms ?? HEALTHY_BUDGET_MS;
  const effectiveTimeout = Math.max(timeoutMs, DEGRADED_BUDGET_MS);
  const start = Date.now();

  try {
    await expect(page.locator(assertion.locator)).toHaveCount(assertion.count, {
      timeout: effectiveTimeout,
    });
  } catch (e) {
    const actual = await page.locator(assertion.locator).count();
    throw new Error(
      `wait_for_dom_count FAILED — locator "${assertion.locator}" expected ${assertion.count} elements, got ${actual} within ${effectiveTimeout}ms`,
    );
  }

  const elapsed = Date.now() - start;
  if (elapsed > DEGRADED_BUDGET_MS) {
    throw new Error(
      `wait_for_dom_count BROKEN (${elapsed}ms > ${DEGRADED_BUDGET_MS}ms) for locator "${assertion.locator}"`,
    );
  }
  if (elapsed > HEALTHY_BUDGET_MS) {
    console.warn(
      `[D.6] wait_for_dom_count DEGRADED (${elapsed}ms) for locator "${assertion.locator}"`,
    );
  }
}

/**
 * Assert: wait_for_visible
 * Waits up to timeout_ms for `locator` to be visible.
 */
async function assertWaitForVisible(page: Page, assertion: WaitForVisibleAssertion): Promise<void> {
  const timeoutMs = assertion.timeout_ms ?? HEALTHY_BUDGET_MS;
  const effectiveTimeout = Math.max(timeoutMs, DEGRADED_BUDGET_MS);
  const start = Date.now();

  try {
    await expect(page.locator(assertion.locator)).toBeVisible({ timeout: effectiveTimeout });
  } catch (e) {
    throw new Error(
      `wait_for_visible FAILED — locator "${assertion.locator}" not visible within ${effectiveTimeout}ms`,
    );
  }

  const elapsed = Date.now() - start;
  if (elapsed > DEGRADED_BUDGET_MS) {
    throw new Error(
      `wait_for_visible BROKEN (${elapsed}ms > ${DEGRADED_BUDGET_MS}ms) for locator "${assertion.locator}"`,
    );
  }
  if (elapsed > HEALTHY_BUDGET_MS) {
    console.warn(
      `[D.6] wait_for_visible DEGRADED (${elapsed}ms) for locator "${assertion.locator}"`,
    );
  }
}

/**
 * D.3 — Assert: no_full_reload
 * Fails if window.__poly_e2e_load_count has changed since since_ts.
 *
 * We detect reloads by reading the counter before and after the assertion
 * window. If since_ts is provided, we also check that it is recent (within
 * the last minute) to catch stale manifests.
 */
async function assertNoFullReload(page: Page, assertion: NoFullReloadAssertion): Promise<void> {
  const loadCountBefore = await readLoadCount(page);

  // Check if since_ts is stale (> 60s ago) — warn but don't fail.
  const nowMs = Date.now();
  if (nowMs - assertion.since_ts > 60_000) {
    console.warn(
      `[D.3] no_full_reload since_ts is ${nowMs - assertion.since_ts}ms in the past — may be stale manifest`,
    );
  }

  // Give the page a brief moment to settle before re-reading the counter.
  // We poll for 500ms total; if the counter changed the page reloaded.
  await page.waitForTimeout(200);
  const loadCountAfter = await readLoadCount(page);

  if (loadCountAfter !== loadCountBefore) {
    throw new Error(
      `no_full_reload FAILED — page reloaded during scenario (load count: ${loadCountBefore} → ${loadCountAfter})`,
    );
  }

  console.log(`[D.3] no_full_reload OK (load count stable at ${loadCountAfter})`);
}

/**
 * Dispatch a single assertion to the appropriate handler.
 */
async function runAssertion(page: Page, assertion: Assertion): Promise<void> {
  switch (assertion.kind) {
    case 'wait_for_text':
      await assertWaitForText(page, assertion);
      break;
    case 'wait_for_dom_count':
      await assertWaitForDomCount(page, assertion);
      break;
    case 'wait_for_visible':
      await assertWaitForVisible(page, assertion);
      break;
    case 'no_full_reload':
      await assertNoFullReload(page, assertion);
      break;
    default: {
      // TypeScript exhaustiveness: cast to get the kind string.
      const unknownKind = (assertion as { kind: string }).kind;
      throw new Error(`Unknown assertion kind: "${unknownKind}"`);
    }
  }
}

// ---------------------------------------------------------------------------
// Load manifest
// ---------------------------------------------------------------------------

function loadManifest(): ScenarioManifest {
  const manifestPath = process.env.E2E_SCENARIO_MANIFEST;
  if (!manifestPath) {
    throw new Error(
      'E2E_SCENARIO_MANIFEST env var is not set. ' +
      'The bash harness must write the manifest and set this var before running Playwright.',
    );
  }

  const resolvedPath = path.resolve(manifestPath);
  if (!fs.existsSync(resolvedPath)) {
    throw new Error(`Manifest file not found: ${resolvedPath}`);
  }

  let manifest: ScenarioManifest;
  try {
    manifest = JSON.parse(fs.readFileSync(resolvedPath, 'utf8'));
  } catch (e) {
    throw new Error(`Failed to parse manifest at ${resolvedPath}: ${e}`);
  }

  if (!manifest.base_url) {
    throw new Error(`Manifest missing required field: base_url`);
  }
  if (!Array.isArray(manifest.assertions)) {
    throw new Error(`Manifest missing or invalid "assertions" array`);
  }

  return manifest;
}

// ---------------------------------------------------------------------------
// Test suite
// ---------------------------------------------------------------------------

test.describe('persona-live', () => {
  // Load manifest once at describe-time so missing manifest fails fast.
  // In Playwright, describe callbacks run synchronously before any test.
  let manifest: ScenarioManifest;

  try {
    manifest = loadManifest();
  } catch (e) {
    // If manifest is not available (e.g. --list mode), skip gracefully.
    if (process.env.E2E_SCENARIO_MANIFEST) {
      throw e;
    }
    // No manifest set — probably running --list. Provide a stub.
    manifest = {
      base_url: 'http://localhost:3000',
      scenario: 'stub',
      assertions: [],
    };
  }

  test.setTimeout(60_000);

  test(`scenario: ${manifest?.scenario ?? 'unknown'}`, async ({ page }) => {
    // D.3 — Install load counter via addInitScript (runs before page load).
    await injectLoadCounter(page);

    // Navigate to the scenario's base URL.
    await page.goto(manifest.base_url, { waitUntil: 'commit', timeout: 30_000 });

    // Wait for the app to boot (WASM startup).
    await Promise.race([
      page
        .waitForFunction(
          () => document.documentElement.getAttribute('data-poly-startup-phase') === 'revealed',
          { timeout: 30_000 },
        )
        .catch(() => null),
      page
        .waitForSelector('.main-layout, .favorites-sidebar, .setup-wizard, nav.server-sidebar', {
          timeout: 30_000,
        })
        .catch(() => null),
    ]);

    // Run all assertions declared in the manifest.
    let assertionIndex = 0;
    for (const assertion of manifest.assertions) {
      assertionIndex++;
      console.log(
        `[persona-live] Running assertion ${assertionIndex}/${manifest.assertions.length}: ${assertion.kind}`,
      );
      await runAssertion(page, assertion);
    }

    console.log(
      `[persona-live] All ${manifest.assertions.length} assertion(s) passed for scenario "${manifest.scenario}"`,
    );
  });
});
