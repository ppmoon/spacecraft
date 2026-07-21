---
status: accepted
---

# Host runtime is Tauri (Rust), not Electron

Phase 1 needs a tray-resident microkernel workbench with OS windows, permissioned plugins, and an embedded terminal. We previously locked Electron + TypeScript + `node-pty`, but the primary product constraint is **runtime size/memory** for an always-on tray shell. We accept **Tauri (Rust Host)** instead of Electron: privileged plugins run **Rust Sidecars**; pure-UI plugins are Web (no Node); the Host remains the **sole Bus router** with contracts validated in Rust (Zod may author schemas on the JS side); the official terminal remains **xterm.js UI + Rust PTY Sidecar**. Electron Phase 1.01 work stays on its branch/PR as a reference prototype and does not merge to `main`; Phase 1 keeps the same seven tracer capabilities and order, re-specified for this stack.

## Considered Options

- **Stay on Electron** — rejected: ships a full Chromium runtime; fights the size/memory goal.
- **Tauri shell + Node plugin-hosts for everything** — rejected: reintroduces Node weight for common plugins and blurs the size win.
- **All-Rust plugins/UI** — rejected: forces third parties off the Web UI ecosystem too early.
- **Node bus daemon beside Tauri** — rejected: keeps Zod-centric routing at the cost of another heavy runtime and splits “sole router.”

## Consequences

- PRD #3 implementation decisions (Electron, TS end-to-end, `node-pty` in Node plugin-host) are superseded; rewrite via `/to-spec` then `/to-tickets` before `/implement`.
- Existing Electron PR/branch is reference-only, not `main`.
- Cloud-agent Linux may still struggle with GUI demos; that is an environment limit, not a reason to pick the stack.
