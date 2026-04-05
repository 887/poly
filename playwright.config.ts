import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests/e2e',
  timeout: 45_000,
  retries: 1,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: 'http://127.0.0.1:3000',
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
  ],
});
