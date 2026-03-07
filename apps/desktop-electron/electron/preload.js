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

const path = require('node:path');
const { contextBridge, ipcRenderer } = require('electron');
const { exposePolyElectronBridge } = require('./shared/preload_bridge');

exposePolyElectronBridge({
  contextBridge,
  ipcRenderer,
  packageJsonPath: path.join(__dirname, 'package.json'),
});
