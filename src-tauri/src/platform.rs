//! OS / window / Sidecar boundary injected into the Host.
//! Production wires Tauri; tests use an in-memory fake.

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowKind {
    Blank,
    Launcher,
    Palette,
    /// Plugin window — Pure-UI surface (no raw Bus; scoped proxy only via Host).
    PureUi { plugin_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WindowId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrayId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SidecarId(pub String);

pub trait Platform: Send + Sync {
    fn create_tray(&self, on_quit: Box<dyn Fn() + Send + Sync>) -> TrayId;
    fn destroy_tray(&self, id: &TrayId);
    fn create_window(&self, kind: WindowKind) -> WindowId;
    /// Open a Plugin window loading `ui_entry`.
    fn create_pure_ui_window(&self, plugin_id: &str, ui_entry: &Path) -> WindowId;
    fn close_window(&self, id: &WindowId);
    fn is_window_destroyed(&self, id: &WindowId) -> bool;
    /// Whether the window may call privileged Host shell commands (not Bus).
    fn window_allows_privileged_apis(&self, id: &WindowId) -> bool;
    fn spawn_sidecar(&self, plugin_id: &str, entry: &Path) -> Result<SidecarId, String>;
    fn stop_sidecar(&self, id: &SidecarId);
    fn is_sidecar_running(&self, id: &SidecarId) -> bool;
    fn register_shortcut(&self, accelerator: &str, handler: Box<dyn Fn() + Send + Sync>);
    fn unregister_all_shortcuts(&self);
    fn quit(&self);
}
