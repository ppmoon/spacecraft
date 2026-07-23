# Smoke: Phase 1.06 — Window Groups open related windows together

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

2. Open launcher from the tray.

3. Open **Hello** (and optionally a blank window). Under **Window Groups**, name a group, keep candidates checked, click **Create from open windows**.

4. Click **Close** on the group — member windows close; the group remains listed.

5. Click **Open** on the group — members reopen.

6. Optionally use **Open hello+echo as group**, or Ctrl/Cmd+K palette commands `Open group: …` / `Close group: …`.

7. Quit from the tray, start again — Window Groups restore with the Workspace. Ungrouped open/close still works.

## Automated coverage

`npm test` covers create-from-open-windows, open/close group members, Workspace restore of groups, and ungrouped windows remaining independent at the Host seam.
