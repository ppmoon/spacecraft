# Smoke: Phase 1.05 — Workspace restores window layout across restarts

## Platforms

| Platform | Status |
| --- | --- |
| Windows | Manual |
| macOS | Manual |
| Linux (current test env: Ubuntu + display) | Manual / agent |

## Steps

1. Start:

   ```bash
   npm install
   npm start
   ```

2. Open launcher → open **Hello** (and optionally a blank window from the command palette).

3. Move/resize the Plugin window to a memorable position.

4. Quit from the tray (this persists Workspace to `plugins/.spacecraft-workspace.json`).

5. Start again with `npm start`.

6. Confirm Hello (and any blank windows) reopen with the prior size/position and the same Plugin instance identity. Plugin UI business state is **not** Host-restored (Plugins own that separately).

7. Optionally corrupt `plugins/.spacecraft-workspace.json` (write garbage), restart — Host must still boot; tray/launcher remain usable.

## Automated coverage

`npm test` covers save-on-stop, restore geometry + instance ids, no business-state fields in the snapshot, and corrupt-file soft-fail at the Host seam (`MemoryPlatform`).
