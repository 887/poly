/**
 * tests/e2e/lib/signup-link-spec.ts
 *
 * Factory helper for signup-link Playwright specs.
 *
 * Each per-backend spec at tests/e2e/signup/<backend>-signup.spec.ts calls
 * one of the three exported helpers:
 *
 *   makeExternalSignupSpec(backendId, expectedHrefPattern, options?)
 *   makeInAppSignupSpec(backendId, expectedRoute)
 *   makeNotSupportedSignupSpec(backendId)
 *
 * Mock-mode (CI default): navigate to /signup/<backend>, locate the element
 * by data-testid, assert the href regex. Never clicks the link to avoid
 * pop-out tab races.
 *
 * Real-network mode (POLY_SIGNUP_E2E_REAL=1): additionally click the link,
 * capture the new tab via waitForEvent('page'), and assert HTTP status < 400.
 */

import { test, expect, type Page } from '@playwright/test';

const WEB_BASE = process.env.POLY_WEB_BASE ?? 'http://localhost:3000';
const REAL_MODE = process.env.POLY_SIGNUP_E2E_REAL === '1';

/** Options accepted by makeExternalSignupSpec. */
export interface ExternalSignupOptions {
  /**
   * If set, the test fills the server-URL input before reading the href.
   * Used for instance-parameterised backends (matrix, stoat, lemmy, forgejo, github).
   */
  fillServerUrl?: string;
  /** Timeout in ms for navigation/element checks. Default 30 000. */
  timeout?: number;
}

/**
 * Navigate to the backend's signup route and return the page.
 * Waits for the signup-form-container to appear.
 */
async function gotoSignupPage(page: Page, backendId: string, timeout: number): Promise<void> {
  await page.goto(`${WEB_BASE}/signup/${backendId}`, { timeout });
  await page.waitForSelector('[data-testid="signup-form-container"]', { timeout });
}

/**
 * Create a Playwright test suite for an external-URL signup link.
 *
 * The test locates `[data-testid="register-link-{backendId}"]` and asserts
 * its `href` attribute matches `expectedHrefPattern`.
 *
 * In real-network mode it also clicks the link and verifies the new tab
 * responds with HTTP status < 400.
 */
export function makeExternalSignupSpec(
  backendId: string,
  expectedHrefPattern: RegExp,
  options: ExternalSignupOptions = {},
): void {
  const timeout = options.timeout ?? 30_000;
  const testid = `register-link-${backendId}`;

  test.describe(`signup-link-${backendId}`, () => {
    test('register link is present with correct href (mock-mode)', async ({ page }) => {
      await gotoSignupPage(page, backendId, timeout);

      if (options.fillServerUrl) {
        // Some backends (matrix, stoat, lemmy, forgejo, github-enterprise) have
        // a server-URL input. Fill it so the href reflects the custom instance.
        const serverInput = page.locator('[data-testid="server-url-input"]');
        if (await serverInput.count() > 0) {
          await serverInput.fill(options.fillServerUrl, { timeout });
          // Give the component a tick to update the href.
          await page.waitForTimeout(200);
        }
      }

      const link = page.locator(`[data-testid="${testid}"]`);
      await expect(link).toBeVisible({ timeout });

      const href = await link.getAttribute('href', { timeout });
      expect(href).toBeTruthy();
      expect(href).toMatch(expectedHrefPattern);
    });

    test.skip(
      !REAL_MODE,
      'Set POLY_SIGNUP_E2E_REAL=1 to run real-network follow-through',
    );

    test('clicking the link opens a reachable page (real-network mode)', async ({
      page,
      context,
    }) => {
      await gotoSignupPage(page, backendId, timeout);

      if (options.fillServerUrl) {
        const serverInput = page.locator('[data-testid="server-url-input"]');
        if (await serverInput.count() > 0) {
          await serverInput.fill(options.fillServerUrl, { timeout });
          await page.waitForTimeout(200);
        }
      }

      const link = page.locator(`[data-testid="${testid}"]`);
      await expect(link).toBeVisible({ timeout });

      // Capture new tab before clicking.
      const [newPage] = await Promise.all([
        context.waitForEvent('page', { timeout: 60_000 }),
        link.click({ timeout }),
      ]);

      // Wait for the new page to load.
      await newPage.waitForLoadState('domcontentloaded', { timeout: 60_000 });

      // HTTP-level assertion: fetch HEAD and expect non-error status.
      const url = newPage.url();
      const resp = await fetch(url, { method: 'HEAD' });
      expect(resp.status).toBeLessThan(400);

      await newPage.close();
    });
  });
}

