'use strict';

/**
 * Electron preload script for the desktop-electron-devtools shell.
 *
 * This intentionally mirrors the production desktop-electron preload bridge so
 * the custom frameless title bar renders in the devtools variant as well.
 */

const path = require('node:path');
const { contextBridge, ipcRenderer } = require('electron');
const { exposePolyElectronBridge } = require('../../desktop-electron/electron/shared/preload_bridge');

exposePolyElectronBridge({
  contextBridge,
  ipcRenderer,
  packageJsonPath: path.join(__dirname, 'package.json'),
});