//! In-memory Platform for Host-seam tests.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::platform::{Platform, TrayId, WindowId, WindowKind};

#[derive(Default)]
struct MemoryState {
    next_id: u64,
    trays: HashMap<String, ()>,
    windows: HashMap<String, (WindowKind, bool)>,
    shortcuts: HashMap<String, Arc<dyn Fn() + Send + Sync>>,
    quit_called: bool,
    tray_quit: Option<Arc<dyn Fn() + Send + Sync>>,
}

#[derive(Clone, Default)]
pub struct MemoryPlatform {
    state: Arc<Mutex<MemoryState>>,
}

impl MemoryPlatform {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn did_quit(&self) -> bool {
        self.state.lock().expect("memory platform").quit_called
    }

    pub fn shortcuts(&self) -> HashMap<String, Arc<dyn Fn() + Send + Sync>> {
        self.state.lock().expect("memory platform").shortcuts.clone()
    }

    pub fn trigger_tray_quit(&self) {
        let quit = self
            .state
            .lock()
            .expect("memory platform")
            .tray_quit
            .clone();
        if let Some(quit) = quit {
            quit();
        }
    }
}

impl Platform for MemoryPlatform {
    fn create_tray(&self, on_quit: Box<dyn Fn() + Send + Sync>) -> TrayId {
        let mut state = self.state.lock().expect("memory platform");
        state.next_id += 1;
        let id = format!("tray-{}", state.next_id);
        state.trays.insert(id.clone(), ());
        let on_quit: Arc<dyn Fn() + Send + Sync> = Arc::from(on_quit);
        state.tray_quit = Some(on_quit);
        TrayId(id)
    }

    fn destroy_tray(&self, id: &TrayId) {
        let mut state = self.state.lock().expect("memory platform");
        state.trays.remove(&id.0);
        state.tray_quit = None;
    }

    fn create_window(&self, kind: WindowKind) -> WindowId {
        let mut state = self.state.lock().expect("memory platform");
        state.next_id += 1;
        let id = format!("win-{}", state.next_id);
        state.windows.insert(id.clone(), (kind, false));
        WindowId(id)
    }

    fn close_window(&self, id: &WindowId) {
        let mut state = self.state.lock().expect("memory platform");
        if let Some(entry) = state.windows.get_mut(&id.0) {
            entry.1 = true;
        }
    }

    fn is_window_destroyed(&self, id: &WindowId) -> bool {
        let state = self.state.lock().expect("memory platform");
        state
            .windows
            .get(&id.0)
            .map(|(_, destroyed)| *destroyed)
            .unwrap_or(true)
    }

    fn register_shortcut(&self, accelerator: &str, handler: Box<dyn Fn() + Send + Sync>) {
        let mut state = self.state.lock().expect("memory platform");
        state
            .shortcuts
            .insert(accelerator.to_string(), Arc::from(handler));
    }

    fn unregister_all_shortcuts(&self) {
        let mut state = self.state.lock().expect("memory platform");
        state.shortcuts.clear();
    }

    fn quit(&self) {
        let mut state = self.state.lock().expect("memory platform");
        state.quit_called = true;
    }
}
