#!/usr/bin/env node
// Quick CDP evaluator to check page state

const { createConnection } = require('net');
const PAGE_ID = 'E14D06EC457E99364412F1343B4F28AF';

function makeWsFrame(data) {
  const payload = Buffer.from(data);
  const len = payload.length;
  const header = len < 126 ? Buffer.from([0x81, len]) :
    Object.assign(Buffer.alloc(4), {}, (() => { const b = Buffer.alloc(4); b[0]=0x81; b[1]=126; b.writeUInt16BE(len,2); return b; })());
  return Buffer.concat([header, payload]);
}

function parseWsFrame(buf) {
  if (buf.length < 2) return null;
  const opcode = buf[0] & 0x0f;
  const masked = (buf[1] & 0x80) !== 0;
  let len = buf[1] & 0x7f;
  let headerLen = 2;
  if (len === 126) { len = buf.readUInt16BE(2); headerLen = 4; }
  if (masked) headerLen += 4;
  if (buf.length < headerLen + len) return null;
  let payload = buf.slice(headerLen, headerLen + len);
  if (masked) {
    const mask = buf.slice(headerLen - 4, headerLen);
    for (let i = 0; i < payload.length; i++) payload[i] ^= mask[i % 4];
  }
  return { opcode, payload, totalLen: headerLen + len };
}

const socket = createConnection({ host: '127.0.0.1', port: 9224 });
let ready = false, msgBuf = Buffer.alloc(0), wsBuf = Buffer.alloc(0);

const wsReq = [
  `GET /devtools/page/${PAGE_ID} HTTP/1.1`,
  'Host: 127.0.0.1:9224',
  'Upgrade: websocket',
  'Connection: Upgrade',
  'Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==',
  'Sec-WebSocket-Version: 13',
  '', '',
].join('\r\n');

socket.write(wsReq);

socket.on('data', (chunk) => {
  if (!ready) {
    msgBuf = Buffer.concat([msgBuf, chunk]);
    const str = msgBuf.toString();
    if (str.includes('\r\n\r\n')) {
      ready = true;
      const bodyStart = str.indexOf('\r\n\r\n') + 4;
      wsBuf = msgBuf.slice(Buffer.byteLength(str.substring(0, bodyStart)));
      console.log('[*] Connected! Evaluating document.title...');
      socket.write(makeWsFrame(JSON.stringify({
        id: 1, method: 'Runtime.evaluate',
        params: { expression: 'document.title + " | readyState=" + document.readyState', returnByValue: true }
      })));
    }
    return;
  }
  wsBuf = Buffer.concat([wsBuf, chunk]);
  while (wsBuf.length > 0) {
    const frame = parseWsFrame(wsBuf);
    if (!frame) break;
    wsBuf = wsBuf.slice(frame.totalLen);
    if (frame.opcode === 1) {
      try {
        const msg = JSON.parse(frame.payload.toString());
        if (msg.id === 1) {
          console.log('[+] Result:', JSON.stringify(msg.result || msg.error));
          socket.destroy();
          process.exit(0);
        }
      } catch(e) {}
    }
  }
});

socket.on('error', e => { console.error('[!]', e.message); process.exit(1); });
setTimeout(() => { console.error('[!] Timed out'); socket.destroy(); process.exit(1); }, 10000);
