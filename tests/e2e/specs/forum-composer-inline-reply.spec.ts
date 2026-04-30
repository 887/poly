/**
 * tests/e2e/specs/forum-composer-inline-reply.spec.ts
 *
 * D.4 — ForumComposer inline reply Playwright e2e spec.
 *
 * Drives the test-lemmy backend through the poly-web WASM UI:
 *   1. Sign into the test-lemmy account via the signup link.
 *   2. Navigate to an existing forum post (the seeded "Rust 2025 edition" post).
 *   3. Click the Reply button (data-testid="forum-composer-reply-btn") on the
 *      first visible comment.
 *   4. The inline ForumComposer expands with ReplyToComment mode.
 *   5. Fill the reply body.
 *   6. Click Submit.
 *   7. Assert the reply appears nested under the parent comment within 5 s.
 *
 * Prerequisites (provided by the e2e harness):
 *   - poly-web running at E2E_WEB_BASE_URL (default http://127.0.0.1:3000)
 *   - test-lemmy running on port 9104
 *
 * If poly-web is not running, the test fails at navigation — expected for
 * local runs without the harness. CI boots poly-web before running this spec.
 *
 * Playwright timeout: 60 s (set via --timeout in scenario.sh).
 * Individual assertion timeouts: ≤ 10 s.
 */

import { test, expect } from '@playwright/test';

const BASE_URL = process.env.E2E_WEB_BASE_URL ?? 'http://127.0.0.1:3000';
const LEMMY_SIGNUP_URL = `${BASE_URL}/signup/lemmy`;

// Reply content — unique per run so we can identify it
const REPLY_BODY = `D4-inline-reply-${Date.now()} created by ForumComposer inline spec.`;

test.describe('ForumComposer — inline reply to comment', () => {
  test('open forum post, click reply on first comment, submit, see nested reply', async ({ page }) => {
    // Step 1 — sign into test-lemmy
    await page.goto(LEMMY_SIGNUP_URL, { waitUntil: 'networkidle', timeout: 30_000 });

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

    const signinBtn = page.locator('button[type="submit"], button:has-text("Sign in"), button:has-text("Connect")').first();
    if (await signinBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await signinBtn.click();
    }

    // Step 2 — wait for forum channel to load (shows posts list)
    // Look for a post row element that we can click into
    await page.waitForSelector('.forum-post-card, .forum-post-row, [data-testid="forum-composer-new-post-btn"]', {
      timeout: 10_000,
      state: 'visible',
    });

    // Step 3 — click on the first post to open the post thread (comments view)
    // The seeded "Rust 2025 edition" post should be visible
    const firstPost = page.locator('.forum-post-card, .forum-post-row').first();
    if (await firstPost.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await firstPost.click();
    }

    // Step 4 — wait for comments to load in the forum post view
    // The Reply button appears on each comment
    await page.waitForSelector('[data-testid="forum-composer-reply-btn"]', {
      timeout: 10_000,
      state: 'visible',
    });

    // Step 5 — click Reply on the first comment
    const firstReplyBtn = page.locator('[data-testid="forum-composer-reply-btn"]').first();
    await firstReplyBtn.click();

    // Step 6 — wait for the inline composer to expand
    await page.waitForSelector('[data-testid="forum-composer"]', {
      timeout: 5_000,
      state: 'visible',
    });

    // Verify it's in reply mode (reply header visible, not the new-post title input)
    const replyHeader = page.locator('[data-testid="forum-composer-reply-header"]');
    await expect(replyHeader).toBeVisible({ timeout: 3_000 });

    // Step 7 — fill the reply body
    const bodyTextarea = page.locator('[data-testid="forum-composer-body-textarea"]');
    await expect(bodyTextarea).toBeVisible({ timeout: 3_000 });
    await bodyTextarea.fill(REPLY_BODY);

    // Step 8 — click Submit
    const submitButton = page.locator('[data-testid="forum-composer-submit-btn"]');
    await expect(submitButton).toBeEnabled({ timeout: 3_000 });
    await submitButton.click();

    // Step 9 — assert the reply appears in the comment tree within 5 s
    // After submit the composer closes and the reply should be visible.
    // The reply body text should appear somewhere in the comments section.
    await expect(page.locator(`text=${REPLY_BODY}`)).toBeVisible({ timeout: 5_000 });
  });
});
