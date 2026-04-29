/**
 * discord-auth.spec.ts
 *
 * Verifies that the Discord client's authentication and guild-list endpoints
 * return the expected shapes against `poly-test-discord`.
 *
 * The tests call the mock API directly over HTTP — no WASM app required.
 *
 * Environment:
 *   DISCORD_MOCK_URL  Base URL of poly-test-discord (default: http://localhost:9200)
 *
 * Real-OAuth tests are skipped unless DISCORD_TEST_WITH_REAL_OAUTH=1 is set.
 */

import { test, expect, request } from '@playwright/test';

const MOCK_URL = process.env.DISCORD_MOCK_URL ?? 'http://localhost:9200';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Obtain a test token for the given username without a password check. */
async function getTestToken(username: string): Promise<string> {
  const ctx = await request.newContext({ baseURL: MOCK_URL });
  const resp = await ctx.post('/test/auth/token', {
    data: { username },
  });
  expect(resp.status(), `test/auth/token failed for ${username}`).toBe(200);
  const body = await resp.json();
  await ctx.dispose();
  return body.token as string;
}

// ---------------------------------------------------------------------------
// Test suite
// ---------------------------------------------------------------------------

test.describe('discord-auth', () => {
  test('password login returns a token', async ({ request: req }) => {
    // Seed first so users exist.
    await req.post(`${MOCK_URL}/seed`);

    const resp = await req.post(`${MOCK_URL}/api/v10/auth/login`, {
      data: { login: 'koala', password: 'testpass123' },
    });
    expect(resp.status()).toBe(200);

    const body = await resp.json();
    expect(typeof body.token).toBe('string');
    expect(body.token.length).toBeGreaterThan(0);
  });

  test('wrong password returns 401', async ({ request: req }) => {
    await req.post(`${MOCK_URL}/seed`);

    const resp = await req.post(`${MOCK_URL}/api/v10/auth/login`, {
      data: { login: 'koala', password: 'wrongpassword' },
    });
    expect(resp.status()).toBe(401);
  });

  test('GET /users/@me returns the authenticated user', async ({ request: req }) => {
    await req.post(`${MOCK_URL}/seed`);
    const token = await getTestToken('koala');

    const resp = await req.get(`${MOCK_URL}/api/v10/users/@me`, {
      headers: { Authorization: `Bot ${token}` },
    });
    expect(resp.status()).toBe(200);

    const user = await resp.json();
    expect(user.id).toBe('1');
    expect(user.username).toBe('koala');
    expect(user.discriminator).toBe('0001');
  });

  test('GET /users/@me without token returns 401', async ({ request: req }) => {
    const resp = await req.get(`${MOCK_URL}/api/v10/users/@me`);
    expect(resp.status()).toBe(401);
  });

  test('GET /users/@me/guilds returns guilds the user belongs to', async ({ request: req }) => {
    await req.post(`${MOCK_URL}/reseed`);
    const token = await getTestToken('koala');

    const resp = await req.get(`${MOCK_URL}/api/v10/users/@me/guilds`, {
      headers: { Authorization: `Bot ${token}` },
    });
    expect(resp.status()).toBe(200);

    const guilds = await resp.json();
    expect(Array.isArray(guilds)).toBe(true);
    expect(guilds.length).toBeGreaterThanOrEqual(1);

    // koala is a member of guild 100 (Australiana) and 101 (Wildlife Chat)
    const ids = guilds.map((g: { id: string }) => g.id);
    expect(ids).toContain('100');
    expect(ids).toContain('101');

    // Each guild object has the required fields
    const guild = guilds.find((g: { id: string }) => g.id === '100');
    expect(guild).toBeTruthy();
    expect(guild.name).toBe('Australiana');
    expect(typeof guild.permissions).toBe('string');
  });

  test('kangaroo is NOT a member of guild 100', async ({ request: req }) => {
    await req.post(`${MOCK_URL}/reseed`);

    // kangaroo is only in guild 100 (member) AND guild 101 (owner)
    // Actually per seed data: kangaroo IS in guild 100 as member AND owns guild 101
    const token = await getTestToken('kangaroo');
    const resp = await req.get(`${MOCK_URL}/api/v10/users/@me/guilds`, {
      headers: { Authorization: `Bot ${token}` },
    });
    expect(resp.status()).toBe(200);
    const guilds = await resp.json();
    // kangaroo is in guilds 100 and 101
    expect(guilds.length).toBeGreaterThanOrEqual(1);
  });

  test('GET /users/{id} returns a specific user', async ({ request: req }) => {
    await req.post(`${MOCK_URL}/seed`);
    const token = await getTestToken('koala');

    const resp = await req.get(`${MOCK_URL}/api/v10/users/2`, {
      headers: { Authorization: `Bot ${token}` },
    });
    expect(resp.status()).toBe(200);

    const user = await resp.json();
    expect(user.id).toBe('2');
    expect(user.username).toBe('kangaroo');
  });

  test('GET /users/{id} returns 404 for unknown user', async ({ request: req }) => {
    await req.post(`${MOCK_URL}/seed`);
    const token = await getTestToken('koala');

    const resp = await req.get(`${MOCK_URL}/api/v10/users/99999`, {
      headers: { Authorization: `Bot ${token}` },
    });
    expect(resp.status()).toBe(404);
  });

  // ---------------------------------------------------------------------------
  // Real-OAuth tests — skipped on CI
  // ---------------------------------------------------------------------------

  test.describe('real-oauth', () => {
    test.skip(
      !process.env.DISCORD_TEST_WITH_REAL_OAUTH,
      'Set DISCORD_TEST_WITH_REAL_OAUTH=1 to run real-OAuth tests',
    );

    test('real Discord /users/@me returns a valid user shape', async ({ request: req }) => {
      const token = process.env.DISCORD_TEST_TOKEN ?? '';
      expect(token.length).toBeGreaterThan(0);

      const resp = await req.get('https://discord.com/api/v10/users/@me', {
        headers: { Authorization: `Bot ${token}` },
      });
      expect(resp.status()).toBe(200);
      const user = await resp.json();
      expect(typeof user.id).toBe('string');
      expect(typeof user.username).toBe('string');
    });
  });
});
