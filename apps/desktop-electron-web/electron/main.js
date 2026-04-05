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
const { spawn } = require('node:child_process');

// ── MCP sidecar configuration ─────────────────────────────────────────────────
const MCP_PORT = parseInt(process.env.POLY_MCP_PORT || '3010', 10);
const MCP_ENABLED = process.env.POLY_MCP_ENABLED !== '0'; // default: enabled
// Path to poly-chat-mcp binary relative to workspace root
const MCP_BIN = process.env.POLY_CHAT_MCP_BIN
  || path.join(__dirname, '..', '..', '..', '..', 'target', 'debug', 'poly-chat-mcp');

let mcpProcess = null;

function startMcpSidecar() {
  if (!MCP_ENABLED) return;
  if (mcpProcess) return; // already running

  const binPath = MCP_BIN;
  console.log(`[Poly MCP] Spawning ${binPath} --port ${MCP_PORT}`);

  mcpProcess = spawn(binPath, ['--port', String(MCP_PORT)], {
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env },
  });

  mcpProcess.stdout.on('data', (d) => process.stdout.write(`[MCP] ${d}`));
  mcpProcess.stderr.on('data', (d) => process.stderr.write(`[MCP] ${d}`));
  mcpProcess.on('exit', (code) => {
    console.log(`[Poly MCP] sidecar exited with code ${code}`);
    mcpProcess = null;
  });
  mcpProcess.on('error', (err) => {
    // Binary not found is expected in CI / web-only runs
    console.warn(`[Poly MCP] failed to start: ${err.message}`);
    mcpProcess = null;
  });
}

function stopMcpSidecar() {
  if (mcpProcess) {
    mcpProcess.kill('SIGTERM');
    mcpProcess = null;
  }
}

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

// MCP status IPC
ipcMain.handle('poly-mcp-status', () => ({
  enabled: MCP_ENABLED,
  port: MCP_PORT,
  running: mcpProcess !== null && !mcpProcess.killed,
}));

ipcMain.handle('poly-mcp-restart', () => {
  stopMcpSidecar();
  startMcpSidecar();
  return { port: MCP_PORT };
});

app.whenReady().then(() => {
  Menu.setApplicationMenu(null);

  void createWindow();
  startMcpSidecar();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('will-quit', stopMcpSidecar);

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});
