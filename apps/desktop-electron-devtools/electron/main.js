'use strict';

/**
 * Electron main process for Poly Desktop — DevTools Build.
 *
 * Loads the Dioxus WASM web bundle from
 * target/dx/poly-desktop-electron/debug/web/public/.
 * Chrome DevTools Protocol (CDP) is enabled on port 9224 so the
 * poly-electron-devtools-mcp server can connect for screenshots, JS eval,
 * DOM inspection, and interaction.
 *
 * This is a DEVELOPER TOOL, not a production build.
 * Security settings are relaxed compared to apps/desktop-electron/electron/main.js
 * to give CDP full access to inspect and control the renderer.
 *
 * DO NOT use this for packaging or distribution.
 */

const { app, BrowserWindow, Menu } = require('electron');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');

// Enable Chrome DevTools Protocol on a fixed port so the MCP can always find it.
// appendSwitch() must be called before app.whenReady() to take effect.
app.commandLine.appendSwitch('remote-debugging-port', '9224');

// Disable GPU sandbox for smoother DevTools integration (developer mode only).
app.commandLine.appendSwitch('no-sandbox');

// Use /tmp instead of /dev/shm for shared memory — required when running
// in Docker, WSL2, or any environment where /dev/shm is inaccessible or
// limited. Without this, Electron's GPU/renderer processes crash with
// "Creating shared memory in /dev/shm/... failed".
app.commandLine.appendSwitch('disable-dev-shm-usage');

// Skip the Chromium zygote process.  The zygote helps renderer startup but
// adds a layer of sandbox complexity that causes ESRCH errors when the
// process-creation environment has restricted user namespaces or custom
// TMPDIR paths.  Without the zygote, renderers are forked directly.
// DEVELOPER TOOL ONLY — never use in production.
app.commandLine.appendSwitch('no-zygote');

const WINDOW_OPTIONS = {
  width: 1440,
  height: 900,
  minWidth: 800,
  minHeight: 600,
  title: 'Poly — Electron DevTools Build',
  webPreferences: {
    // DevTools variant: relax isolation so CDP has full access.
    contextIsolation: false,
    nodeIntegration: false,
    devTools: true,
  },
  backgroundColor: '#1a1a1a',
  // Defer show until content is ready to avoid white flash.
  show: false,
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
    path.join(__dirname, '..', '..', '..', 'target', 'dx', 'poly-desktop-electron', 'debug', 'web', 'public'),
    path.join(__dirname, '..', '..', '..', 'target', 'dx', 'poly-desktop-electron', 'release', 'web', 'public'),
    path.join(__dirname, '..', '..', 'desktop-electron', 'dist'),
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
  win.once('ready-to-show', () => win.show());
  win.on('closed', () => {
    console.error('[Poly Electron DevTools] BrowserWindow closed');
  });
  win.webContents.on('render-process-gone', (_event, details) => {
    console.error(
      `[Poly Electron DevTools] Renderer process gone: reason=${details.reason} exitCode=${details.exitCode}`,
    );
  });
  win.webContents.on('did-fail-load', (_event, errorCode, errorDescription, validatedUrl) => {
    console.error(
      `[Poly Electron DevTools] did-fail-load errorCode=${errorCode} description=${errorDescription} url=${validatedUrl}`,
    );
  });
  win.webContents.on('console-message', (_event, level, message, line, sourceId) => {
    console.error(
      `[Poly Electron DevTools][console:${level}] ${sourceId}:${line} ${message}`,
    );
  });

  // Load the WASM bundle built by:
  //   dx build --platform web   (run in apps/desktop-electron/)
  //
  // Dioxus CLI (dx 0.7+) outputs web builds to:
  //   <workspace-root>/target/dx/<binary-name>/debug/web/public/
  //
  // Directory layout:
  //   apps/
  //     desktop-electron-devtools/
  //       electron/       ← __dirname (this file)
  //   target/
  //     dx/
  //       poly-desktop-electron/
  //         debug/
  //           web/
  //             public/
  //               index.html   ← WASM bundle
  //
  // Relative path from electron/ dir: ../../../target/dx/...
  let webRoot;
  try {
    webRoot = resolveWebRoot();
    assetServer = await startAssetServer(webRoot);
  } catch (err) {
    console.error(
      `[Poly Electron DevTools] Failed to resolve or serve the web bundle: ${err.message}\n` +
      `  Make sure to run 'dx build --platform web' in apps/desktop-electron/ first.\n` +
      `  Expected bundle roots include target/dx/poly-desktop-electron/.../web/public`,
    );
    return;
  }

  const address = assetServer.address();
  const port = typeof address === 'object' && address ? address.port : 0;
  const appUrl = `http://127.0.0.1:${port}/`;

  win.loadURL(appUrl).catch((err) => {
    console.error(
      `[Poly Electron DevTools] Failed to load ${appUrl}: ${err.message}\n` +
      `  Web root: ${webRoot}`,
    );
  });

  // Open DevTools only when explicitly requested (e.g. POLY_DEV_DEVTOOLS=1).
  // Normally we leave it closed so the CDP connection from the MCP server is
  // the sole debugger attached to the page — multiple CDP clients can coexist,
  // but auto-opening DevTools can delay page load.
  if (process.env.POLY_DEV_DEVTOOLS === '1') {
    win.webContents.openDevTools({ mode: 'detach' });
  }

  // External links → system browser (security hygiene even in devtools build).
  win.webContents.setWindowOpenHandler(({ url }) => {
    // Intentionally not calling shell.openExternal to keep devtools self-contained.
    console.warn(`[Poly Electron DevTools] Blocked external navigation to: ${url}`);
    return { action: 'deny' };
  });
}

app.whenReady().then(() => {
  // Remove the default application menu for a cleaner debugging workspace.
  Menu.setApplicationMenu(null);
  void createWindow();

  // macOS: re-create the window when the dock icon is clicked.
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('child-process-gone', (_event, details) => {
  console.error(
    `[Poly Electron DevTools] Child process gone: type=${details.type} reason=${details.reason} name=${details.name ?? 'unknown'} serviceName=${details.serviceName ?? 'unknown'} exitCode=${details.exitCode}`,
  );
});

process.on('uncaughtException', (error) => {
  console.error('[Poly Electron DevTools] uncaughtException', error);
});

process.on('unhandledRejection', (reason) => {
  console.error('[Poly Electron DevTools] unhandledRejection', reason);
});

app.on('window-all-closed', () => {
  if (assetServer) {
    assetServer.close();
    assetServer = null;
  }
  // Quit on all platforms (including macOS) for predictable MCP control.
  app.quit();
});
