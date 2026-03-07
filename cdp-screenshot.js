#!/usr/bin/env node
// Simple CDP screenshot via Node.js built-in modules
// (No external npm packages required)

const { createConnection } = require('net');
const { createHash } = require('crypto');
const fs = require('fs');
const http = require('http');

const PAGE_ID = 'E14D06EC457E99364412F1343B4F28AF';
const WS_URL = `ws://127.0.0.1:9224/devtools/page/${PAGE_ID}`;
const OUT_PATH = '/home/laragana/workspcacemsg/devtools-screenshots/electron-direct.png';

function makeWsHandshake(host, path) {
  const key = Buffer.from(Math.random().toString()).toString('base64');
  return {
    request: [
      `GET ${path} HTTP/1.1`,
      `Host: ${host}`,
      'Upgrade: websocket',
      'Connection: Upgrade',
      `Sec-WebSocket-Key: ${key}`,
      'Sec-WebSocket-Version: 13',
      '',
      '',
    ].join('\r\n'),
    key,
  };
}

function parseWsFrame(buf) {
  if (buf.length < 2) return null;
  const isFinal = (buf[0] & 0x80) !== 0;
  const opcode = buf[0] & 0x0f;
  const masked = (buf[1] & 0x80) !== 0;
  let len = buf[1] & 0x7f;
  let headerLen = 2;
  if (len === 126) { len = buf.readUInt16BE(2); headerLen = 4; }
  else if (len === 127) { len = Number(buf.readBigUInt64BE(2)); headerLen = 10; }
  if (masked) headerLen += 4;
  if (buf.length < headerLen + len) return null;
  let payload = buf.slice(headerLen, headerLen + len);
  if (masked) {
    const mask = buf.slice(headerLen - 4, headerLen);
    for (let i = 0; i < payload.length; i++) {
      payload[i] ^= mask[i % 4];
    }
  }
  return { opcode, payload, totalLen: headerLen + len };
}

function makeWsFrame(data) {
  const payload = Buffer.from(data);
  const len = payload.length;
  let header;
  if (len < 126) {
    header = Buffer.alloc(2);
    header[0] = 0x81; // FIN + text opcode
    header[1] = len;
  } else if (len < 65536) {
    header = Buffer.alloc(4);
    header[0] = 0x81;
    header[1] = 126;
    header.writeUInt16BE(len, 2);
  } else {
    header = Buffer.alloc(10);
    header[0] = 0x81;
    header[1] = 127;
    header.writeBigUInt64BE(BigInt(len), 2);
  }
  return Buffer.concat([header, payload]);
}

const socket = createConnection({ host: '127.0.0.1', port: 9224 });
let handshakeDone = false;
let msgBuf = Buffer.alloc(0);
let wsBuf = Buffer.alloc(0);

const { request: wsReq } = makeWsHandshake('127.0.0.1:9224', `/devtools/page/${PAGE_ID}`);
socket.write(wsReq);

socket.on('data', (chunk) => {
  if (!handshakeDone) {
    msgBuf = Buffer.concat([msgBuf, chunk]);
    const str = msgBuf.toString();
    if (str.includes('\r\n\r\n')) {
      console.log('[*] WebSocket handshake complete');
      handshakeDone = true;
      const bodyStart = str.indexOf('\r\n\r\n') + 4;
      wsBuf = msgBuf.slice(Buffer.from(str.substring(0, bodyStart)).length);
      
      // Send screenshot command
      const cmd = JSON.stringify({ id: 1, method: 'Page.captureScreenshot', params: { format: 'png' } });
      console.log('[*] Sending Page.captureScreenshot...');
      socket.write(makeWsFrame(cmd));
    }
    return;
  }
  
  wsBuf = Buffer.concat([wsBuf, chunk]);
  
  // Parse WebSocket frames
  while (wsBuf.length > 0) {
    const frame = parseWsFrame(wsBuf);
    if (!frame) break;
    wsBuf = wsBuf.slice(frame.totalLen);
    
    if (frame.opcode === 1) { // text
      try {
        const msg = JSON.parse(frame.payload.toString());
        if (msg.id === 1) {
          if (msg.error) {
            console.error('[!] CDP error:', JSON.stringify(msg.error));
            socket.destroy();
            process.exit(1);
          }
          const imgData = msg.result && msg.result.data;
          if (imgData) {
            const bytes = Buffer.from(imgData, 'base64');
            fs.writeFileSync(OUT_PATH, bytes);
            console.log(`[+] Screenshot saved! ${bytes.length.toLocaleString()} bytes -> ${OUT_PATH}`);
            socket.destroy();
            process.exit(0);
          } else {
            console.error('[!] No data in screenshot response:', JSON.stringify(msg));
            socket.destroy();
            process.exit(1);
          }
        }
      } catch (e) {
        // Skip non-JSON or events
      }
    }
  }
});

socket.on('error', (err) => {
  console.error('[!] Socket error:', err.message);
  process.exit(1);
});

// Timeout after 30 seconds
setTimeout(() => {
  console.error('[!] Screenshot timed out after 30s');
  socket.destroy();
  process.exit(1);
}, 30000);
