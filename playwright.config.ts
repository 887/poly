import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests/e2e',
  timeout: 120_000,
  retries: 1,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: 'http://127.0.0.1:3001',
    screenshot: 'only-on-failure',
    video: 'off',
    trace: 'off',
  },
  projects: [
    {
      name: 'desktop',
      testMatch: /desktop\.spec\.ts|plugin-toggle\.spec\.ts/,
      use: {
        browserName: 'chromium',
        viewport: { width: 1280, height: 800 },
      },
    },
    {
      name: 'mobile',
      testMatch: /mobile\.spec\.ts/,
      use: {
        ...devices['iPhone 13'],
        browserName: 'chromium',
      },
    },
    {
      name: 'electron',
      testMatch: /electron\.spec\.ts/,
      use: {
        // Electron tests use the _electron API directly, no browser/viewport needed
      },
    },
    {
      name: 'discord-api',
      testMatch: /discord\/.*\.spec\.ts/,
      // Specs share one mock server and call /reseed in beforeEach, which
      // wipes auth tokens for sibling workers. Run serially to avoid races.
      fullyParallel: false,
      workers: 1,
      use: {
        // HTTP-only tests — no browser, no viewport.
        // Set DISCORD_MOCK_URL to point at poly-test-discord (default: http://localhost:9200).
        baseURL: process.env.DISCORD_MOCK_URL ?? 'http://localhost:9200',
      },
    },
    {
      // D.3 + D.4 — ForumComposer e2e specs driven by the persona-multi-agent harness.
      // poly-web must be running (started by the harness) at E2E_WEB_BASE_URL.
      name: 'forum-composer',
      testMatch: /specs\/forum-composer.*\.spec\.ts/,
      use: {
        browserName: 'chromium',
        viewport: { width: 1280, height: 800 },
        baseURL: process.env.E2E_WEB_BASE_URL ?? 'http://127.0.0.1:3000',
        headless: process.env.PLAYWRIGHT_HEADED !== '1',
      },
    },
  ],
});
