# Spacecraft

Extensible desktop workbench host (Electron + TypeScript). Phase 1 builds the microkernel shell: tray, launcher, command palette, plugins, and an official terminal demo.

## Requirements

- Node.js 20+
- npm
- OS: Windows, macOS, or Linux (Phase 1 test environment: Ubuntu with a display / Xvfb)

## Setup

```bash
npm install
```

## Run

```bash
npm start
```

This builds TypeScript to `dist/` and launches Electron. The app stays in the system tray / menu bar.

### Host actions

| Action | How |
| --- | --- |
| Open launcher | Tray menu → **Open Launcher** |
| Open command palette | `Ctrl/Cmd+K` or tray menu → **Command Palette** |
| Open blank window | Command palette → **Open blank window** (or launcher button) |
| Quit | Tray menu → **Quit** |

## Test

```bash
npm test
```

Host behaviour is covered at the **Host** seam with an in-memory platform double (see `tests/host.test.ts`). Electron OS APIs are the injected system boundary.

## Smoke check (Phase 1 platforms)

Manual smoke on Windows, macOS, and the current test Linux:

1. `npm install && npm start`
2. Confirm tray / menu-bar icon appears
3. Open launcher from the tray; close it
4. Press `Ctrl+K` / `Cmd+K`; choose **Open blank window**
5. Quit from the tray

```bash
npm run smoke   # opens launcher + palette + blank window, then quits
```

See `docs/smoke/phase-1-01.md` for platform notes (including Chromium shm limits on some Linux agent images).
