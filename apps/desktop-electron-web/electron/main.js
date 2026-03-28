'use strict';

/**
 * Electron main process for Poly Desktop (thin web shell).
 *
 * This is the "thin shell" variant — it ALWAYS loads from the dx serve dev
 * server and ALWAYS has CDP enabled.  There is no production mode: no asset
 * server, no dist/ loading.
 *
 * Used by poly-desktop-devtools-mcp (electron mode) and
 * poly-electron-devtools-mcp.  Stays alive across WASM rebuilds — only the
 * page reloads when `dx serve` finishes recompiling.
 */

const { app, BrowserWindow, Menu, ipcMain, shell } = require('electron');
const path = require('node:path');
const {
  attachWindowStateListeners,
  registerWindowControlsIpc,
} = require('./shared/main_process');

// ── Always-on remote debugging ────────────────────────────────────────────────
// CDP port: default 9224 (same as the MCP expects).  Override with env var.
const CDP_PORT = process.env.POLY_ELECTRON_REMOTE_DEBUGGING_PORT || '9224';
app.commandLine.appendSwitch('remote-debugging-port', CDP_PORT);

// ── Security: keep remote content from running Node.js ───────────────────────
const WINDOW_OPTIONS = {
  width: 1280,
  height: 800,
  minWidth: 800,
  minHeight: 600,
  title: 'Poly',
  frame: false,
  autoHideMenuBar: true,
  webPreferences: {
    preload: path.join(__dirname, 'preload.js'),
    contextIsolation: true,
    nodeIntegration: false,
    sandbox: false,
  },
  backgroundColor: '#1a1a1a',
  show: false,
};

async function createWindow() {
  const win = new BrowserWindow(WINDOW_OPTIONS);

  attachWindowStateListeners(win);

  // Always load from the dx serve dev server.
  const devPort = process.env.POLY_DEV_SERVE_PORT || '3001';
  const appUrl = `http://127.0.0.1:${devPort}/`;
  console.log(`[Poly] Thin shell — loading from ${appUrl} (CDP on port ${CDP_PORT})`);

  win.loadURL(appUrl).catch((err) => {
    console.error(`[Poly] Failed to load ${appUrl}: ${err.message}`);
  });

  if (process.env.POLY_DEVTOOLS === '1') {
    win.webContents.openDevTools({ mode: 'detach' });
  }

  win.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url).catch(() => undefined);
    return { action: 'deny' };
  });
}

registerWindowControlsIpc(ipcMain, BrowserWindow);

app.whenReady().then(() => {
  Menu.setApplicationMenu(null);

  void createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});
