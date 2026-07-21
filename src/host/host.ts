import type { Platform, TrayHandle, WindowHandle } from "../platform/platform.js";

export interface Host {
  start(): Promise<void>;
  stop(): Promise<void>;
  isTrayVisible(): boolean;
  isRunning(): boolean;
  openLauncher(): void;
  closeLauncher(): void;
  isLauncherOpen(): boolean;
  openCommandPalette(): void;
  closeCommandPalette(): void;
  isCommandPaletteOpen(): boolean;
  openBlankWindow(): void;
  getOpenWindows(): ReadonlyArray<{ id: string; kind: string }>;
}

export function createHost(platform: Platform): Host {
  let tray: TrayHandle | null = null;
  let running = false;
  let launcher: WindowHandle | null = null;
  let palette: WindowHandle | null = null;
  const blankWindows: WindowHandle[] = [];

  function pruneDestroyed() {
    if (launcher?.isDestroyed()) launcher = null;
    if (palette?.isDestroyed()) palette = null;
    for (let i = blankWindows.length - 1; i >= 0; i--) {
      if (blankWindows[i]!.isDestroyed()) blankWindows.splice(i, 1);
    }
  }

  const host: Host = {
    async start() {
      if (running) return;
      tray = platform.createTray({
        onQuit: () => {
          void host.stop();
          platform.quit();
        },
      });
      platform.registerShortcut("CommandOrControl+K", () => {
        host.openCommandPalette();
      });
      running = true;
    },

    async stop() {
      if (!running) return;
      platform.unregisterAllShortcuts();
      pruneDestroyed();
      launcher?.close();
      launcher = null;
      palette?.close();
      palette = null;
      for (const w of blankWindows) w.close();
      blankWindows.length = 0;
      tray?.destroy();
      tray = null;
      running = false;
    },

    isTrayVisible() {
      return tray !== null;
    },

    isRunning() {
      return running;
    },

    openLauncher() {
      pruneDestroyed();
      if (launcher) return;
      launcher = platform.createWindow({ kind: "launcher" });
    },

    closeLauncher() {
      pruneDestroyed();
      launcher?.close();
      launcher = null;
    },

    isLauncherOpen() {
      pruneDestroyed();
      return launcher !== null;
    },

    openCommandPalette() {
      pruneDestroyed();
      if (palette) return;
      palette = platform.createWindow({ kind: "palette" });
    },

    closeCommandPalette() {
      pruneDestroyed();
      palette?.close();
      palette = null;
    },

    isCommandPaletteOpen() {
      pruneDestroyed();
      return palette !== null;
    },

    openBlankWindow() {
      blankWindows.push(platform.createWindow({ kind: "blank" }));
    },

    getOpenWindows() {
      pruneDestroyed();
      const result: { id: string; kind: string }[] = [];
      if (launcher) result.push({ id: launcher.id, kind: launcher.kind });
      if (palette) result.push({ id: palette.id, kind: palette.kind });
      for (const w of blankWindows) result.push({ id: w.id, kind: w.kind });
      return result;
    },
  };

  return host;
}
