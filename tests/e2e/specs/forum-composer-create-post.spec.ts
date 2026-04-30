/**
 * tests/e2e/specs/forum-composer-create-post.spec.ts
 *
 * D.3 — ForumComposer "Create Post" Playwright e2e spec.
 *
 * Drives the test-lemmy backend through the poly-web WASM UI:
 *   1. Sign into the test-lemmy account via the signup link.
 *   2. Navigate to a Lemmy forum community channel.
 *   3. Click the "Create Post" button (data-testid="forum-composer-new-post-btn").
 *   4. Fill in a title and body in the ForumComposer form.
 *   5. Click Submit.
 *   6. Assert the new post title appears in the forum list within 5 s.
 *
 * Prerequisites (provided by the e2e harness):
 *   - poly-web running at E2E_WEB_BASE_URL (default http://127.0.0.1:3000)
 *   - test-lemmy running on port 9104
 *
 * If poly-web is not running, the test will fail at navigation — this is
 * expected and acceptable for local runs where poly-web is not active.
 * CI runs the test after the harness boots poly-web.
 *
 * Playwright timeout: 60 s (set via --timeout in scenario.sh).
 * Individual assertion timeouts: ≤ 10 s to stay well under the cap.
 */

import { test, expect } from '@playwright/test';

const BASE_URL = process.env.E2E_WEB_BASE_URL ?? 'http://127.0.0.1:3000';

// Test-lemmy seed data constants (from servers/test-lemmy/src/state.rs)
// Lemmy backend account credentials
const LEMMY_SIGNUP_URL = `${BASE_URL}/signup/lemmy`;

// New post content for this test run — unique so we can identify it in the list
const POST_TITLE = `D3-test-post-${Date.now()}`;
const POST_BODY = 'This post was created by the D.3 ForumComposer Playwright spec.';

test.describe('ForumComposer — create new post', () => {
  test('navigate to lemmy forum, open composer, submit post, see it in list', async ({ page }) => {
    // Step 1 — navigate to the signup page and sign into test-lemmy
    // The signup flow sets up the account and redirects to the forum view.
    await page.goto(LEMMY_SIGNUP_URL, { waitUntil: 'networkidle', timeout: 30_000 });

    // Fill in test-lemmy credentials: server URL + username/password
    // The signup form has a server URL field and credential fields.
    const serverUrlInput = page.locator('input[name="server_url"], input[placeholder*="instance"], input[type="url"]').first();
    if (await serverUrlInput.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await serverUrlInput.fill('http://127.0.0.1:9104');
    }

    const usernameInput = page.locator('input[name="username"], input[placeholder*="username"], input[type="text"]').first();
    if (await usernameInput.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await usernameInput.fill('testuser');
    }

    const passwordInput = page.locator('input[name="password"], input[type="password"]').first();
    if (await passwordInput.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await passwordInput.fill('password123');
    }

    const submitBtn = page.locator('button[type="submit"], button:has-text("Sign in"), button:has-text("Connect")').first();
    if (await submitBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await submitBtn.click();
    }

    // Step 2 — wait for the forum channel view to load.
    // The forum channel shows the "Create Post" button when an account is active.
    await page.waitForSelector('[data-testid="forum-composer-new-post-btn"]', {
      timeout: 10_000,
      state: 'visible',
    });

    // Step 3 — click "Create Post"
    await page.click('[data-testid="forum-composer-new-post-btn"]');

    // Step 4 — wait for the composer to appear
    await page.waitForSelector('[data-testid="forum-composer"]', {
      timeout: 5_000,
      state: 'visible',
    });

    // Step 5 — fill in title
    const titleInput = page.locator('[data-testid="forum-composer-title-input"]');
    await expect(titleInput).toBeVisible({ timeout: 3_000 });
    await titleInput.fill(POST_TITLE);

    // Step 6 — fill in body via the textarea
    const bodyTextarea = page.locator('[data-testid="forum-composer-body-textarea"]');
    await expect(bodyTextarea).toBeVisible({ timeout: 3_000 });
    await bodyTextarea.fill(POST_BODY);

    // Step 7 — click Submit
    const submitButton = page.locator('[data-testid="forum-composer-submit-btn"]');
    await expect(submitButton).toBeEnabled({ timeout: 3_000 });
    await submitButton.click();

    // Step 8 — assert the new post title appears in the forum list within 5 s
    await expect(page.locator(`text=${POST_TITLE}`)).toBeVisible({ timeout: 5_000 });
  });
});
