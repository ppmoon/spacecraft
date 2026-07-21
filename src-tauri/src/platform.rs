//! OS / window boundary injected into the Host.
//! Production wires Tauri; tests use an in-memory fake.
//! Methods take `&self` so Host can call the Platform without fighting borrows;
//! implementations use interior mutability where needed.

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowKind {
    Blank,
    Launcher,
    Palette,
    /// Pure-UI Plugin window — local UI, no privileged Host/Sidecar APIs.
    PureUi { plugin_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WindowId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrayId(pub String);

pub trait Platform: Send + Sync {
    fn create_tray(&self, on_quit: Box<dyn Fn() + Send + Sync>) -> TrayId;
    fn destroy_tray(&self, id: &TrayId);
    fn create_window(&self, kind: WindowKind) -> WindowId;
    /// Open a Pure-UI Plugin window loading `ui_entry` (absolute or plugin-relative path).
    fn create_pure_ui_window(&self, plugin_id: &str, ui_entry: &Path) -> WindowId;
    fn close_window(&self, id: &WindowId);
    fn is_window_destroyed(&self, id: &WindowId) -> bool;
    /// Whether the window may call privileged Host commands / Sidecar APIs.
    fn window_allows_privileged_apis(&self, id: &WindowId) -> bool;
    fn register_shortcut(&self, accelerator: &str, handler: Box<dyn Fn() + Send + Sync>);
    fn unregister_all_shortcuts(&self);
    fn quit(&self);
}
