# Phase 1 — Host skeleton → Manifest/Bus → Terminal demo

## Destination

A runnable **Electron microkernel workbench** that can load a local plugin via manifest, route permissioned bus traffic through a dedicated plugin-host process, and prove the model with an official **embedded terminal** plugin (`xterm.js` + `node-pty`) on Windows, macOS, and the current test Linux.

## Product (locked)

- Extensible desktop workbench for office workers (engineers, designers, PMs, ops, analysts).
- Prefer commercially permissive open-source libraries.
- Host provides window shell, WebView containers, and event bus; plugins own content.

## Architecture decisions in scope for Phase 1

1. Electron custom shell + custom plugin protocol (do not fork VS Code).
2. OS-level multi-window + window groups.
3. Shell UI: tray + light launcher + command palette.
4. Workspace: host owns layout restore; plugins own business state.
5. Process model: main + one plugin-host process per plugin + one render process per window.
6. Bus: Pub/Sub + Request/Response; main routes; namespaced + Zod contracts + manifest permissions.
7. Window content: local UI by default; remote URL requires permission + domain allowlist (thin support ok; terminal is the proof).
8. Package shape: directory/zip + declarative manifest.
9. v1 distribution: local install; store later; show & confirm permissions on install (signature field reserved).
10. Instances: multi-instance by default; optional `singleton`.
11. Official empty core + unloadable demo plugins; terminal is the proving plugin (embedded only — no external iTerm/system terminal).
12. Stack: TypeScript end-to-end + Zod; UI framework not mandated.
13. Platforms: Win + macOS + current test-environment Linux.

## Out of scope (Phase 1)

- Plugin marketplace / store
- Mandatory code signing / publisher identity
- Forking or embedding VS Code / Theia
- Opening external terminal apps (iTerm2, Windows Terminal, etc.)
- Role-based mega-suites (full IDE / design / data packs)
- Arbitrary Linux distro matrix beyond the current test environment
- Cross-device sync, accounts, cloud backup
- Auto-update channels (unless trivial scaffolding appears while packing)

## Ticket chain (tracer bullets)

Work the frontier top-down. Each ticket is a vertical slice: demoable end-to-end, sized for one fresh agent context.

Parent PRD: [#3](https://github.com/ppmoon/spacecraft/issues/3)

| # | Issue | Title | Blocked by |
|---|-------|-------|------------|
| 01 | [#4](https://github.com/ppmoon/spacecraft/issues/4) | Host boots with tray, launcher, and command palette | — |
| 02 | [#5](https://github.com/ppmoon/spacecraft/issues/5) | Hello plugin loads from a folder and opens a local UI window | 01 |
| 03 | [#6](https://github.com/ppmoon/spacecraft/issues/6) | Plugin host process speaks on the permissioned dual-channel bus | 02 |
| 04 | [#7](https://github.com/ppmoon/spacecraft/issues/7) | Install a local zip/folder plugin with permission confirmation | 02 |
| 05 | [#8](https://github.com/ppmoon/spacecraft/issues/8) | Workspace restores window layout across restarts | 02 |
| 06 | [#9](https://github.com/ppmoon/spacecraft/issues/9) | Window groups open related windows together | 05 |
| 07 | [#10](https://github.com/ppmoon/spacecraft/issues/10) | Official terminal demo plugin (xterm.js + node-pty) | 03 |

Local mirrors: `.scratch/phase-1/issues/`.

```
01 Host
 └─ 02 Hello plugin (manifest + local UI window)
      ├─ 03 Plugin host + bus ──► 07 Terminal demo
      ├─ 04 Local install + permission confirm
      └─ 05 Workspace restore ──► 06 Window groups
```

## Testing seams (Phase 1)

Prefer the highest seam that still fails for the right reason:

1. **Host integration** — boot app headlessly/CI where possible; open launcher actions; assert a window exists.
2. **Plugin load seam** — given a fixture plugin directory, host lists it and opens its UI URL via the custom protocol.
3. **Bus seam** — fixture plugin-host ↔ main: publish/subscribe and request/response round-trips; reject undeclared permissions.
4. **Terminal seam** — spawn PTY, write a command, observe output in the terminal UI bridge (or a test double for the xterm surface).

Do not assert internal file layout or private class names. Prefer fixture plugins over mocks of the bus itself.

## Domain vocabulary (starter)

| Term | Meaning |
|------|---------|
| Host | Electron main + shell UI (tray, launcher, command palette) |
| Plugin | Packaged extension with manifest, optional host logic, optional UI |
| Manifest | Declarative contract: id, entries, windows, permissions, bus contracts |
| Plugin host process | Dedicated Node process for one plugin’s logic |
| Window | OS-level `BrowserWindow` / WebView container filled by a plugin |
| Window group | Named set of related windows opened/closed together |
| Workspace | Persisted layout of windows/groups + plugin instance ids (not business state) |
| Bus | Main-routed Pub/Sub + Req/Response with Zod validation and permissions |
| Instance | One running copy of a plugin window/session (`instanceId`) |

See `.scratch/phase-1/` for local ticket mirrors and the issue index.
