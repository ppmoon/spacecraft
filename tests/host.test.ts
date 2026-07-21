import { describe, expect, test } from "vitest";
import { createHost } from "../src/host/host.js";
import { createMemoryPlatform } from "../src/platform/memory-platform.js";

describe("Host boot and tray", () => {
  test("host boots with tray visible", async () => {
    const platform = createMemoryPlatform();
    const host = createHost(platform);

    await host.start();

    expect(host.isTrayVisible()).toBe(true);
  });

  test("tray quit stops the host", async () => {
    const platform = createMemoryPlatform();
    const host = createHost(platform);
    await host.start();

    platform.triggerTrayQuit();

    expect(host.isRunning()).toBe(false);
    expect(host.isTrayVisible()).toBe(false);
    expect(platform.didQuit()).toBe(true);
  });
});

describe("Launcher", () => {
  test("launcher can be opened and closed", async () => {
    const platform = createMemoryPlatform();
    const host = createHost(platform);
    await host.start();

    host.openLauncher();
    expect(host.isLauncherOpen()).toBe(true);
    expect(host.getOpenWindows().some((w) => w.kind === "launcher")).toBe(true);

    host.closeLauncher();
    expect(host.isLauncherOpen()).toBe(false);
    expect(host.getOpenWindows().some((w) => w.kind === "launcher")).toBe(false);
  });
});

describe("Command palette", () => {
  test("Ctrl/Cmd+K opens the command palette", async () => {
    const platform = createMemoryPlatform();
    const host = createHost(platform);
    await host.start();

    const handler = platform.getShortcuts().get("CommandOrControl+K");
    expect(handler).toBeTypeOf("function");
    handler!();

    expect(host.isCommandPaletteOpen()).toBe(true);
    expect(host.getOpenWindows().some((w) => w.kind === "palette")).toBe(true);
  });

  test("command palette can open a blank OS-level window", async () => {
    const platform = createMemoryPlatform();
    const host = createHost(platform);
    await host.start();

    host.openCommandPalette();
    host.openBlankWindow();

    const blanks = host.getOpenWindows().filter((w) => w.kind === "blank");
    expect(blanks).toHaveLength(1);
  });

  test("command palette can be closed", async () => {
    const platform = createMemoryPlatform();
    const host = createHost(platform);
    await host.start();

    host.openCommandPalette();
    host.closeCommandPalette();

    expect(host.isCommandPaletteOpen()).toBe(false);
  });
});
