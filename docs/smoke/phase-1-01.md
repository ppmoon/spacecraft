# Smoke: Phase 1.01 — Host boots with tray, launcher, and command palette (Tauri)

## Platforms

| Platform | Status |
| --- | --- |
| Windows | Manual |
| macOS | Manual |
| Linux (current test env: Ubuntu + display) | Manual / agent |

## Steps

1. Install and start:

   ```bash
   npm install
   npm start
   ```

2. **Tray**: icon visible; menu includes Open Launcher, Command Palette, Quit.

3. **Launcher**: open from tray; UI appears; Close dismisses it.

4. **Command palette**: `Ctrl/Cmd+K` opens palette; **Open blank window** creates an OS-level window titled/labelled as blank.

5. **Quit**: tray Quit exits the process cleanly.

## Automated coverage

`npm test` exercises the Host seam (boot/tray, launcher open/close, palette shortcut, blank window) against the memory Platform. It does not replace the manual Tauri smoke above.

## Test Linux notes

On the current cloud/agent Ubuntu image:

- `npm start` boots the Tauri Host and tray (display / D-Bus warnings are expected without a full desktop session).
- Opening webviews may fail if the environment lacks a usable display; Host seam tests still cover launcher/palette/blank-window behaviour via the injected Platform.
- Prefer `npm test` for CI; use a full desktop (or Win/macOS) for interactive window smoke.

Optional auto-quit smoke that opens launcher + palette + blank window:

```bash
npm run smoke
```