/**
 * Create a Playwright test suite for an in-app signup link (poly-server).
 *
 * Navigates to the picker (/signup), clicks the register link, and asserts:
 *   1. The URL changed to the expected in-app route.
 *   2. signup-form-container is visible on the destination page.
 */
export function makeInAppSignupSpec(backendId: string, expectedRoute: string): void {
  const timeout = 30_000;
  const testid = `register-link-${backendId}`;

  test.describe(`signup-link-${backendId} (in-app)`, () => {
    test('register link navigates to in-app signup route', async ({ page }) => {
      // Start at the picker page so the link is visible (it hides when already
      // on the target route, per Phase D logic).
      await page.goto(`${WEB_BASE}/signup`, { timeout });
      await page.waitForSelector('[data-testid="signup-picker-container"]', { timeout });

      const link = page.locator(`[data-testid="${testid}"]`);
      await expect(link).toBeVisible({ timeout });

      await link.click({ timeout });

      // After click: URL should end with the expected in-app route.
      await page.waitForURL(`**${expectedRoute}`, { timeout });
      expect(page.url()).toContain(expectedRoute);

      // And the signup form container should render.
      await expect(
        page.locator('[data-testid="signup-form-container"]'),
      ).toBeVisible({ timeout });
    });

    test('register link is hidden when already on the in-app route', async ({ page }) => {
      // Navigate directly to the in-app route — Phase D hides the link here.
      await page.goto(`${WEB_BASE}${expectedRoute}`, { timeout });
      await page.waitForSelector('[data-testid="signup-form-container"]', { timeout });

      const link = page.locator(`[data-testid="${testid}"]`);
      // The link must not be present (or not visible) when already on the route.
      await expect(link).toHaveCount(0, { timeout });
    });
  });
}

/**
 * Create a Playwright test suite for a NotSupported backend (demo).
 *
 * Asserts that no `[data-testid^="register-link-"]` element is rendered
 * anywhere within the signup-form-container.
 */
export function makeNotSupportedSignupSpec(backendId: string): void {
  const timeout = 30_000;

  test.describe(`signup-link-${backendId} (not-supported)`, () => {
    test('no register link rendered for unsupported backend', async ({ page }) => {
      await page.goto(`${WEB_BASE}/signup/${backendId}`, { timeout });
      // Wait for the form container (or an "add account" root) to appear.
      // Use a broad selector so the test doesn't fail if signup-form-container
      // itself is absent for the demo backend.
      await page.waitForSelector('body', { timeout });

      // Assert no register-link element for this backend exists anywhere.
      const link = page.locator(`[data-testid="register-link-${backendId}"]`);
      await expect(link).toHaveCount(0, { timeout });
    });

    test('no register-link-* element present in signup-form scope', async ({ page }) => {
      await page.goto(`${WEB_BASE}/signup/${backendId}`, { timeout });
      await page.waitForSelector('body', { timeout });

      // Even a generic register-link-* inside a form container must be absent.
      const anyLink = page.locator(
        '[data-testid="signup-form-container"] [data-testid^="register-link-"]',
      );
      await expect(anyLink).toHaveCount(0, { timeout });
    });
  });
}
