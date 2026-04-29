/**
 * discord-context-menus.spec.ts
 *
 * Verifies that the DM right-click context menu operations dispatch the
 * correct HTTP requests to the Discord API. Tests call the `poly-test-discord`
 * mock server directly — no WASM app required.
 *
 * Operations under test:
 *   - Block user       → PUT /users/@me/relationships/{id}  { type: 2 }
 *   - Add friend       → PUT /users/@me/relationships/{id}  { type: 1 }
 *   - Remove friend    → DELETE /users/@me/relationships/{id}
 *   - Set user note    → PUT /users/@me/notes/{id}          { note: "..." }
 *   - Invite to server → POST /channels/{id}/invites
 *
 * Environment:
 *   DISCORD_MOCK_URL  Base URL of poly-test-discord (default: http://localhost:9200)
 */

import { test, expect, request } from '@playwright/test';

const MOCK_URL = process.env.DISCORD_MOCK_URL ?? 'http://localhost:9200';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function getTestToken(username: string): Promise<string> {
  const ctx = await request.newContext({ baseURL: MOCK_URL });
  const resp = await ctx.post('/test/auth/token', {
    data: { username },
  });
  expect(resp.status()).toBe(200);
  const body = await resp.json();
  await ctx.dispose();
  return body.token as string;
}

// ---------------------------------------------------------------------------
// Test suite
// ---------------------------------------------------------------------------

test.describe('discord-context-menus', () => {
  test.beforeEach(async ({ request: req }) => {
    await req.post(`${MOCK_URL}/reseed`);
  });

  // ── Block user ─────────────────────────────────────────────────────────────

  test('PUT /relationships/{id} with type=2 blocks a user (204)', async ({ request: req }) => {
    const token = await getTestToken('koala');

    // Block kangaroo (user 2)
    const resp = await req.put(`${MOCK_URL}/api/v10/users/@me/relationships/2`, {
      headers: { Authorization: `Bot ${token}` },
      data: { type: 2 },
    });
    expect(resp.status()).toBe(204);
  });

  test('block on unknown user returns 404', async ({ request: req }) => {
    const token = await getTestToken('koala');

    const resp = await req.put(`${MOCK_URL}/api/v10/users/@me/relationships/99999`, {
      headers: { Authorization: `Bot ${token}` },
      data: { type: 2 },
    });
    expect(resp.status()).toBe(404);
  });

  // ── Add friend ─────────────────────────────────────────────────────────────

  test('PUT /relationships/{id} with type=1 sends a friend request (204)', async ({
    request: req,
  }) => {
    const token = await getTestToken('koala');

    const resp = await req.put(`${MOCK_URL}/api/v10/users/@me/relationships/3`, {
      headers: { Authorization: `Bot ${token}` },
      data: { type: 1 },
    });
    expect(resp.status()).toBe(204);
  });

  // ── Remove friend / unblock ─────────────────────────────────────────────────

  test('DELETE /relationships/{id} removes friend or unblocks (204)', async ({
    request: req,
  }) => {
    const token = await getTestToken('koala');

    const resp = await req.delete(`${MOCK_URL}/api/v10/users/@me/relationships/2`, {
      headers: { Authorization: `Bot ${token}` },
    });
    expect(resp.status()).toBe(204);
  });

  test('DELETE /relationships/{id} on unknown user returns 404', async ({ request: req }) => {
    const token = await getTestToken('koala');

    const resp = await req.delete(`${MOCK_URL}/api/v10/users/@me/relationships/99999`, {
      headers: { Authorization: `Bot ${token}` },
    });
    expect(resp.status()).toBe(404);
  });

  // ── Set user note ───────────────────────────────────────────────────────────

  test('PUT /notes/{id} sets a private note (204)', async ({ request: req }) => {
    const token = await getTestToken('koala');

    const resp = await req.put(`${MOCK_URL}/api/v10/users/@me/notes/2`, {
      headers: { Authorization: `Bot ${token}` },
      data: { note: 'My kangaroo friend' },
    });
    expect(resp.status()).toBe(204);
  });

  test('PUT /notes/{id} with empty note clears it (204)', async ({ request: req }) => {
    const token = await getTestToken('koala');

    const resp = await req.put(`${MOCK_URL}/api/v10/users/@me/notes/2`, {
      headers: { Authorization: `Bot ${token}` },
      data: { note: '' },
    });
    expect(resp.status()).toBe(204);
  });

  // ── Invite to server ────────────────────────────────────────────────────────

  test('POST /channels/{id}/invites returns an invite code', async ({ request: req }) => {
    const token = await getTestToken('koala');

    const resp = await req.post(`${MOCK_URL}/api/v10/channels/200/invites`, {
      headers: { Authorization: `Bot ${token}` },
      data: { max_age: 86400, max_uses: 0, unique: true },
    });
    expect(resp.status()).toBe(200);

    const invite = await resp.json();
    expect(typeof invite.code).toBe('string');
    expect(invite.code.length).toBeGreaterThan(0);
    expect(invite.channel.id).toBe('200');
  });

  test('POST /channels/{id}/invites on unknown channel returns 404', async ({ request: req }) => {
    const token = await getTestToken('koala');

    const resp = await req.post(`${MOCK_URL}/api/v10/channels/99999/invites`, {
      headers: { Authorization: `Bot ${token}` },
      data: { max_age: 86400, max_uses: 0 },
    });
    expect(resp.status()).toBe(404);
  });

  // ── Auth guard ──────────────────────────────────────────────────────────────

  test('PUT /relationships without token returns 401', async ({ request: req }) => {
    const resp = await req.put(`${MOCK_URL}/api/v10/users/@me/relationships/2`, {
      data: { type: 2 },
    });
    expect(resp.status()).toBe(401);
  });

  test('PUT /notes without token returns 401', async ({ request: req }) => {
    const resp = await req.put(`${MOCK_URL}/api/v10/users/@me/notes/2`, {
      data: { note: 'test' },
    });
    expect(resp.status()).toBe(401);
  });
});
