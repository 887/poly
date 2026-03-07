'use strict';

/**
 * Electron preload script for Poly Desktop.
 *
 * Runs in the renderer process's isolated world (contextIsolation: true).
 * The main Poly app is pure WASM and does not require any Node.js bridge.
 * This file exists as a hook for future native integrations (e.g. system
 * notifications, tray updates, file-system access via IPC).
 *
 * Security constraints:
 * - contextBridge.exposeInMainWorld() must be used to expose any API.
 * - Never expose raw Node.js or Electron APIs to the renderer.
 * - All IPC channels must be explicitly allowlisted.
 */

const { contextBridge, ipcRenderer } = require('electron');

function dispatchWindowState(state) {
  window.dispatchEvent(
    new CustomEvent('poly-window-state', {
      detail: state,
    }),
  );
}

ipcRenderer.on('poly-window-state', (_event, state) => {
  dispatchWindowState(state);
});

// Expose a minimal, safe API surface to the WASM renderer.
// Currently empty — no Node.js bridge is required by the WASM app.
// Add entries here if and when native integrations are needed.
contextBridge.exposeInMainWorld('polyElectron', {
  isElectron: true,

  // Platform identifier — allows the WASM app to detect Electron vs browser.
  platform: process.platform,

  // Poly version exposed from package.json for telemetry / about screen.
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  version: require('./package.json').version,

  minimize: () => ipcRenderer.send('poly-window-minimize'),
  toggleMaximize: () => ipcRenderer.send('poly-window-toggle-maximize'),
  closeWindow: () => ipcRenderer.send('poly-window-close'),
  windowState: () => ipcRenderer.invoke('poly-window-state'),

  // Example: future notification bridge
  // notify: (title, body) => ipcRenderer.send('notify', { title, body }),
});
