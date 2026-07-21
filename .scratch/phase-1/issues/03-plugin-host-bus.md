# 03 — Plugin host process speaks on the permissioned dual-channel bus

**GitHub:** https://github.com/ppmoon/spacecraft/issues/6  
**Parent:** https://github.com/ppmoon/spacecraft/issues/3

**What to build:** Opening a plugin spawns a dedicated plugin-host process. Window UI talks only through a host proxy. Plugins use main-routed Pub/Sub + Request/Response with namespaced topics, Zod validation, and manifest permissions; undeclared traffic fails closed.

**Blocked by:** 02 — Hello plugin loads from a folder and opens a local UI window (#5)

**Status:** ready-for-agent

- [ ] One plugin ⇒ one plugin-host process lifecycle
- [ ] Pub/Sub round-trip through main
- [ ] Request/Response round-trip with Zod contract
- [ ] Manifest permissions gate emit/subscribe/call
- [ ] Render process gets a scoped proxy only
- [ ] Tests cover allow and deny paths at the bus seam
