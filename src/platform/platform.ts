/**
 * OS / Electron boundary injected into Host.
 * Production wires Electron; tests use an in-memory fake.
 */
export type WindowKind = "blank" | "launcher" | "palette";

export interface WindowHandle {
  readonly id: string;
  readonly kind: WindowKind;
  close(): void;
  isDestroyed(): boolean;
}

export interface TrayHandle {
  destroy(): void;
}

export interface Platform {
  createTray(options: { onQuit: () => void }): TrayHandle;
  createWindow(options: { kind: WindowKind }): WindowHandle;
  registerShortcut(accelerator: string, handler: () => void): void;
  unregisterAllShortcuts(): void;
  quit(): void;
}
