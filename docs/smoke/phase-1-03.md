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
   - `plugins/echo/` — privileged (`sidecar: echo-sidecar` + Bus permissions/contracts)

2. Build Host (also builds the `echo-sidecar` binary):

   ```bash
   npm install
   npm start
   ```

3. Open launcher → **Echo** → **Open**.

4. In the Echo window:
   - **Emit ping** — out-of-process Sidecar answers via `echo.pong`
   - **Call reflect** — request/response through Host-validated contract

5. Confirm there is no Node plugin-host / Node bus daemon (Rust Host + Rust Sidecar only).

6. Quit from the tray (Sidecar process is killed with the Host).

## Automated coverage

`npm test` covers at the Host / Bus seam:

- one Sidecar process per privileged Plugin (spawn/stop)
- Pub/Sub allow path (echo.ping → echo.pong via Sidecar stdio)
- Request/Response allow path (`echo.reflect`)
- deny paths for undeclared emit/subscribe/call
- contract validation failures
- window commands use a scoped proxy (not a raw global Bus)

## Optional smoke

```bash
npm run smoke   # also opens hello + echo
```
