import {
  app,
  BrowserWindow,
  Menu,
  Tray,
  globalShortcut,
  ipcMain,
  nativeImage,
} from "electron";
import path from "node:path";
import type { Platform, TrayHandle, WindowHandle, WindowKind } from "./platform.js";

export interface ElectronHostBridge {
  openBlankWindow(): void;
  openLauncher(): void;
  closeLauncher(): void;
  openCommandPalette(): void;
  closeCommandPalette(): void;
}

export function createElectronPlatform(options: {
  appPath: string;
  getBridge: () => ElectronHostBridge;
}): Platform {
  const { appPath, getBridge } = options;
  const iconPath = path.join(appPath, "assets", "tray-icon.png");

  function pageFor(kind: WindowKind): string {
    const file =
      kind === "launcher"
        ? "launcher.html"
        : kind === "palette"
          ? "palette.html"
          : "blank.html";
    return path.join(appPath, "ui", file);
  }

  function createWindow(opts: { kind: WindowKind }): WindowHandle {
    const isOverlay = opts.kind === "palette" || opts.kind === "launcher";
    const win = new BrowserWindow({
      width: opts.kind === "palette" ? 560 : opts.kind === "launcher" ? 420 : 800,
      height: opts.kind === "palette" ? 280 : opts.kind === "launcher" ? 240 : 600,
      show: true,
      frame: opts.kind !== "palette",
      transparent: opts.kind === "palette",
      alwaysOnTop: isOverlay,
      webPreferences: {
        preload: path.join(appPath, "dist", "preload.js"),
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: true,
      },
    });
    void win.loadFile(pageFor(opts.kind));

    return {
      id: String(win.id),
      kind: opts.kind,
      close() {
        if (!win.isDestroyed()) win.close();
      },
      isDestroyed() {
        return win.isDestroyed();
      },
    };
  }

  return {
    createTray({ onQuit }) {
      const image = nativeImage.createFromPath(iconPath);
      const tray = new Tray(image.isEmpty() ? nativeImage.createEmpty() : image);
      tray.setToolTip("Spacecraft");
      tray.setContextMenu(
        Menu.buildFromTemplate([
          {
            label: "Open Launcher",
            click: () => getBridge().openLauncher(),
          },
          {
            label: "Command Palette",
            click: () => getBridge().openCommandPalette(),
          },
          { type: "separator" },
          {
            label: "Quit",
            click: () => onQuit(),
          },
        ]),
      );
      const handle: TrayHandle = {
        destroy() {
          tray.destroy();
        },
      };
      return handle;
    },
    createWindow,
    registerShortcut(accelerator, handler) {
      globalShortcut.register(accelerator, handler);
    },
    unregisterAllShortcuts() {
      globalShortcut.unregisterAll();
    },
    quit() {
      app.quit();
    },
  };
}

export function registerHostIpc(getBridge: () => ElectronHostBridge): void {
  ipcMain.handle("host:open-blank-window", () => {
    getBridge().openBlankWindow();
  });
  ipcMain.handle("host:close-launcher", () => {
    getBridge().closeLauncher();
  });
  ipcMain.handle("host:close-command-palette", () => {
    getBridge().closeCommandPalette();
  });
}
