//! Host — tray, launcher, command palette, blank windows.
//! Behaviour is exercised at the Host seam via an injected Platform.

use std::sync::{Arc, Mutex};

use crate::platform::{Platform, TrayId, WindowId, WindowKind};

pub struct Host {
    platform: Arc<dyn Platform>,
    inner: Mutex<HostState>,
}

struct HostState {
    running: bool,
    tray: Option<TrayId>,
    launcher: Option<WindowId>,
    palette: Option<WindowId>,
    blanks: Vec<WindowId>,
}

impl Host {
    pub fn new(platform: Arc<dyn Platform>) -> Self {
        Self {
            platform,
            inner: Mutex::new(HostState {
                running: false,
                tray: None,
                launcher: None,
                palette: None,
                blanks: Vec::new(),
            }),
        }
    }

    pub fn start(self: &Arc<Self>) {
        {
            let state = self.inner.lock().expect("host");
            if state.running {
                return;
            }
        }

        let host_for_tray = Arc::clone(self);
        let platform_for_quit = Arc::clone(&self.platform);
        let tray = self.platform.create_tray(Box::new(move || {
            host_for_tray.stop();
            platform_for_quit.quit();
        }));

        let host_for_shortcut = Arc::clone(self);
        self.platform.register_shortcut(
            "CommandOrControl+K",
            Box::new(move || {
                host_for_shortcut.open_command_palette();
            }),
        );

        let mut state = self.inner.lock().expect("host");
        state.tray = Some(tray);
        state.running = true;
    }

    pub fn stop(&self) {
        let mut state = self.inner.lock().expect("host");
        if !state.running {
            return;
        }

        self.platform.unregister_all_shortcuts();
        Self::prune_locked(&self.platform, &mut state);

        if let Some(id) = state.launcher.take() {
            self.platform.close_window(&id);
        }
        if let Some(id) = state.palette.take() {
            self.platform.close_window(&id);
        }
        for id in state.blanks.drain(..) {
            self.platform.close_window(&id);
        }
        if let Some(tray) = state.tray.take() {
            self.platform.destroy_tray(&tray);
        }
        state.running = false;
    }

    pub fn is_tray_visible(&self) -> bool {
        self.inner.lock().expect("host").tray.is_some()
    }

    pub fn is_running(&self) -> bool {
        self.inner.lock().expect("host").running
    }

    pub fn open_launcher(&self) {
        let mut state = self.inner.lock().expect("host");
        Self::prune_locked(&self.platform, &mut state);
        if state.launcher.is_some() {
            return;
        }
        state.launcher = Some(self.platform.create_window(WindowKind::Launcher));
    }

    pub fn close_launcher(&self) {
        let mut state = self.inner.lock().expect("host");
        Self::prune_locked(&self.platform, &mut state);
        if let Some(id) = state.launcher.take() {
            self.platform.close_window(&id);
        }
    }

    pub fn is_launcher_open(&self) -> bool {
        let mut state = self.inner.lock().expect("host");
        Self::prune_locked(&self.platform, &mut state);
        state.launcher.is_some()
    }

    pub fn open_command_palette(&self) {
        let mut state = self.inner.lock().expect("host");
        Self::prune_locked(&self.platform, &mut state);
        if state.palette.is_some() {
            return;
        }
        state.palette = Some(self.platform.create_window(WindowKind::Palette));
    }

    pub fn close_command_palette(&self) {
        let mut state = self.inner.lock().expect("host");
        Self::prune_locked(&self.platform, &mut state);
        if let Some(id) = state.palette.take() {
            self.platform.close_window(&id);
        }
    }

    pub fn is_command_palette_open(&self) -> bool {
        let mut state = self.inner.lock().expect("host");
        Self::prune_locked(&self.platform, &mut state);
        state.palette.is_some()
    }

    pub fn open_blank_window(&self) {
        let mut state = self.inner.lock().expect("host");
        state
            .blanks
            .push(self.platform.create_window(WindowKind::Blank));
    }

    pub fn open_windows(&self) -> Vec<(String, WindowKind)> {
        let mut state = self.inner.lock().expect("host");
        Self::prune_locked(&self.platform, &mut state);
        let mut result = Vec::new();
        if let Some(id) = &state.launcher {
            result.push((id.0.clone(), WindowKind::Launcher));
        }
        if let Some(id) = &state.palette {
            result.push((id.0.clone(), WindowKind::Palette));
        }
        for id in &state.blanks {
            result.push((id.0.clone(), WindowKind::Blank));
        }
        result
    }

    fn prune_locked(platform: &Arc<dyn Platform>, state: &mut HostState) {
        if state
            .launcher
            .as_ref()
            .is_some_and(|id| platform.is_window_destroyed(id))
        {
            state.launcher = None;
        }
        if state
            .palette
            .as_ref()
            .is_some_and(|id| platform.is_window_destroyed(id))
        {
            state.palette = None;
        }
        state
            .blanks
            .retain(|id| !platform.is_window_destroyed(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_platform::MemoryPlatform;

    fn boot() -> (Arc<Host>, MemoryPlatform) {
        let platform = MemoryPlatform::new();
        let host = Arc::new(Host::new(Arc::new(platform.clone())));
        host.start();
        (host, platform)
    }

    #[test]
    fn host_boots_with_tray_visible() {
        let (host, _) = boot();
        assert!(host.is_tray_visible());
    }

    #[test]
    fn tray_quit_stops_the_host() {
        let (host, platform) = boot();
        platform.trigger_tray_quit();
        assert!(!host.is_running());
        assert!(!host.is_tray_visible());
        assert!(platform.did_quit());
    }

    #[test]
    fn launcher_can_be_opened_and_closed() {
        let (host, _) = boot();
        host.open_launcher();
        assert!(host.is_launcher_open());
        assert!(host
            .open_windows()
            .iter()
            .any(|(_, k)| *k == WindowKind::Launcher));

        host.close_launcher();
        assert!(!host.is_launcher_open());
        assert!(!host
            .open_windows()
            .iter()
            .any(|(_, k)| *k == WindowKind::Launcher));
    }

    #[test]
    fn shortcut_opens_command_palette() {
        let (host, platform) = boot();
        let handler = platform
            .shortcuts()
            .get("CommandOrControl+K")
            .cloned()
            .expect("shortcut registered");
        handler();
        assert!(host.is_command_palette_open());
        assert!(host
            .open_windows()
            .iter()
            .any(|(_, k)| *k == WindowKind::Palette));
    }

    #[test]
    fn command_palette_can_open_blank_os_window() {
        let (host, _) = boot();
        host.open_command_palette();
        host.open_blank_window();
        let blanks: Vec<_> = host
            .open_windows()
            .into_iter()
            .filter(|(_, k)| *k == WindowKind::Blank)
            .collect();
        assert_eq!(blanks.len(), 1);
    }

    #[test]
    fn command_palette_can_be_closed() {
        let (host, _) = boot();
        host.open_command_palette();
        host.close_command_palette();
        assert!(!host.is_command_palette_open());
    }
}
