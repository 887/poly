import { test, expect, Page } from '@playwright/test';

// Complete setup wizard + navigate to plugin settings via UI clicks.
// Dioxus is a client-side SPA — direct URL navigation doesn't work
// without the app being fully loaded first.
async function goToPlugins(page: Page) {
  await page.goto('/');

  // Wait for the WASM app to load (either wizard or main app)
  await page.waitForSelector('.setup-wizard, .main-layout, .favorites-sidebar', { timeout: 20_000 });

  // Dismiss setup wizard if present (fresh browser context = first launch)
  const wizard = page.locator('.setup-start-btn');
  if (await wizard.isVisible({ timeout: 2_000 }).catch(() => false)) {
    await wizard.click();
    // Wait for main app to appear after wizard completes
    await page.waitForSelector('.favorites-sidebar, .main-layout', { timeout: 15_000 });
  }

  // Click the settings gear icon in the sidebar
  await page.locator('.icon-settings').click();

  // Wait for settings page to render
  await page.waitForSelector('.settings-nav', { timeout: 10_000 });

  // Click the "Plugins" nav item in the settings sidebar
  const pluginsNav = page.locator('.settings-nav-item', { hasText: /plugin/i });
  await pluginsNav.click();

  // Wait for plugin rows to appear
  await page.waitForSelector('.plugin-row', { timeout: 10_000 });
}

// Find a native plugin row by its display name.
function pluginRow(page: Page, name: string) {
  return page.locator('.plugin-row', { hasText: name }).first();
}

// Get the toggle checkbox inside a plugin row.
function pluginCheckbox(page: Page, name: string) {
  return pluginRow(page, name).locator('input[type="checkbox"]');
}

// ---------------------------------------------------------------------------
// Stoat plugin toggle
// ---------------------------------------------------------------------------

test.describe('Stoat plugin toggle', () => {
  test('stoat row is visible with correct badge', async ({ page }) => {
    await goToPlugins(page);
    const row = pluginRow(page, 'Stoat');
    await expect(row).toBeVisible();
    await expect(row.locator('.plugin-type-badge')).toContainText(/native/i);
  });

  test('stoat can be disabled and re-enabled', async ({ page }) => {
    await goToPlugins(page);
    const checkbox = pluginCheckbox(page, 'Stoat');

    const wasChecked = await checkbox.isChecked();

    // Toggle off
    if (wasChecked) {
      await checkbox.click();
      await expect(checkbox).not.toBeChecked();
    }

    // Toggle back on
    await checkbox.click();
    await expect(checkbox).toBeChecked();

    // Restore original state
    if (!wasChecked) {
      await checkbox.click();
      await expect(checkbox).not.toBeChecked();
    }
  });

  test('stoat toggle persists after page reload', async ({ page }) => {
    await goToPlugins(page);
    const checkbox = pluginCheckbox(page, 'Stoat');

    const initialState = await checkbox.isChecked();

    // Toggle
    await checkbox.click();
    const newState = !initialState;
    await expect(checkbox).toBeChecked({ checked: newState });

    // Reload — app should skip wizard (setup_complete persisted in IndexedDB)
    await page.reload();
    await page.waitForSelector('.favorites-sidebar, .main-layout', { timeout: 20_000 });
    await page.locator('.icon-settings').click();
    await page.waitForSelector('.settings-nav', { timeout: 10_000 });
    await page.locator('.settings-nav-item', { hasText: /plugin/i }).click();
    await page.waitForSelector('.plugin-row', { timeout: 10_000 });

    const afterReload = await pluginCheckbox(page, 'Stoat').isChecked();
    expect(afterReload).toBe(newState);

    // Restore
    if (afterReload !== initialState) {
      await pluginCheckbox(page, 'Stoat').click();
    }
  });
});

// ---------------------------------------------------------------------------
// Matrix plugin toggle
// ---------------------------------------------------------------------------

test.describe('Matrix plugin toggle', () => {
  test('matrix row is visible with correct badge', async ({ page }) => {
    await goToPlugins(page);
    const row = pluginRow(page, 'Matrix');
    await expect(row).toBeVisible();
    await expect(row.locator('.plugin-type-badge')).toContainText(/native/i);
  });

  test('matrix can be disabled and re-enabled', async ({ page }) => {
    await goToPlugins(page);
    const checkbox = pluginCheckbox(page, 'Matrix');

    const wasChecked = await checkbox.isChecked();

    if (wasChecked) {
      await checkbox.click();
      await expect(checkbox).not.toBeChecked();
    }

    await checkbox.click();
    await expect(checkbox).toBeChecked();

    if (!wasChecked) {
      await checkbox.click();
      await expect(checkbox).not.toBeChecked();
    }
  });

  test('matrix toggle persists after page reload', async ({ page }) => {
    await goToPlugins(page);
    const checkbox = pluginCheckbox(page, 'Matrix');

    const initialState = await checkbox.isChecked();

    await checkbox.click();
    const newState = !initialState;
    await expect(checkbox).toBeChecked({ checked: newState });

    await page.reload();
    await page.waitForSelector('.favorites-sidebar, .main-layout', { timeout: 20_000 });
    await page.locator('.icon-settings').click();
    await page.waitForSelector('.settings-nav', { timeout: 10_000 });
    await page.locator('.settings-nav-item', { hasText: /plugin/i }).click();
    await page.waitForSelector('.plugin-row', { timeout: 10_000 });

    const afterReload = await pluginCheckbox(page, 'Matrix').isChecked();
    expect(afterReload).toBe(newState);

    if (afterReload !== initialState) {
      await pluginCheckbox(page, 'Matrix').click();
    }
  });
});

// ---------------------------------------------------------------------------
// Both plugins together
// ---------------------------------------------------------------------------

test.describe('Multi-plugin interaction', () => {
  test('disabling one plugin does not affect the other', async ({ page }) => {
    await goToPlugins(page);

    const stoatCheckbox = pluginCheckbox(page, 'Stoat');
    const matrixCheckbox = pluginCheckbox(page, 'Matrix');

    // Ensure both enabled
    if (!(await stoatCheckbox.isChecked())) await stoatCheckbox.click();
    if (!(await matrixCheckbox.isChecked())) await matrixCheckbox.click();
    await expect(stoatCheckbox).toBeChecked();
    await expect(matrixCheckbox).toBeChecked();

    // Disable Stoat — Matrix unaffected
    await stoatCheckbox.click();
    await expect(stoatCheckbox).not.toBeChecked();
    await expect(matrixCheckbox).toBeChecked();

    // Disable Matrix — Stoat still off
    await matrixCheckbox.click();
    await expect(matrixCheckbox).not.toBeChecked();
    await expect(stoatCheckbox).not.toBeChecked();

    // Re-enable both
    await stoatCheckbox.click();
    await matrixCheckbox.click();
    await expect(stoatCheckbox).toBeChecked();
    await expect(matrixCheckbox).toBeChecked();
  });
});
