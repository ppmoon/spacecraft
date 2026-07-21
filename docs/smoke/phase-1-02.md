# Smoke: Phase 1.02 — Hello Pure-UI Plugin loads from a folder

## Platforms

| Platform | Status |
| --- | --- |
| Windows | Manual |
| macOS | Manual |
| Linux (current test env: Ubuntu + display) | Manual / agent |

## Steps

1. Ensure the fixture Plugin is present at `plugins/hello/` (`manifest.json` + `index.html`).

2. Install and start:

   ```bash
   npm install
   npm start
   ```

   Optional: `SPACECRAFT_PLUGINS_DIR=/path/to/plugins npm start`

3. Open the launcher from the tray. Confirm **Hello** appears under Plugins.

4. Click **Open** on Hello. An OS window titled **hello** (or Hello) loads the local Pure-UI page.

5. Confirm the Plugin window is a Pure-UI surface: it does not receive Host privileged commands (capability `win-*` only). There is no Sidecar.

6. Quit from the tray.

## Automated coverage

`npm test` covers Manifest validation (invalid packages skipped), scanning the plugins directory, opening the hello Pure-UI window, and asserting the window denies privileged APIs — all at the Host seam with `MemoryPlatform`.

## Optional smoke

```bash
npm run smoke   # also opens the hello Plugin window before quit
```
