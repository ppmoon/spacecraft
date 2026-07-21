import { app } from "electron";
import { createHost, type Host } from "./host/host.js";
import {
  createElectronPlatform,
  registerHostIpc,
  type ElectronHostBridge,
} from "./platform/electron-platform.js";

let host: Host | null = null;

function bridge(): ElectronHostBridge {
  if (!host) {
    throw new Error("Host is not started");
  }
  return {
    openBlankWindow: () => host!.openBlankWindow(),
    openLauncher: () => host!.openLauncher(),
    closeLauncher: () => host!.closeLauncher(),
    openCommandPalette: () => host!.openCommandPalette(),
    closeCommandPalette: () => host!.closeCommandPalette(),
  };
}

async function main(): Promise<void> {
  // Linux containers often need these Chromium flags.
  if (process.platform === "linux") {
    app.commandLine.appendSwitch("no-sandbox");
    app.commandLine.appendSwitch("disable-dev-shm-usage");
  }

  await app.whenReady();

  const platform = createElectronPlatform({
    appPath: app.getAppPath(),
    getBridge: bridge,
  });

  host = createHost(platform);
  registerHostIpc(bridge);
  await host.start();

  if (process.env.SPACECRAFT_SMOKE === "1") {
    host.openLauncher();
    host.openCommandPalette();
    host.openBlankWindow();
    setTimeout(() => {
      void host?.stop();
      app.quit();
    }, 1500);
  }

  // Stay alive in the tray when the last window closes.
  app.on("window-all-closed", () => {});

  app.on("before-quit", () => {
    void host?.stop();
  });
}

void main();
