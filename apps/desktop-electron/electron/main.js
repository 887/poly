'use strict';

/**
 * Electron main process for Poly Desktop.
 *
 * Loads the Dioxus WASM web build produced by:
 *   dx build --release --platform web
 * (run from the apps/desktop-electron/ directory).
 *
 * The build output lands in apps/desktop-electron/dist/index.html.
 */

const { app, BrowserWindow, Menu, shell } = require('electron');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');

// ── Security: keep remote content from running Node.js ───────────────────────
// contextIsolation and nodeIntegration=false prevent any loaded page from
// accessing Node APIs regardless of what the WASM app does.
const WINDOW_OPTIONS = {
  width: 1280,
  height: 800,
  minWidth: 800,
  minHeight: 600,
  title: 'Poly',
  webPreferences: {
    preload: path.join(__dirname, 'preload.js'),
    contextIsolation: true,
    nodeIntegration: false,
    sandbox: true,
  },
  // Use system default background to avoid white flash on first paint.
  backgroundColor: '#1a1a1a',
  show: false, // defer show until ready-to-show
};

/** @type {http.Server | null} */
let assetServer = null;

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

function resolveWebRoot() {
  const candidates = [
    path.join(__dirname, '..', 'dist'),
    path.join(__dirname, '..', '..', '..', 'target', 'dx', 'poly-desktop-electron', 'debug', 'web', 'public'),
    path.join(__dirname, '..', '..', '..', 'target', 'dx', 'poly-desktop-electron', 'release', 'web', 'public'),
  ];

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

async function createWindow() {
  const win = new BrowserWindow(WINDOW_OPTIONS);

  // Show only after the first paint — avoids white-flash on cold start.
  win.once('ready-to-show', () => win.show());

  // Load the local WASM web-app bundle. Path relative to THIS file (electron/).
  let webRoot;
  try {
    webRoot = resolveWebRoot();
    assetServer = await startAssetServer(webRoot);
  } catch (err) {
    // Friendly message if the WASM build hasn't been run yet.
    console.error(
      `[Poly] Failed to resolve or serve the web bundle: ${err.message}\n` +
      `  Did you run 'dx build --platform web' in apps/desktop-electron/?`,
    );
    return;
  }

  const address = assetServer.address();
  const port = typeof address === 'object' && address ? address.port : 0;
  const appUrl = `http://127.0.0.1:${port}/`;
  win.loadURL(appUrl).catch((err) => {
    console.error(
      `[Poly] Failed to load ${appUrl}: ${err.message}\n` +
      `  Web root: ${webRoot}`,
    );
  });

  // Open DevTools in development (NODE_ENV=development or POLY_DEV=1).
  if (process.env.NODE_ENV === 'development' || process.env.POLY_DEV === '1') {
    win.webContents.openDevTools({ mode: 'detach' });
  }

  // Open external links in the system browser, not inside Electron.
  win.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url).catch(() => undefined);
    return { action: 'deny' };
  });
}

// ── App lifecycle ─────────────────────────────────────────────────────────────

app.whenReady().then(() => {
  // Remove the default menu for a cleaner UI.  Platform-specific menus
  // (e.g. macOS dock behaviour) are handled automatically by Electron.
  Menu.setApplicationMenu(null);

  void createWindow();

  // macOS: re-create window when clicking dock icon with no open windows.
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

// Quit when all windows are closed (except macOS, where the app stays alive).
app.on('window-all-closed', () => {
  if (assetServer) {
    assetServer.close();
    assetServer = null;
  }
  if (process.platform !== 'darwin') {
    app.quit();
  }
});
