# Smoke: Phase 1.03 — Privileged Sidecar + permissioned dual-channel Bus

## Platforms

| Platform | Status |
| --- | --- |
| Windows | Manual |
| macOS | Manual |
| Linux (current test env: Ubuntu + display) | Manual / agent |

## Steps

1. Fixtures present:
   - `plugins/hello/` — Pure-UI
   - `plugins/echo/` — privileged (Sidecar marker + Bus permissions/contracts)

2. Start:

   ```bash
   npm install
   npm start
   ```

3. Open launcher → **Echo** → **Open**.

4. In the Echo window:
   - **Emit ping** — Sidecar should answer via `echo.pong` (check log / `bus://event`)
   - **Call reflect** — request/response round-trip with Host-validated contract

5. Confirm there is no Node plugin-host / Node bus daemon (Rust Host only).

6. Quit from the tray (Sidecar lifecycle stops with the Host).

## Automated coverage

`npm test` covers at the Host / Bus seam:

- one Sidecar per privileged Plugin (spawn/stop)
- Pub/Sub allow path (echo.ping → echo.pong)
- Request/Response allow path (`echo.reflect`)
- deny paths for undeclared emit/subscribe/call
- contract validation failures
- window commands use a scoped proxy (not a raw global Bus)

## Optional smoke

```bash
npm run smoke   # also opens hello + echo
```
