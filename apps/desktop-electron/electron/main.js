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

const { app, BrowserWindow, Menu, ipcMain, shell } = require('electron');
const path = require('node:path');
const {
  attachWindowStateListeners,
  registerWindowControlsIpc,
  resolveWebRoot,
  startAssetServer,
} = require('./shared/main_process');

// ── Security: keep remote content from running Node.js ───────────────────────
// contextIsolation and nodeIntegration=false prevent any loaded page from
// accessing Node APIs regardless of what the WASM app does.
const WINDOW_OPTIONS = {
  width: 1280,
  height: 800,
  // Match the thin dev shell so Electron can shrink into the mobile layout.
  minWidth: 320,
  minHeight: 400,
  title: 'Poly',
  frame: false,
  autoHideMenuBar: true,
  webPreferences: {
    preload: path.join(__dirname, 'preload.js'),
    contextIsolation: true,
    nodeIntegration: false,
    sandbox: false,
  },
  // Use system default background to avoid white flash on first paint.
  backgroundColor: '#1a1a1a',
  show: false, // defer show until ready-to-show
};

/** @type {http.Server | null} */
let assetServer = null;

async function createWindow() {
  const win = new BrowserWindow(WINDOW_OPTIONS);

  // Show only after the first paint — avoids white-flash on cold start.
  attachWindowStateListeners(win);

  let appUrl;

  if (process.env.POLY_DEV === '1') {
    // Dev mode: poly-electron-devtools-mcp runs `dx serve --platform web` on
    // this port.  Electron loads directly from the live dev server — no build
    // step needed and the window survives WASM rebuilds (just a Page.reload).
    const devPort = process.env.POLY_DEV_SERVE_PORT || '3001';
    appUrl = `http://127.0.0.1:${devPort}/`;
    console.log(`[Poly] Dev mode — loading from ${appUrl}`);
  } else {
    // Production mode: serve the pre-built WASM bundle from disk.
    let webRoot;
    try {
      webRoot = resolveWebRoot([
        path.join(__dirname, '..', 'dist'),
        path.join(__dirname, '..', '..', '..', 'target', 'dx', 'poly-desktop-electron', 'debug', 'web', 'public'),
        path.join(__dirname, '..', '..', '..', 'target', 'dx', 'poly-desktop-electron', 'release', 'web', 'public'),
      ]);
      assetServer = await startAssetServer(webRoot);
    } catch (err) {
      console.error(
        `[Poly] Failed to resolve or serve the web bundle: ${err.message}\n` +
        `  Did you run 'dx build --platform web' in apps/desktop-electron/?`,
      );
      return;
    }
    const address = assetServer.address();
    const port = typeof address === 'object' && address ? address.port : 0;
    appUrl = `http://127.0.0.1:${port}/`;
  }

  win.loadURL(appUrl).catch((err) => {
    console.error(`[Poly] Failed to load ${appUrl}: ${err.message}`);
  });

  // Open DevTools only when POLY_DEVTOOLS=1 is explicitly set.
  // POLY_DEV=1 (used by VS Code launch tasks for CDP debug port) no longer
  // triggers auto-open — set POLY_DEVTOOLS=1 separately if you want it.
  if (process.env.POLY_DEVTOOLS === '1') {
    win.webContents.openDevTools({ mode: 'detach' });
  }

  // Open external links in the system browser, not inside Electron.
  win.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url).catch(() => undefined);
    return { action: 'deny' };
  });
}

registerWindowControlsIpc(ipcMain, BrowserWindow);

// ── App lifecycle ─────────────────────────────────────────────────────────────

if (process.env.POLY_ELECTRON_REMOTE_DEBUGGING_PORT) {
  app.commandLine.appendSwitch(
    'remote-debugging-port',
    process.env.POLY_ELECTRON_REMOTE_DEBUGGING_PORT,
  );
}

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
