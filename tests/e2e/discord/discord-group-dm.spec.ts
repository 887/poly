/**
 * discord-group-dm.spec.ts
 *
 * Verifies DM and group-DM lifecycle operations against `poly-test-discord`.
 *
 * Operations under test:
 *   - Open DM      → POST /users/@me/channels
 *   - List DMs     → GET  /users/@me/channels
 *   - Leave group  → DELETE /channels/{id}  (DM or group DM)
 *   - Add user     → PUT /channels/{id}/recipients/{uid}
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

test.describe('discord-group-dm', () => {
  test.beforeEach(async ({ request: req }) => {
    await req.post(`${MOCK_URL}/reseed`);
  });

  // ── Open DM ────────────────────────────────────────────────────────────────

  test('POST /users/@me/channels opens a DM and returns channel object', async ({
    request: req,
  }) => {
    const token = await getTestToken('koala');

    const resp = await req.post(`${MOCK_URL}/api/v10/users/@me/channels`, {
      headers: { Authorization: `Bot ${token}` },
      data: { recipient_id: '2' },
    });
    expect(resp.status()).toBe(200);

    const ch = await resp.json();
    expect(typeof ch.id).toBe('string');
    // Channel type 1 = DM (Discord CHANNEL_TYPE_DM)
    expect(ch.type).toBe(1);
  });

  test('opening the same DM twice returns the same channel id', async ({ request: req }) => {
    const token = await getTestToken('koala');

    const resp1 = await req.post(`${MOCK_URL}/api/v10/users/@me/channels`, {
      headers: { Authorization: `Bot ${token}` },
      data: { recipient_id: '3' },
    });
    const ch1 = await resp1.json();

    const resp2 = await req.post(`${MOCK_URL}/api/v10/users/@me/channels`, {
      headers: { Authorization: `Bot ${token}` },
      data: { recipient_id: '3' },
    });
    const ch2 = await resp2.json();

    expect(ch1.id).toBe(ch2.id);
  });

  // ── List DMs ───────────────────────────────────────────────────────────────

  test('GET /users/@me/channels returns DM channels', async ({ request: req }) => {
    const token = await getTestToken('koala');

    const resp = await req.get(`${MOCK_URL}/api/v10/users/@me/channels`, {
      headers: { Authorization: `Bot ${token}` },
    });
    expect(resp.status()).toBe(200);

    const dms = await resp.json();
    expect(Array.isArray(dms)).toBe(true);
    // Seed data includes channel 300 (Private DM)
    expect(dms.length).toBeGreaterThanOrEqual(1);
    // All returned channels should be DM type (1) or Group DM type (3)
    for (const ch of dms) {
      expect([1, 3]).toContain(ch.type);
    }
  });

  // ── Leave DM (DELETE /channels/{id}) ──────────────────────────────────────

  test('DELETE /channels/{id} closes a DM and returns 204', async ({ request: req }) => {
    const token = await getTestToken('koala');

    // Open a fresh DM to get a known channel ID.
    const openResp = await req.post(`${MOCK_URL}/api/v10/users/@me/channels`, {
      headers: { Authorization: `Bot ${token}` },
      data: { recipient_id: '2' },
    });
    const ch = await openResp.json();
    const channelId = ch.id as string;

    // Delete (leave) the DM.
    const deleteResp = await req.delete(`${MOCK_URL}/api/v10/channels/${channelId}`, {
      headers: { Authorization: `Bot ${token}` },
    });
    expect(deleteResp.status()).toBe(204);
  });

  test('DELETE /channels/{id} on unknown channel returns 404', async ({ request: req }) => {
    const token = await getTestToken('koala');

    const resp = await req.delete(`${MOCK_URL}/api/v10/channels/99999`, {
      headers: { Authorization: `Bot ${token}` },
    });
    expect(resp.status()).toBe(404);
  });

  test('deleted DM no longer appears in channel list', async ({ request: req }) => {
    const token = await getTestToken('koala');

    // Open a DM channel.
    const openResp = await req.post(`${MOCK_URL}/api/v10/users/@me/channels`, {
      headers: { Authorization: `Bot ${token}` },
      data: { recipient_id: '3' },
    });
    const ch = await openResp.json();
    const channelId = ch.id as string;

    // Delete it.
    await req.delete(`${MOCK_URL}/api/v10/channels/${channelId}`, {
      headers: { Authorization: `Bot ${token}` },
    });

    // List DMs — deleted channel should not appear.
    const listResp = await req.get(`${MOCK_URL}/api/v10/users/@me/channels`, {
      headers: { Authorization: `Bot ${token}` },
    });
    const dms = await listResp.json();
    const ids = dms.map((c: { id: string }) => c.id);
    expect(ids).not.toContain(channelId);
  });

  // ── Add user to group DM ───────────────────────────────────────────────────

  test('PUT /channels/{id}/recipients/{uid} adds a user to group DM (204)', async ({
    request: req,
  }) => {
    const token = await getTestToken('koala');

    // Open a DM first to get a channel id (the mock uses a stable synthetic ID).
    const openResp = await req.post(`${MOCK_URL}/api/v10/users/@me/channels`, {
      headers: { Authorization: `Bot ${token}` },
      data: { recipient_id: '2' },
    });
    const ch = await openResp.json();

    const resp = await req.put(
      `${MOCK_URL}/api/v10/channels/${ch.id}/recipients/3`,
      {
        headers: { Authorization: `Bot ${token}` },
        data: {},
      },
    );
    // Mock accepts any recipient add and returns 204.
    expect(resp.status()).toBe(204);
  });

  // ── Auth guard ──────────────────────────────────────────────────────────────

  test('POST /users/@me/channels without token returns 401', async ({ request: req }) => {
    const resp = await req.post(`${MOCK_URL}/api/v10/users/@me/channels`, {
      data: { recipient_id: '2' },
    });
    expect(resp.status()).toBe(401);
  });

  test('DELETE /channels/{id} without token returns 401', async ({ request: req }) => {
    const resp = await req.delete(`${MOCK_URL}/api/v10/channels/300`);
    expect(resp.status()).toBe(401);
  });
});
