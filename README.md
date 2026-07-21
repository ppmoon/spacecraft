# Spacecraft

Extensible desktop workbench host (**Tauri + Rust**). Phase 1 builds the microkernel shell: tray, launcher, command palette, plugins, and an official terminal demo.

> **Runtime:** Host is Tauri (Rust), not Electron — see [ADR-0001](docs/adr/0001-tauri-host-not-electron.md). Electron Phase 1.01 is reference-only and must not define `main`.

## Requirements

- Rust 1.77+ (edition 2021)
- Node.js 20+ (for `@tauri-apps/cli` only)
- OS: Windows, macOS, or Linux (Phase 1 test environment: Ubuntu with a display / Xvfb)
- Linux packages: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `librsvg2-dev`, `patchelf`, `libayatana-appindicator3-dev`

## Setup

```bash
npm install
```

## Run

```bash
npm start
```

This launches the Tauri Host. The Host stays in the system tray / menu bar (no main window on boot).

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

Host behaviour is covered at the **Host** seam with an in-memory Platform double (`src-tauri` Rust tests). Tauri OS APIs are the injected system boundary.

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

See `docs/smoke/phase-1-01.md` for platform notes.
