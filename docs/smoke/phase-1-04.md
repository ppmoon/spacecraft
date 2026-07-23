# Smoke: Phase 1.04 — Install local zip/folder Plugin with permission confirmation

## Platforms

| Platform | Status |
| --- | --- |
| Windows | Manual |
| macOS | Manual |
| Linux (current test env: Ubuntu + display) | Manual / agent |

## Steps

1. Fixture package (not pre-installed): `fixtures/packages/notes/`

2. Start:

   ```bash
   npm install
   npm start
   ```

3. Open launcher from the tray.

4. Under **Install Plugin**, enter the absolute path to `fixtures/packages/notes` (or a zip of that folder).

5. Click **Review permissions**. Confirm the list shows Bus permissions and that the signature field note appears.

6. Click **Decline** once — launcher Plugin list must stay unchanged.

7. Review again, then **Confirm install**. **Notes** appears under Plugins and can be opened.

8. Quit from the tray.

## Automated coverage

`npm test` covers folder install, zip install, decline-no-change, and reserved signature acceptance at the Host seam.

## Optional zip

```bash
# from repo root
cd fixtures/packages && zip -r /tmp/notes.zip notes
# paste /tmp/notes.zip into the launcher install path
```
