import { contextBridge, ipcRenderer } from "electron";

contextBridge.exposeInMainWorld("spacecraft", {
  openBlankWindow: () => ipcRenderer.invoke("host:open-blank-window"),
  closeLauncher: () => ipcRenderer.invoke("host:close-launcher"),
  closeCommandPalette: () => ipcRenderer.invoke("host:close-command-palette"),
});
