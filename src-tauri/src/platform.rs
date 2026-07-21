//! OS / window boundary injected into the Host.
//! Production wires Tauri; tests use an in-memory fake.
//! Methods take `&self` so Host can call the Platform without fighting borrows;
//! implementations use interior mutability where needed.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKind {
    Blank,
    Launcher,
    Palette,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WindowId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrayId(pub String);

pub trait Platform: Send + Sync {
    fn create_tray(&self, on_quit: Box<dyn Fn() + Send + Sync>) -> TrayId;
    fn destroy_tray(&self, id: &TrayId);
    fn create_window(&self, kind: WindowKind) -> WindowId;
    fn close_window(&self, id: &WindowId);
    fn is_window_destroyed(&self, id: &WindowId) -> bool;
    fn register_shortcut(&self, accelerator: &str, handler: Box<dyn Fn() + Send + Sync>);
    fn unregister_all_shortcuts(&self);
    fn quit(&self);
}
