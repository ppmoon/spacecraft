# 05 — Workspace restores window layout across restarts

**GitHub:** https://github.com/ppmoon/spacecraft/issues/8  
**Parent:** https://github.com/ppmoon/spacecraft/issues/3

**What to build:** The host persists and restores workspace layout (windows, geometry, plugin instance ids). Plugins keep their own business state.

**Blocked by:** 02 — Hello plugin loads from a folder and opens a local UI window (#5)

**Status:** ready-for-agent

- [ ] App close saves layout + instance ids
- [ ] Restore reopens correct plugins/windows with prior geometry
- [ ] Host does not snapshot plugin business state
- [ ] Corrupt workspace fails gracefully
