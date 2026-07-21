# 02 — Hello plugin loads from a folder and opens a local UI window

**GitHub:** https://github.com/ppmoon/spacecraft/issues/5  
**Parent:** https://github.com/ppmoon/spacecraft/issues/3

**What to build:** A developer drops a hello-world plugin folder (declarative manifest + local UI entry) into the plugins directory; the host lists it in the launcher and opens an OS window that loads the plugin’s local UI over a custom protocol, with Node integration disabled.

**Blocked by:** 01 — Host boots with tray, launcher, and command palette (#4)

**Status:** ready-for-agent

- [ ] Minimal manifest schema is validated (id, name, version, ui entry, window type)
- [ ] Host scans a plugins directory and shows the hello plugin in the launcher
- [ ] Opening the plugin creates an OS window rendering its local UI (custom protocol)
- [ ] Window page cannot access Node APIs directly (no nodeIntegration)
- [ ] Fixture hello plugin is included for manual and automated checks
