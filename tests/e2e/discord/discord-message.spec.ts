/**
 * discord-message.spec.ts
 *
 * Verifies that the Discord message send/receive endpoints work against
 * `poly-test-discord`, including the gateway WebSocket MESSAGE_CREATE event.
 *
 * Environment:
 *   DISCORD_MOCK_URL  Base URL of poly-test-discord (default: http://localhost:9200)
 */

import { test, expect, request } from '@playwright/test';
import { WebSocket } from 'ws';

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

/** Wait for a single WebSocket message matching predicate. Resolves with the parsed JSON. */
function waitForWsMessage(
  ws: WebSocket,
  predicate: (msg: unknown) => boolean,
  timeoutMs = 5000,
): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('WebSocket message timeout')), timeoutMs);
    const handler = (raw: Buffer | string) => {
      try {
        const parsed = JSON.parse(raw.toString());
        if (predicate(parsed)) {
          clearTimeout(timer);
          ws.off('message', handler);
          resolve(parsed);
        }
      } catch {
        // not JSON, ignore
      }
    };
    ws.on('message', handler);
  });
}

// ---------------------------------------------------------------------------
// Test suite
// ---------------------------------------------------------------------------

test.describe('discord-message', () => {
  test.beforeEach(async ({ request: req }) => {
    await req.post(`${MOCK_URL}/reseed`);
  });

  test('GET /channels/{id}/messages returns existing messages', async ({ request: req }) => {
    const token = await getTestToken('koala');

    const resp = await req.get(`${MOCK_URL}/api/v10/channels/200/messages`, {
      headers: { Authorization: `Bot ${token}` },
    });
    expect(resp.status()).toBe(200);

    const messages = await resp.json();
    expect(Array.isArray(messages)).toBe(true);
    expect(messages.length).toBeGreaterThanOrEqual(3);

    const msg = messages[0];
    expect(typeof msg.id).toBe('string');
    expect(typeof msg.content).toBe('string');
    expect(msg.channel_id).toBe('200');
    expect(msg.author).toBeTruthy();
    expect(typeof msg.author.username).toBe('string');
  });

  test('GET /channels/{id}/messages respects limit param', async ({ request: req }) => {
    const token = await getTestToken('koala');

    const resp = await req.get(`${MOCK_URL}/api/v10/channels/200/messages?limit=1`, {
      headers: { Authorization: `Bot ${token}` },
    });
    expect(resp.status()).toBe(200);

    const messages = await resp.json();
    expect(messages.length).toBe(1);
  });

  test('POST /channels/{id}/messages sends a message and returns it', async ({ request: req }) => {
    const token = await getTestToken('koala');

    const content = `test message ${Date.now()}`;
    const resp = await req.post(`${MOCK_URL}/api/v10/channels/200/messages`, {
      headers: { Authorization: `Bot ${token}` },
      data: { content },
    });
    expect(resp.status()).toBe(200);

    const msg = await resp.json();
    expect(msg.content).toBe(content);
    expect(msg.channel_id).toBe('200');
    expect(msg.author.username).toBe('koala');
  });

  test('sent message is returned by subsequent GET', async ({ request: req }) => {
    const token = await getTestToken('koala');

    const content = `persistence check ${Date.now()}`;
    await req.post(`${MOCK_URL}/api/v10/channels/200/messages`, {
      headers: { Authorization: `Bot ${token}` },
      data: { content },
    });

    const resp = await req.get(`${MOCK_URL}/api/v10/channels/200/messages`, {
      headers: { Authorization: `Bot ${token}` },
    });
    const messages = await resp.json();
    const found = messages.some((m: { content: string }) => m.content === content);
    expect(found).toBe(true);
  });

  test('GET /channels/{id}/messages returns 401 without token', async ({ request: req }) => {
    const resp = await req.get(`${MOCK_URL}/api/v10/channels/200/messages`);
    expect(resp.status()).toBe(401);
  });

  test('POST /channels/{id}/messages returns 401 without token', async ({ request: req }) => {
    const resp = await req.post(`${MOCK_URL}/api/v10/channels/200/messages`, {
      data: { content: 'unauthorized' },
    });
    expect(resp.status()).toBe(401);
  });

  // ---------------------------------------------------------------------------
  // Gateway WebSocket — MESSAGE_CREATE event
  // ---------------------------------------------------------------------------

  test('sending a message emits a MESSAGE_CREATE gateway event', async ({ request: req }) => {
    const token = await getTestToken('koala');

    // Resolve the gateway WebSocket URL from the server.
    const gwResp = await req.get(`${MOCK_URL}/api/v10/gateway`, {
      headers: { Authorization: `Bot ${token}` },
    });
    expect(gwResp.status()).toBe(200);
    const gwBody = await gwResp.json();
    // The gateway URL from the test server points to /gateway/ws.
    // If the response URL is a placeholder, build one from MOCK_URL.
    const wsUrl =
      (gwBody.url as string).startsWith('ws')
        ? gwBody.url
        : MOCK_URL.replace(/^http/, 'ws') + '/gateway/ws';

    const ws = new WebSocket(wsUrl);

    // Wait for READY event (first message from server)
    await waitForWsMessage(
      ws,
      (m: unknown) => {
        const msg = m as { t?: string; op?: number };
        return msg.t === 'READY' || msg.op === 0;
      },
      5000,
    );

    // Now send a message via REST; expect a MESSAGE_CREATE gateway event.
    const content = `gateway test ${Date.now()}`;
    const messageCreate = waitForWsMessage(
      ws,
      (m: unknown) => {
        const msg = m as { t?: string; d?: { content?: string } };
        return msg.t === 'MESSAGE_CREATE' && msg.d?.content === content;
      },
      5000,
    );

    await req.post(`${MOCK_URL}/api/v10/channels/200/messages`, {
      headers: { Authorization: `Bot ${token}` },
      data: { content },
    });

    const event = await messageCreate;
    expect(event).toBeTruthy();

    ws.close();
  });
});
