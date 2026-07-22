//! Host — tray, launcher, command palette, blank windows, Pure-UI Plugins.
//! Behaviour is exercised at the Host seam via an injected Platform.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::manifest::Manifest;
use crate::platform::{Platform, TrayId, WindowId, WindowKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListedPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
}

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
    plugin_windows: Vec<(WindowId, WindowKind)>,
    plugins: HashMap<String, Manifest>,
    plugins_dir: Option<PathBuf>,
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
                plugin_windows: Vec::new(),
                plugins: HashMap::new(),
                plugins_dir: None,
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
        for (id, _) in state.plugin_windows.drain(..) {
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

    /// Scan `dir` for Plugin folders with a valid Manifest. Invalid packages are skipped.
    /// Remembers `dir` so later launcher opens re-scan (drop-in discovery).
    pub fn load_plugins_from(&self, dir: &Path) {
        let discovered = Self::scan_plugins_dir(dir);
        let mut state = self.inner.lock().expect("host");
        state.plugins_dir = Some(dir.to_path_buf());
        state.plugins = discovered;
    }

    fn scan_plugins_dir(dir: &Path) -> HashMap<String, Manifest> {
        let mut discovered = HashMap::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return discovered;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if let Ok(manifest) = Manifest::load_from_plugin_dir(&path) {
                discovered.insert(manifest.id.clone(), manifest);
            }
        }
        discovered
    }

    fn rescan_plugins_locked(state: &mut HostState) {
        let Some(dir) = state.plugins_dir.clone() else {
            return;
        };
        state.plugins = Self::scan_plugins_dir(&dir);
    }

    pub fn listed_plugins(&self) -> Vec<ListedPlugin> {
        let mut state = self.inner.lock().expect("host");
        Self::rescan_plugins_locked(&mut state);
        let mut list: Vec<_> = state
            .plugins
            .values()
            .map(|m| ListedPlugin {
                id: m.id.clone(),
                name: m.name.clone(),
                version: m.version.clone(),
            })
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    pub fn open_plugin(&self, id: &str) -> Result<(), String> {
        let mut state = self.inner.lock().expect("host");
        Self::prune_locked(&self.platform, &mut state);
        let Some(manifest) = state.plugins.get(id).cloned() else {
            return Err(format!("unknown Plugin: {id}"));
        };
        let ui_entry = manifest.ui_entry_path();
        let plugin_id = manifest.id.clone();
        let window = self
            .platform
            .create_pure_ui_window(&plugin_id, &ui_entry);
        state.plugin_windows.push((
            window,
            WindowKind::PureUi { plugin_id },
        ));
        Ok(())
    }

    pub fn open_launcher(&self) {
        let mut state = self.inner.lock().expect("host");
        Self::rescan_plugins_locked(&mut state);
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
        for (id, kind) in &state.plugin_windows {
            result.push((id.0.clone(), kind.clone()));
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
        state
            .plugin_windows
            .retain(|(id, _)| !platform.is_window_destroyed(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_platform::MemoryPlatform;
    use std::fs;
    use std::path::PathBuf;

    fn boot() -> (Arc<Host>, MemoryPlatform) {
        let platform = MemoryPlatform::new();
        let host = Arc::new(Host::new(Arc::new(platform.clone())));
        host.start();
        (host, platform)
    }

    fn fixture_plugins_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins")
    }

    fn temp_plugins_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "spacecraft-plugins-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp plugins dir");
        dir
    }

    fn write_plugin(root: &Path, id: &str, manifest_body: &str, ui_name: &str) {
        let dir = root.join(id);
        fs::create_dir_all(&dir).expect("plugin dir");
        fs::write(dir.join("manifest.json"), manifest_body).expect("manifest");
        fs::write(dir.join(ui_name), "<html><body>hi</body></html>").expect("ui");
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

    #[test]
    fn host_lists_hello_plugin_from_plugins_directory() {
        let (host, _) = boot();
        host.load_plugins_from(&fixture_plugins_dir());
        let listed = host.listed_plugins();
        assert_eq!(
            listed,
            vec![ListedPlugin {
                id: "hello".into(),
                name: "Hello".into(),
                version: "0.1.0".into(),
            }]
        );
    }

    #[test]
    fn invalid_manifest_packages_are_not_listed() {
        let dir = temp_plugins_dir();
        write_plugin(
            &dir,
            "bad-empty-id",
            r#"{
              "id": "",
              "name": "Bad",
              "version": "1.0.0",
              "ui": "index.html",
              "window": { "type": "local" }
            }"#,
            "index.html",
        );
        write_plugin(
            &dir,
            "good",
            r#"{
              "id": "good",
              "name": "Good",
              "version": "1.0.0",
              "ui": "index.html",
              "window": { "type": "local" }
            }"#,
            "index.html",
        );
        // Missing ui file
        let missing_ui = dir.join("missing-ui");
        fs::create_dir_all(&missing_ui).unwrap();
        fs::write(
            missing_ui.join("manifest.json"),
            r#"{
              "id": "missing-ui",
              "name": "Missing",
              "version": "1.0.0",
              "ui": "nope.html",
              "window": { "type": "local" }
            }"#,
        )
        .unwrap();

        let (host, _) = boot();
        host.load_plugins_from(&dir);
        assert_eq!(
            host.listed_plugins(),
            vec![ListedPlugin {
                id: "good".into(),
                name: "Good".into(),
                version: "1.0.0".into(),
            }]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_hello_plugin_creates_pure_ui_window_with_local_ui() {
        let (host, platform) = boot();
        host.load_plugins_from(&fixture_plugins_dir());
        host.open_plugin("hello").expect("open hello");

        let pure: Vec<_> = host
            .open_windows()
            .into_iter()
            .filter(|(_, k)| matches!(k, WindowKind::PureUi { plugin_id } if plugin_id == "hello"))
            .collect();
        assert_eq!(pure.len(), 1);

        let id = WindowId(pure[0].0.clone());
        assert!(!platform.window_allows_privileged_apis(&id));

        let ui = platform.ui_entry_for(&id).expect("ui path recorded");
        assert!(ui.ends_with("plugins/hello/index.html"));
        assert!(ui.is_file());
    }

    #[test]
    fn opening_launcher_rescans_plugins_directory() {
        let dir = temp_plugins_dir();
        let (host, _) = boot();
        host.load_plugins_from(&dir);
        assert!(host.listed_plugins().is_empty());

        write_plugin(
            &dir,
            "dropped",
            r#"{
              "id": "dropped",
              "name": "Dropped",
              "version": "0.1.0",
              "ui": "index.html",
              "window": { "type": "local" }
            }"#,
            "index.html",
        );
        host.open_launcher();
        assert_eq!(
            host.listed_plugins(),
            vec![ListedPlugin {
                id: "dropped".into(),
                name: "Dropped".into(),
                version: "0.1.0".into(),
            }]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_plugin_open_fails() {
        let (host, _) = boot();
        let err = host.open_plugin("nope").expect_err("should fail");
        assert!(err.contains("unknown Plugin"));
    }
}
