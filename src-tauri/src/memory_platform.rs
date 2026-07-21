//! In-memory Platform for Host-seam tests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::platform::{Platform, SidecarId, TrayId, WindowId, WindowKind};

struct MemoryWindow {
    destroyed: bool,
    privileged: bool,
    ui_entry: Option<PathBuf>,
}

struct MemorySidecar {
    plugin_id: String,
    running: bool,
}

#[derive(Default)]
struct MemoryState {
    next_id: u64,
    trays: HashMap<String, ()>,
    windows: HashMap<String, MemoryWindow>,
    sidecars: HashMap<String, MemorySidecar>,
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

    pub fn ui_entry_for(&self, id: &WindowId) -> Option<PathBuf> {
        self.state
            .lock()
            .expect("memory platform")
            .windows
            .get(&id.0)
            .and_then(|w| w.ui_entry.clone())
    }

    pub fn running_sidecar_count_for(&self, plugin_id: &str) -> usize {
        self.state
            .lock()
            .expect("memory platform")
            .sidecars
            .values()
            .filter(|s| s.plugin_id == plugin_id && s.running)
            .count()
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

    fn create_window(&self, _kind: WindowKind) -> WindowId {
        let mut state = self.state.lock().expect("memory platform");
        state.next_id += 1;
        let id = format!("win-{}", state.next_id);
        state.windows.insert(
            id.clone(),
            MemoryWindow {
                destroyed: false,
                privileged: true,
                ui_entry: None,
            },
        );
        WindowId(id)
    }

    fn create_pure_ui_window(&self, plugin_id: &str, ui_entry: &Path) -> WindowId {
        let mut state = self.state.lock().expect("memory platform");
        state.next_id += 1;
        let id = format!("plugin-{}-{}", plugin_id, state.next_id);
        state.windows.insert(
            id.clone(),
            MemoryWindow {
                destroyed: false,
                privileged: false,
                ui_entry: Some(ui_entry.to_path_buf()),
            },
        );
        WindowId(id)
    }

    fn close_window(&self, id: &WindowId) {
        let mut state = self.state.lock().expect("memory platform");
        if let Some(entry) = state.windows.get_mut(&id.0) {
            entry.destroyed = true;
        }
    }

    fn is_window_destroyed(&self, id: &WindowId) -> bool {
        let state = self.state.lock().expect("memory platform");
        state
            .windows
            .get(&id.0)
            .map(|w| w.destroyed)
            .unwrap_or(true)
    }

    fn window_allows_privileged_apis(&self, id: &WindowId) -> bool {
        let state = self.state.lock().expect("memory platform");
        state
            .windows
            .get(&id.0)
            .map(|w| w.privileged && !w.destroyed)
            .unwrap_or(false)
    }

    fn spawn_sidecar(&self, plugin_id: &str) -> SidecarId {
        let mut state = self.state.lock().expect("memory platform");
        state.next_id += 1;
        let id = format!("sidecar-{}-{}", plugin_id, state.next_id);
        state.sidecars.insert(
            id.clone(),
            MemorySidecar {
                plugin_id: plugin_id.to_string(),
                running: true,
            },
        );
        SidecarId(id)
    }

    fn stop_sidecar(&self, id: &SidecarId) {
        let mut state = self.state.lock().expect("memory platform");
        if let Some(s) = state.sidecars.get_mut(&id.0) {
            s.running = false;
        }
    }

    fn is_sidecar_running(&self, id: &SidecarId) -> bool {
        let state = self.state.lock().expect("memory platform");
        state
            .sidecars
            .get(&id.0)
            .map(|s| s.running)
            .unwrap_or(false)
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
