'use strict';

function exposePolyElectronBridge({ contextBridge, ipcRenderer, packageJsonPath }) {
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

  contextBridge.exposeInMainWorld('polyElectron', {
    isElectron: true,
    platform: process.platform,
    version: require(packageJsonPath).version,
    minimize: () => ipcRenderer.send('poly-window-minimize'),
    toggleMaximize: () => ipcRenderer.send('poly-window-toggle-maximize'),
    closeWindow: () => ipcRenderer.send('poly-window-close'),
    windowState: () => ipcRenderer.invoke('poly-window-state'),
    mcpStatus: () => ipcRenderer.invoke('poly-mcp-status'),
    mcpRestart: () => ipcRenderer.invoke('poly-mcp-restart'),
  });
}

module.exports = {
  exposePolyElectronBridge,
};