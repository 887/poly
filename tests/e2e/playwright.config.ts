/**
 * tests/e2e/playwright.config.ts
 *
 * Playwright configuration for the persona multi-agent e2e harness (Phase D).
 *
 * This is a SEPARATE config from the root playwright.config.ts.
 * The root config covers desktop/mobile/electron UI tests.
 * This config covers persona live-UI assertions driven by the bash harness.
 *
 * Usage (from the bash harness):
 *   npx playwright test --config tests/e2e/playwright.config.ts
 *
 * Usage (list tests, syntax check):
 *   npx playwright test --config tests/e2e/playwright.config.ts --list
 *
 * The base URL comes from E2E_SCENARIO_MANIFEST → base_url field, not from
 * this config. The spec reads the env var and navigates to the correct URL.
 * We set a permissive baseURL here as a fallback only.
 */

import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './specs',
  timeout: 60_000,
  retries: 0,
  reporter: [
    ['list'],
    ['html', { open: 'never', outputFolder: 'playwright-report' }],
    ['json', { outputFile: 'playwright-report/results.json' }],
  ],
  use: {
    baseURL: process.env.E2E_WEB_BASE_URL ?? 'http://127.0.0.1:3000',
    screenshot: 'only-on-failure',
    video: 'off',
    trace: 'off',
  },
  projects: [
    {
      name: 'persona-live',
      testMatch: /persona-live\.spec\.ts/,
      use: {
        browserName: 'chromium',
        viewport: { width: 1280, height: 800 },
        // Headless by default in CI; override with PLAYWRIGHT_HEADED=1 locally.
        headless: process.env.PLAYWRIGHT_HEADED !== '1',
      },
    },
  ],
});
