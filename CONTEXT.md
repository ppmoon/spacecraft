# Spacecraft

Extensible desktop workbench: a microkernel **Host** that loads **Plugins**, opens OS-level windows, and routes a permissioned **Bus**.

## Language

**Host**:
The microkernel shell process that owns the tray/launcher/palette, window lifecycle, plugin install surface, and the Bus router.
_Avoid_: App shell, main, Electron main, Tauri app (when speaking in domain terms)

**Plugin**:
A packaged extension with a declarative Manifest. May be a pure-UI plugin or a privileged plugin that also runs a Sidecar.
_Avoid_: Extension, add-on, app (ambiguous with Host)

**Manifest**:
The declarative package descriptor for a Plugin: identity, entrypoints, window types, permissions, and Bus contracts.
_Avoid_: package.json (unless literally the npm file), config

**Bus**:
The Host-owned dual-channel message fabric (Pub/Sub and Request/Response) that is the sole cross-plugin router, with manifest-declared permissions and validated contracts.
_Avoid_: IPC, event emitter, Tauri event (implementation)

**Sidecar**:
A dedicated out-of-process worker for a privileged Plugin’s non-UI logic (e.g. PTY), isolated so a crash does not take down other plugins or the Host.
_Avoid_: plugin-host (legacy Electron term), helper, daemon (unless OS-level)

**Pure-UI Plugin**:
A Plugin whose logic runs only in its window UI surface, with no Sidecar; isolation is at the window/webview instance.
_Avoid_: lightweight plugin (vague)

**Workspace**:
The Host-persisted layout of open windows and plugin instance identities across restarts. Plugins persist their own business state separately.
_Avoid_: session, desktop save

**Window Group**:
A Host-managed set of related windows that open or close together.
_Avoid_: workspace (different concept), tab set
