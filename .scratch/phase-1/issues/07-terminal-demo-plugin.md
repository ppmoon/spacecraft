# 07 — Official terminal demo plugin (xterm.js + node-pty)

**GitHub:** https://github.com/ppmoon/spacecraft/issues/10  
**Parent:** https://github.com/ppmoon/spacecraft/issues/3

**What to build:** Official unloadable terminal demo proving the full path: xterm.js UI + node-pty in plugin host + pty permission + multi-instance. No external terminal apps.

**Blocked by:** 03 — Plugin host process speaks on the permissioned dual-channel bus (#6)

**Status:** ready-for-agent

- [ ] Loads like any other plugin (same manifest/permission model)
- [ ] xterm.js in window; node-pty in plugin-host process
- [ ] Interactive shell session works
- [ ] Multiple instances supported
- [ ] pty permission declared and enforced
- [ ] Works on Win, macOS, current test Linux
- [ ] Removable without breaking the host
- [ ] No external terminal launch feature
