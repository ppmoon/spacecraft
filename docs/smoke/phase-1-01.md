# Smoke: Phase 1.01 — Host boots with tray, launcher, and command palette

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

`npm test` exercises the Host seam (boot/tray, launcher open/close, palette shortcut, blank window) against the memory platform. It does not replace the manual Electron smoke above.

## Test Linux notes

On the current cloud/agent Ubuntu image:

- `npm start` boots the Electron main process and tray (dbus warnings are expected without a session bus).
- Opening BrowserWindows may fail if Chromium shared-memory/zygote is restricted; Host seam tests still cover launcher/palette/blank-window behaviour via the injected platform.
- Prefer `npm test` for CI; use a full desktop (or Win/macOS) for interactive window smoke.

Optional auto-quit smoke that opens launcher + palette + blank window:

```bash
npm run smoke
```
