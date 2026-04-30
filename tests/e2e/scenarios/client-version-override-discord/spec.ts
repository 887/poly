/**
 * Playwright spec: client-version-override-discord
 *
 * Drives the Settings UI to set a version override for the Discord backend,
 * then asserts the override persisted. Wire-level assertions (User-Agent on
 * the mock server) are done in scenario.sh via curl; this spec covers only
 * the DOM-layer interactions and the MCP-layer persistence check via the
 * effective-version text node.
 *
 * Selectors: data-testid exclusively (stable, no role/text coupling).
 *
 * Phase H of plan-client-version-override-and-sandbox.md
 */

import { test, expect, Page } from '@playwright/test';

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const BACKEND_ID = 'discord';
const OVERRIDE_VERSION = 'e2e-test/9.9.9';

// data-testid selectors scoped to the discord backend card
const SEL = {
  section: '[data-testid="client-settings-section"]',
  card: `[data-testid="client-settings-backend-${BACKEND_ID}-card"]`,
  effectiveVersion: `[data-testid="client-settings-backend-${BACKEND_ID}-version-effective"]`,
  overrideToggle: `[data-testid="client-settings-backend-${BACKEND_ID}-version-override-toggle"]`,
  overrideInput: `[data-testid="client-settings-backend-${BACKEND_ID}-version-override-input"]`,
  overrideSave: `[data-testid="client-settings-backend-${BACKEND_ID}-version-override-save"]`,
  overrideClear: `[data-testid="client-settings-backend-${BACKEND_ID}-version-override-clear"]`,
};

// ---------------------------------------------------------------------------
// Page Object: ClientSettingsPage
// ---------------------------------------------------------------------------

class ClientSettingsPage {
  constructor(private readonly page: Page) {}

  /** Navigate to the app root and wait for the WASM startup to complete. */
  async open(): Promise<void> {
    await this.page.goto('/', { waitUntil: 'commit' });

    // Wait for the WASM boot — either the startup-phase attribute OR a layout node.
    await Promise.race([
      this.page
        .waitForFunction(
          () =>
            document.documentElement.getAttribute('data-poly-startup-phase') ===
            'revealed',
          { timeout: 90_000 },
        )
        .catch(() => null),
      this.page
        .waitForSelector('.main-layout, .favorites-sidebar, .setup-wizard, nav.server-sidebar', {
          timeout: 90_000,
        })
        .catch(() => null),
    ]);

    // Skip setup wizard if it appeared.
    const wizardVisible = await this.page
      .locator('.setup-start-btn')
      .isVisible({ timeout: 2_000 })
      .catch(() => false);
    if (wizardVisible) {
      await this.page.locator('.setup-start-btn').click();
      await this.page.waitForSelector('.main-layout, .favorites-sidebar, nav.server-sidebar', {
        timeout: 15_000,
      });
    }
  }

  /** Navigate to the Settings page (route /settings or equivalent). */
  async goToSettings(): Promise<void> {
    // Try the URL-based navigation first (fastest).
    await this.page.goto('/settings', { waitUntil: 'domcontentloaded' });

    // Wait for the client-settings section to appear.
    await this.page.waitForSelector(SEL.section, { timeout: 30_000 });
  }

  /** Scroll the client-settings section into view. */
  async scrollToClientSettings(): Promise<void> {
    await this.page.locator(SEL.section).scrollIntoViewIfNeeded();
    await expect(this.page.locator(SEL.section)).toBeVisible();
  }

  /** Expand the Discord backend card (click if needed). */
  async expandDiscordCard(): Promise<void> {
    const card = this.page.locator(SEL.card);
    await expect(card).toBeVisible({ timeout: 10_000 });
    // Cards may be collapsed — click to expand if override-toggle is not yet visible.
    const toggleVisible = await this.page
      .locator(SEL.overrideToggle)
      .isVisible({ timeout: 2_000 })
      .catch(() => false);
    if (!toggleVisible) {
      await card.click();
      await this.page.waitForSelector(SEL.overrideToggle, { timeout: 10_000 });
    }
  }

  /** Enable override input by clicking the toggle. */
  async enableOverrideInput(): Promise<void> {
    const toggle = this.page.locator(SEL.overrideToggle);
    await expect(toggle).toBeVisible({ timeout: 5_000 });
    await toggle.click();
    // Input should now be visible.
    await expect(this.page.locator(SEL.overrideInput)).toBeVisible({ timeout: 5_000 });
  }

  /** Fill the override input and click Save. */
  async setVersionOverride(version: string): Promise<void> {
    const input = this.page.locator(SEL.overrideInput);
    await input.fill(version);
    await this.page.locator(SEL.overrideSave).click();
  }

  /** Read the effective-version text currently shown in the card. */
  async readEffectiveVersion(): Promise<string> {
    return this.page.locator(SEL.effectiveVersion).innerText();
  }

  /** Click the clear-override button. */
  async clearOverride(): Promise<void> {
    const clearBtn = this.page.locator(SEL.overrideClear);
    await expect(clearBtn).toBeVisible({ timeout: 5_000 });
    await clearBtn.click();
  }
}

// ---------------------------------------------------------------------------
// Test suite
// ---------------------------------------------------------------------------

test.describe('client-version-override-discord', () => {
  // Longer timeout: WASM boot can take up to 90s on a cold cache.
  test.setTimeout(180_000);

  test('set version override, verify it persists, then clear it', async ({ page }) => {
    const settings = new ClientSettingsPage(page);

    // ── Step 1: Boot the app ───────────────────────────────────────────────
    await settings.open();

    // ── Step 2: Navigate to Settings ──────────────────────────────────────
    await settings.goToSettings();
    await settings.scrollToClientSettings();

    // ── Step 3: Expand Discord card ────────────────────────────────────────
    await settings.expandDiscordCard();

    // ── Step 4: Enable override input ─────────────────────────────────────
    await settings.enableOverrideInput();

    // ── Step 5: Set version override ──────────────────────────────────────
    await settings.setVersionOverride(OVERRIDE_VERSION);

    // ── Step 6: Verify effective version shows the override ────────────────
    // Poll the effective-version node: the UI updates reactively after Save.
    await expect(async () => {
      const text = await settings.readEffectiveVersion();
      expect(text).toContain(OVERRIDE_VERSION);
    }).toPass({ timeout: 15_000, intervals: [500, 1_000, 2_000] });

    // ── Step 7: Clear the override ─────────────────────────────────────────
    await settings.clearOverride();

    // ── Step 8: Verify the effective version no longer shows the override ──
    // After clear the effective version reverts to the backend default.
    await expect(async () => {
      const text = await settings.readEffectiveVersion();
      expect(text).not.toContain(OVERRIDE_VERSION);
    }).toPass({ timeout: 15_000, intervals: [500, 1_000, 2_000] });
  });
});
