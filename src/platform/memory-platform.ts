import type { Platform, TrayHandle, WindowHandle, WindowKind } from "./platform.js";

export function createMemoryPlatform(): Platform & {
  getWindows(): WindowHandle[];
  getShortcuts(): Map<string, () => void>;
  didQuit(): boolean;
  triggerTrayQuit(): void;
} {
  const windows: WindowHandle[] = [];
  const shortcuts = new Map<string, () => void>();
  let quitCalled = false;
  let nextId = 1;
  let trayQuit: (() => void) | null = null;

  return {
    createTray(options) {
      trayQuit = options.onQuit;
      return {
        destroy() {
          trayQuit = null;
        },
      };
    },
    createWindow(options: { kind: WindowKind }) {
      const id = String(nextId++);
      let destroyed = false;
      const handle: WindowHandle = {
        id,
        kind: options.kind,
        close() {
          destroyed = true;
        },
        isDestroyed() {
          return destroyed;
        },
      };
      windows.push(handle);
      return handle;
    },
    registerShortcut(accelerator, handler) {
      shortcuts.set(accelerator, handler);
    },
    unregisterAllShortcuts() {
      shortcuts.clear();
    },
    quit() {
      quitCalled = true;
    },
    getWindows() {
      return windows.filter((w) => !w.isDestroyed());
    },
    getShortcuts() {
      return shortcuts;
    },
    didQuit() {
      return quitCalled;
    },
    triggerTrayQuit() {
      trayQuit?.();
    },
  };
}
