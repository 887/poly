'use strict';

/**
 * Electron-side implementation of the Poly host bridge.
 *
 * Mirrors the Rust dispatcher in `crates/host-bridge/src/lib.rs`. Native
 * shells (Wry, Electron, future iOS / Android) all expose the SAME JSON
 * protocol so WASM-side clients (e.g. poly-host-bridge::Client) work
 * regardless of which shell they happen to be running inside.
 *
 * Wire format:
 *   POST /host  →  {"call": "exec-command", "program": "...", "args": [...]}
 *   POST /host  →  {"call": "http-request", "method": "...", "url": "...", "headers": [...], "body_b64": "..."}
 *
 * Response:
 *   {"ok":  {"kind": "exec-output",  "exit_code": ..., "stdout_b64": "...", "stderr_b64": "..."}}
 *   {"ok":  {"kind": "http-response","status": ..., "headers": [...], "body_b64": "..."}}
 *   {"err": "<message>"}
 *
 * Port + path are kept in sync with poly_host_bridge::{BRIDGE_PORT, BRIDGE_PATH}.
 */

const http = require('node:http');
const { spawn } = require('node:child_process');

const BRIDGE_PORT = 9333;
const BRIDGE_PATH = '/host';

function ok(payload) {
  return JSON.stringify({ ok: payload });
}

function err(message) {
  return JSON.stringify({ err: String(message) });
}

function execCommand({ program, args }) {
  return new Promise((resolve) => {
    if (typeof program !== 'string' || !Array.isArray(args)) {
      resolve(err('exec-command: program must be string and args must be array'));
      return;
    }
    const child = spawn(program, args.map(String), {
      stdio: ['ignore', 'pipe', 'pipe'],
      shell: false,
    });
    const stdoutChunks = [];
    const stderrChunks = [];
    child.stdout.on('data', (d) => stdoutChunks.push(d));
    child.stderr.on('data', (d) => stderrChunks.push(d));
    child.on('error', (e) => {
      resolve(err(`failed to spawn \`${program}\`: ${e.message}`));
    });
    child.on('close', (code) => {
      const stdout = Buffer.concat(stdoutChunks);
      const stderr = Buffer.concat(stderrChunks);
      resolve(
        ok({
          kind: 'exec-output',
          exit_code: code === null ? -1 : code,
          stdout_b64: stdout.toString('base64'),
          stderr_b64: stderr.toString('base64'),
        }),
      );
    });
  });
}

async function httpRequest({ method, url, headers, body_b64 }) {
  if (typeof method !== 'string' || typeof url !== 'string') {
    return err('http-request: method and url must be strings');
  }
  const reqHeaders = {};
  if (Array.isArray(headers)) {
    for (const pair of headers) {
      if (Array.isArray(pair) && pair.length === 2) {
        reqHeaders[String(pair[0])] = String(pair[1]);
      }
    }
  }
  let body;
  if (body_b64) {
    try {
      body = Buffer.from(String(body_b64), 'base64');
    } catch (e) {
      return err(`invalid body_b64: ${e.message}`);
    }
  }
  try {
    const resp = await fetch(url, { method, headers: reqHeaders, body });
    const respHeaders = [];
    for (const [k, v] of resp.headers.entries()) {
      respHeaders.push([k, v]);
    }
    const arrayBuf = await resp.arrayBuffer();
    return ok({
      kind: 'http-response',
      status: resp.status,
      headers: respHeaders,
      body_b64: Buffer.from(arrayBuf).toString('base64'),
    });
  } catch (e) {
    return err(`http request failed: ${e.message}`);
  }
}

async function dispatch(call) {
  if (!call || typeof call !== 'object') {
    return err('host call payload must be an object');
  }
  switch (call.call) {
    case 'exec-command':
      return execCommand(call);
    case 'http-request':
      return httpRequest(call);
    default:
      return err(`unknown host call: ${call.call}`);
  }
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    req.on('error', reject);
  });
}

// CORS headers — the WASM renderer is loaded from a different origin
// (http://127.0.0.1:3001/) than the bridge (http://127.0.0.1:9333/), so any
// fetch from the renderer will trigger a preflight. Mirror the Wry shell's
// permissive `tower_http::cors::CorsLayer::any()` so the same renderer code
// works in either container.
const CORS_HEADERS = {
  'access-control-allow-origin': '*',
  'access-control-allow-methods': '*',
  'access-control-allow-headers': '*',
  'access-control-max-age': '86400',
};

function writeWithCors(res, status, contentType, body) {
  res.writeHead(status, { 'content-type': contentType, ...CORS_HEADERS });
  res.end(body);
}

function start() {
  const server = http.createServer(async (req, res) => {
    if (req.method === 'OPTIONS') {
      res.writeHead(204, CORS_HEADERS);
      res.end();
      return;
    }
    if (req.method !== 'POST' || req.url !== BRIDGE_PATH) {
      writeWithCors(res, 404, 'text/plain', 'not found');
      return;
    }
    let payload;
    try {
      const raw = await readBody(req);
      payload = JSON.parse(raw);
    } catch (e) {
      writeWithCors(res, 400, 'application/json', err(`invalid host-call JSON: ${e.message}`));
      return;
    }
    const body = await dispatch(payload);
    writeWithCors(res, 200, 'application/json', body);
  });

  server.on('error', (e) => {
    console.error(`[Poly host-bridge] failed to bind 127.0.0.1:${BRIDGE_PORT}: ${e.message}`);
  });

  server.listen(BRIDGE_PORT, '127.0.0.1', () => {
    console.log(`[Poly host-bridge] listening on http://127.0.0.1:${BRIDGE_PORT}${BRIDGE_PATH}`);
  });

  return server;
}

module.exports = { start, BRIDGE_PORT, BRIDGE_PATH };
