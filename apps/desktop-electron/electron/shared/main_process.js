'use strict';

const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');

function sendWindowState(win) {
  if (win.isDestroyed()) {
    return;
  }

  win.webContents.send('poly-window-state', {
    isMaximized: win.isMaximized(),
    isFullScreen: win.isFullScreen(),
  });
}

function attachWindowStateListeners(win, { showOnReady = true } = {}) {
  win.once('ready-to-show', () => {
    if (showOnReady) {
      win.show();
    }
    sendWindowState(win);
  });

  win.on('maximize', () => sendWindowState(win));
  win.on('unmaximize', () => sendWindowState(win));
  win.on('enter-full-screen', () => sendWindowState(win));
  win.on('leave-full-screen', () => sendWindowState(win));
}

function registerWindowControlsIpc(ipcMain, BrowserWindow) {
  ipcMain.on('poly-window-minimize', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.minimize();
  });

  ipcMain.on('poly-window-toggle-maximize', (event) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (!win) {
      return;
    }

    if (win.isMaximized()) {
      win.unmaximize();
    } else {
      win.maximize();
    }
  });

  ipcMain.on('poly-window-close', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.close();
  });

  ipcMain.handle('poly-window-state', (event) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (!win) {
      return { isMaximized: false, isFullScreen: false };
    }

    return {
      isMaximized: win.isMaximized(),
      isFullScreen: win.isFullScreen(),
    };
  });
}

function contentTypeFor(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  switch (ext) {
    case '.html': return 'text/html; charset=utf-8';
    case '.js': return 'text/javascript; charset=utf-8';
    case '.css': return 'text/css; charset=utf-8';
    case '.wasm': return 'application/wasm';
    case '.json': return 'application/json; charset=utf-8';
    case '.svg': return 'image/svg+xml';
    case '.png': return 'image/png';
    case '.jpg':
    case '.jpeg': return 'image/jpeg';
    case '.webp': return 'image/webp';
    case '.ico': return 'image/x-icon';
    default: return 'application/octet-stream';
  }
}

function resolveWebRoot(candidates) {
  for (const candidate of candidates) {
    if (fs.existsSync(path.join(candidate, 'index.html'))) {
      return candidate;
    }
  }

  throw new Error(
    `Could not find a built Poly Electron web bundle. Tried: ${candidates.join(', ')}`,
  );
}

async function startAssetServer(rootDir) {
  return await new Promise((resolve, reject) => {
    const server = http.createServer((req, res) => {
      const requestUrl = new URL(req.url || '/', 'http://127.0.0.1');
      let relativePath = decodeURIComponent(requestUrl.pathname);
      if (relativePath === '/') {
        relativePath = '/index.html';
      }

      const normalizedPath = path.normalize(relativePath).replace(/^([.][.][/\\])+/, '');
      let filePath = path.join(rootDir, normalizedPath);
      if (!filePath.startsWith(rootDir)) {
        res.writeHead(403);
        res.end('Forbidden');
        return;
      }

      if (!fs.existsSync(filePath)) {
        const extension = path.extname(filePath);
        if (!extension) {
          filePath = path.join(rootDir, 'index.html');
        }
      }

      fs.readFile(filePath, (err, data) => {
        if (err) {
          res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
          res.end(`Not found: ${requestUrl.pathname}`);
          return;
        }

        res.writeHead(200, {
          'Content-Type': contentTypeFor(filePath),
          'Cache-Control': 'no-store',
        });
        res.end(data);
      });
    });

    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      resolve(server);
    });
  });
}

module.exports = {
  attachWindowStateListeners,
  registerWindowControlsIpc,
  resolveWebRoot,
  sendWindowState,
  startAssetServer,
};