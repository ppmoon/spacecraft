//! Host — tray, launcher, command palette, Plugins, Sidecar lifecycle, Bus router.
//! Behaviour is exercised at the Host / Bus seam via an injected Platform.
//! Window UI never receives a raw Bus — only a scoped `BusProxy`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;

use crate::bus::{Bus, BusError, BusProxy};
use crate::install::{
    commit_staged_install, permission_items, stage_from_folder, stage_from_zip, InstallProposal,
    PendingInstall, PermissionItem,
};
use crate::manifest::Manifest;
use crate::platform::{Platform, SidecarId, TrayId, WindowId, WindowKind};
use crate::sidecar_bridge;
use crate::workspace::{
    WindowGeometry, Workspace, WorkspaceGroup, WorkspaceGroupMember, WorkspaceWindow,
    WorkspaceWindowKind,
};
use std::process::Child;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListedPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListedWindowGroup {
    pub id: String,
    pub name: String,
    pub member_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListedContentWindow {
    pub label: String,
    pub kind: String,
    pub plugin_id: Option<String>,
    pub instance_id: Option<String>,
}

struct BlankWindowEntry {
    id: WindowId,
    blank_id: String,
}

struct PluginWindowEntry {
    id: WindowId,
    plugin_id: String,
    instance_id: String,
}

struct WindowGroupEntry {
    id: String,
    name: String,
    members: Vec<WorkspaceGroupMember>,
}

pub struct Host {
    platform: Arc<dyn Platform>,
    bus: Arc<Bus>,
    inner: Mutex<HostState>,
}

struct HostState {
    running: bool,
    tray: Option<TrayId>,
    launcher: Option<WindowId>,
    palette: Option<WindowId>,
    blanks: Vec<BlankWindowEntry>,
    plugin_windows: Vec<PluginWindowEntry>,
    window_groups: Vec<WindowGroupEntry>,
    /// plugin_id → (platform lifecycle id, child process)
    sidecars: HashMap<String, (SidecarId, Child)>,
    plugins: HashMap<String, Manifest>,
    plugins_dir: Option<PathBuf>,
    pending_install: Option<PendingInstall>,
    /// plugin_id → confirmed permission grants from install
    confirmed_grants: HashMap<String, Vec<PermissionItem>>,
    staging_root: PathBuf,
    workspace_path: Option<PathBuf>,
}

impl Host {
    pub fn new(platform: Arc<dyn Platform>) -> Self {
        Self {
            platform,
            bus: Arc::new(Bus::new()),
            inner: Mutex::new(HostState {
                running: false,
                tray: None,
                launcher: None,
                palette: None,
                blanks: Vec::new(),
                plugin_windows: Vec::new(),
                window_groups: Vec::new(),
                sidecars: HashMap::new(),
                plugins: HashMap::new(),
                plugins_dir: None,
                pending_install: None,
                confirmed_grants: HashMap::new(),
                staging_root: std::env::temp_dir().join(format!(
                    "spacecraft-install-staging-{}",
                    uuid::Uuid::new_v4()
                )),
                workspace_path: None,
            }),
        }
    }

    /// Path where Workspace layout is persisted on stop / restore.
    pub fn set_workspace_path(&self, path: PathBuf) {
        self.inner.lock().expect("host").workspace_path = Some(path);
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

        Self::prune_locked(&self.platform, &mut state);
        if let Some(path) = state.workspace_path.clone() {
            let snapshot = Self::capture_workspace_locked(&self.platform, &state);
            drop(state);
            if let Err(e) = snapshot.save_to_path(&path) {
                eprintln!("spacecraft: workspace save failed: {e}");
            }
            state = self.inner.lock().expect("host");
        }

        self.platform.unregister_all_shortcuts();
        Self::prune_locked(&self.platform, &mut state);

        if let Some(id) = state.launcher.take() {
            self.platform.close_window(&id);
        }
        if let Some(id) = state.palette.take() {
            self.platform.close_window(&id);
        }
        for entry in state.blanks.drain(..) {
            self.platform.close_window(&entry.id);
        }
        for entry in state.plugin_windows.drain(..) {
            self.platform.close_window(&entry.id);
        }
        state.window_groups.clear();
        for (_, (sidecar_id, mut child)) in state.sidecars.drain() {
            let _ = child.kill();
            let _ = child.wait();
            self.platform.stop_sidecar(&sidecar_id);
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
        let grants = load_grants(dir);
        let mut state = self.inner.lock().expect("host");
        state.plugins_dir = Some(dir.to_path_buf());
        state.plugins = discovered;
        state.confirmed_grants = grants;
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

    /// Inspect a local Plugin folder and return a permission proposal (no install yet).
    pub fn propose_install_from_folder(&self, source: &Path) -> Result<InstallProposal, String> {
        let mut state = self.inner.lock().expect("host");
        Self::clear_pending_locked(&mut state);
        let staging_root = state.staging_root.clone();
        drop(state);
        std::fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
        let (staging_dir, manifest) = stage_from_folder(source, &staging_root)?;
        self.store_pending(staging_dir, manifest, "folder")
    }

    /// Inspect a local Plugin zip and return a permission proposal (no install yet).
    pub fn propose_install_from_zip(&self, source: &Path) -> Result<InstallProposal, String> {
        let mut state = self.inner.lock().expect("host");
        Self::clear_pending_locked(&mut state);
        let staging_root = state.staging_root.clone();
        drop(state);
        std::fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
        let (staging_dir, manifest) = stage_from_zip(source, &staging_root)?;
        self.store_pending(staging_dir, manifest, "zip")
    }

    fn store_pending(
        &self,
        staging_dir: PathBuf,
        manifest: Manifest,
        source_kind: &str,
    ) -> Result<InstallProposal, String> {
        let proposal = InstallProposal {
            proposal_id: uuid::Uuid::new_v4().to_string(),
            plugin_id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            permissions: permission_items(&manifest),
            signature_present: manifest.signature.is_some(),
            source_kind: source_kind.to_string(),
        };
        let mut state = self.inner.lock().expect("host");
        state.pending_install = Some(PendingInstall {
            proposal: proposal.clone(),
            staging_dir,
            manifest,
        });
        Ok(proposal)
    }

    pub fn pending_install(&self) -> Option<InstallProposal> {
        self.inner
            .lock()
            .expect("host")
            .pending_install
            .as_ref()
            .map(|p| p.proposal.clone())
    }

    /// Confirm the pending proposal: copy into plugins dir and record grants.
    pub fn confirm_install(&self, proposal_id: &str) -> Result<ListedPlugin, String> {
        let mut state = self.inner.lock().expect("host");
        let pending = state
            .pending_install
            .take()
            .ok_or_else(|| "no pending Plugin install".to_string())?;
        if pending.proposal.proposal_id != proposal_id {
            state.pending_install = Some(pending);
            return Err("proposal id mismatch".into());
        }
        let plugins_dir = state
            .plugins_dir
            .clone()
            .ok_or_else(|| "plugins directory is not configured".to_string())?;
        let staging = pending.staging_dir.clone();
        let plugin_id = pending.manifest.id.clone();
        let grants = pending.proposal.permissions.clone();
        drop(state);

        commit_staged_install(&staging, &plugins_dir, &plugin_id)?;
        let _ = std::fs::remove_dir_all(&staging);
        persist_grants(&plugins_dir, &plugin_id, &grants)?;

        let mut state = self.inner.lock().expect("host");
        state.confirmed_grants.insert(plugin_id.clone(), grants);
        Self::rescan_plugins_locked(&mut state);
        let listed = state
            .plugins
            .get(&plugin_id)
            .map(|m| ListedPlugin {
                id: m.id.clone(),
                name: m.name.clone(),
                version: m.version.clone(),
            })
            .ok_or_else(|| "installed Plugin did not appear after confirm".to_string())?;
        Ok(listed)
    }

    /// Decline the pending proposal; leave the workbench unchanged.
    pub fn decline_install(&self, proposal_id: &str) -> Result<(), String> {
        let mut state = self.inner.lock().expect("host");
        let pending = state
            .pending_install
            .take()
            .ok_or_else(|| "no pending Plugin install".to_string())?;
        if pending.proposal.proposal_id != proposal_id {
            state.pending_install = Some(pending);
            return Err("proposal id mismatch".into());
        }
        let staging = pending.staging_dir;
        drop(state);
        let _ = std::fs::remove_dir_all(staging);
        Ok(())
    }

    pub fn confirmed_grants_for(&self, plugin_id: &str) -> Option<Vec<PermissionItem>> {
        self.inner
            .lock()
            .expect("host")
            .confirmed_grants
            .get(plugin_id)
            .cloned()
    }

    fn clear_pending_locked(state: &mut HostState) {
        if let Some(pending) = state.pending_install.take() {
            let _ = std::fs::remove_dir_all(pending.staging_dir);
        }
    }

    /// Scoped Bus proxy for a loaded Plugin surface (window UI or test double).
    /// This is the only Bus entry point — there is no raw global Bus API.
    pub fn scoped_bus(&self, plugin_id: &str) -> Result<BusProxy, BusError> {
        let state = self.inner.lock().expect("host");
        let Some(manifest) = state.plugins.get(plugin_id) else {
            return Err(BusError::NotLoaded(plugin_id.to_string()));
        };
        Ok(self.bus.proxy(
            manifest.id.clone(),
            manifest.permissions.clone(),
            manifest.contracts.clone(),
        ))
    }

    pub fn open_plugin(&self, id: &str) -> Result<(), String> {
        {
            let state = self.inner.lock().expect("host");
            if !state.plugins.contains_key(id) {
                return Err(format!("unknown Plugin: {id}"));
            }
        }
        let instance_id = uuid::Uuid::new_v4().to_string();
        self.open_plugin_instance(id, &instance_id, None)?;
        Ok(())
    }

    pub fn sidecar_running_for(&self, plugin_id: &str) -> bool {
        let mut state = self.inner.lock().expect("host");
        if let Some((id, child)) = state.sidecars.get_mut(plugin_id) {
            match child.try_wait() {
                Ok(None) => self.platform.is_sidecar_running(id),
                Ok(Some(_)) | Err(_) => false,
            }
        } else {
            false
        }
    }

    pub fn close_plugin_windows(&self, plugin_id: &str) {
        let mut state = self.inner.lock().expect("host");
        let mut kept = Vec::new();
        for entry in state.plugin_windows.drain(..) {
            if entry.plugin_id == plugin_id {
                self.platform.close_window(&entry.id);
            } else {
                kept.push(entry);
            }
        }
        state.plugin_windows = kept;
        Self::stop_sidecar_locked(&self.platform, &mut state, plugin_id);
    }

    /// Resolve Plugin id from a window label (`plugin-{id}-{n}`).
    pub fn plugin_id_for_window_label(&self, label: &str) -> Option<String> {
        let state = self.inner.lock().expect("host");
        state.plugin_windows.iter().find_map(|entry| {
            if entry.id.0 == label {
                Some(entry.plugin_id.clone())
            } else {
                None
            }
        })
    }

    pub fn bus_emit_from_window(
        &self,
        window_label: &str,
        topic: &str,
        payload: Value,
    ) -> Result<(), String> {
        let plugin_id = self
            .plugin_id_for_window_label(window_label)
            .ok_or_else(|| "window is not a Plugin surface".to_string())?;
        let proxy = self.scoped_bus(&plugin_id).map_err(|e| e.to_string())?;
        proxy.emit(topic, payload).map_err(|e| e.to_string())
    }

    pub fn bus_call_from_window(
        &self,
        window_label: &str,
        topic: &str,
        payload: Value,
    ) -> Result<Value, String> {
        let plugin_id = self
            .plugin_id_for_window_label(window_label)
            .ok_or_else(|| "window is not a Plugin surface".to_string())?;
        let proxy = self.scoped_bus(&plugin_id).map_err(|e| e.to_string())?;
        proxy.call(topic, payload).map_err(|e| e.to_string())
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
        let blank_id = uuid::Uuid::new_v4().to_string();
        let id = self.platform.create_window(WindowKind::Blank);
        state.blanks.push(BlankWindowEntry { id, blank_id });
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
        for entry in &state.blanks {
            result.push((entry.id.0.clone(), WindowKind::Blank));
        }
        for entry in &state.plugin_windows {
            result.push((
                entry.id.0.clone(),
                WindowKind::PureUi {
                    plugin_id: entry.plugin_id.clone(),
                },
            ));
        }
        result
    }

    /// Content windows eligible for Window Groups (blanks + Plugins; not launcher/palette).
    pub fn list_content_windows(&self) -> Vec<ListedContentWindow> {
        let mut state = self.inner.lock().expect("host");
        Self::prune_locked(&self.platform, &mut state);
        let mut result = Vec::new();
        for entry in &state.blanks {
            result.push(ListedContentWindow {
                label: entry.id.0.clone(),
                kind: "blank".into(),
                plugin_id: None,
                instance_id: Some(entry.blank_id.clone()),
            });
        }
        for entry in &state.plugin_windows {
            result.push(ListedContentWindow {
                label: entry.id.0.clone(),
                kind: "plugin".into(),
                plugin_id: Some(entry.plugin_id.clone()),
                instance_id: Some(entry.instance_id.clone()),
            });
        }
        result
    }

    /// Bind currently open content windows into a named Window Group.
    pub fn create_window_group(
        &self,
        name: &str,
        window_labels: &[String],
    ) -> Result<ListedWindowGroup, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Window Group name is required".into());
        }
        if window_labels.is_empty() {
            return Err("Window Group needs at least one window".into());
        }

        let mut state = self.inner.lock().expect("host");
        Self::prune_locked(&self.platform, &mut state);

        let mut members = Vec::new();
        for label in window_labels {
            if let Some(blank) = state.blanks.iter().find(|e| e.id.0 == *label) {
                members.push(WorkspaceGroupMember::Blank {
                    blank_id: blank.blank_id.clone(),
                });
                continue;
            }
            if let Some(plugin) = state.plugin_windows.iter().find(|e| e.id.0 == *label) {
                members.push(WorkspaceGroupMember::Plugin {
                    plugin_id: plugin.plugin_id.clone(),
                    instance_id: plugin.instance_id.clone(),
                });
                continue;
            }
            return Err(format!("unknown content window: {label}"));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let listed = ListedWindowGroup {
            id: id.clone(),
            name: name.to_string(),
            member_count: members.len(),
        };
        state.window_groups.push(WindowGroupEntry {
            id,
            name: name.to_string(),
            members,
        });
        Ok(listed)
    }

    /// Declare a Window Group by opening the given Plugins as members now.
    pub fn open_window_group_declared(
        &self,
        name: &str,
        plugin_ids: &[String],
    ) -> Result<ListedWindowGroup, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Window Group name is required".into());
        }
        if plugin_ids.is_empty() {
            return Err("Window Group needs at least one Plugin".into());
        }

        let mut labels = Vec::new();
        for plugin_id in plugin_ids {
            {
                let state = self.inner.lock().expect("host");
                if !state.plugins.contains_key(plugin_id) {
                    return Err(format!("unknown Plugin: {plugin_id}"));
                }
            }
            let instance_id = uuid::Uuid::new_v4().to_string();
            let window_id = self
                .open_plugin_instance(plugin_id, &instance_id, None)?
                .ok_or_else(|| format!("unknown Plugin: {plugin_id}"))?;
            labels.push(window_id.0);
        }
        self.create_window_group(name, &labels)
    }

    pub fn list_window_groups(&self) -> Vec<ListedWindowGroup> {
        let state = self.inner.lock().expect("host");
        state
            .window_groups
            .iter()
            .map(|g| ListedWindowGroup {
                id: g.id.clone(),
                name: g.name.clone(),
                member_count: g.members.len(),
            })
            .collect()
    }

    /// Open all member windows for a Window Group (skips members already open).
    pub fn open_window_group(&self, group_id: &str) -> Result<(), String> {
        let members = {
            let state = self.inner.lock().expect("host");
            let group = state
                .window_groups
                .iter()
                .find(|g| g.id == group_id)
                .ok_or_else(|| format!("unknown Window Group: {group_id}"))?;
            group.members.clone()
        };

        for member in members {
            match member {
                WorkspaceGroupMember::Blank { blank_id } => {
                    let already_open = {
                        let state = self.inner.lock().expect("host");
                        state.blanks.iter().any(|e| e.blank_id == blank_id)
                    };
                    if !already_open {
                        let mut state = self.inner.lock().expect("host");
                        let id = self.platform.create_window(WindowKind::Blank);
                        state.blanks.push(BlankWindowEntry { id, blank_id });
                    }
                }
                WorkspaceGroupMember::Plugin {
                    plugin_id,
                    instance_id,
                } => {
                    let already_open = {
                        let state = self.inner.lock().expect("host");
                        state
                            .plugin_windows
                            .iter()
                            .any(|e| e.instance_id == instance_id)
                    };
                    if !already_open {
                        let _ = self.open_plugin_instance(&plugin_id, &instance_id, None)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Close currently open member windows for a Window Group (definition remains).
    pub fn close_window_group(&self, group_id: &str) -> Result<(), String> {
        let members = {
            let state = self.inner.lock().expect("host");
            let group = state
                .window_groups
                .iter()
                .find(|g| g.id == group_id)
                .ok_or_else(|| format!("unknown Window Group: {group_id}"))?;
            group.members.clone()
        };

        let mut state = self.inner.lock().expect("host");
        for member in members {
            match member {
                WorkspaceGroupMember::Blank { blank_id } => {
                    let mut kept = Vec::new();
                    for entry in state.blanks.drain(..) {
                        if entry.blank_id == blank_id {
                            self.platform.close_window(&entry.id);
                        } else {
                            kept.push(entry);
                        }
                    }
                    state.blanks = kept;
                }
                WorkspaceGroupMember::Plugin {
                    plugin_id: _,
                    instance_id,
                } => {
                    let mut kept = Vec::new();
                    for entry in state.plugin_windows.drain(..) {
                        if entry.instance_id == instance_id {
                            self.platform.close_window(&entry.id);
                        } else {
                            kept.push(entry);
                        }
                    }
                    state.plugin_windows = kept;
                }
            }
        }
        Self::prune_locked(&self.platform, &mut state);
        Ok(())
    }

    /// Current Workspace snapshot (layout + Plugin instance ids + Window Groups).
    pub fn capture_workspace(&self) -> Workspace {
        let mut state = self.inner.lock().expect("host");
        Self::prune_locked(&self.platform, &mut state);
        Self::capture_workspace_locked(&self.platform, &state)
    }

    fn capture_workspace_locked(platform: &Arc<dyn Platform>, state: &HostState) -> Workspace {
        let mut windows = Vec::new();
        for entry in &state.blanks {
            if let Some(geometry) = platform.window_geometry(&entry.id) {
                windows.push(WorkspaceWindow {
                    kind: WorkspaceWindowKind::Blank {
                        blank_id: entry.blank_id.clone(),
                    },
                    geometry,
                });
            }
        }
        for entry in &state.plugin_windows {
            if let Some(geometry) = platform.window_geometry(&entry.id) {
                windows.push(WorkspaceWindow {
                    kind: WorkspaceWindowKind::Plugin {
                        plugin_id: entry.plugin_id.clone(),
                        instance_id: entry.instance_id.clone(),
                    },
                    geometry,
                });
            }
        }
        let groups = state
            .window_groups
            .iter()
            .map(|g| WorkspaceGroup {
                id: g.id.clone(),
                name: g.name.clone(),
                members: g.members.clone(),
            })
            .collect();
        Workspace { windows, groups }
    }

    pub fn save_workspace(&self) -> Result<(), String> {
        let mut state = self.inner.lock().expect("host");
        Self::prune_locked(&self.platform, &mut state);
        let path = state
            .workspace_path
            .clone()
            .ok_or_else(|| "workspace path is not configured".to_string())?;
        let snapshot = Self::capture_workspace_locked(&self.platform, &state);
        drop(state);
        snapshot.save_to_path(&path)
    }

    /// Restore last Workspace. Corrupt/missing data is ignored; Host keeps running.
    pub fn restore_workspace(&self) -> Result<(), String> {
        let path = {
            let state = self.inner.lock().expect("host");
            state
                .workspace_path
                .clone()
                .ok_or_else(|| "workspace path is not configured".to_string())?
        };
        let Some(workspace) = Workspace::load_from_path(&path)? else {
            return Ok(());
        };
        self.apply_workspace(workspace)
    }

    fn apply_workspace(&self, workspace: Workspace) -> Result<(), String> {
        // Replace content windows so restore matches the last snapshot (not append).
        {
            let mut state = self.inner.lock().expect("host");
            for entry in state.blanks.drain(..) {
                self.platform.close_window(&entry.id);
            }
            for entry in state.plugin_windows.drain(..) {
                self.platform.close_window(&entry.id);
            }
            state.window_groups.clear();
            let sidecar_plugins: Vec<String> = state.sidecars.keys().cloned().collect();
            for plugin_id in sidecar_plugins {
                Self::stop_sidecar_locked(&self.platform, &mut state, &plugin_id);
            }
        }

        for window in workspace.windows {
            match window.kind {
                WorkspaceWindowKind::Blank { blank_id } => {
                    let blank_id = if blank_id.is_empty() {
                        uuid::Uuid::new_v4().to_string()
                    } else {
                        blank_id
                    };
                    let id = self.platform.create_window(WindowKind::Blank);
                    self.platform.set_window_geometry(&id, &window.geometry);
                    let mut state = self.inner.lock().expect("host");
                    state.blanks.push(BlankWindowEntry { id, blank_id });
                }
                WorkspaceWindowKind::Plugin {
                    plugin_id,
                    instance_id,
                } => {
                    let _ = self.open_plugin_instance(
                        &plugin_id,
                        &instance_id,
                        Some(window.geometry),
                    );
                }
            }
        }

        {
            let mut state = self.inner.lock().expect("host");
            state.window_groups = workspace
                .groups
                .into_iter()
                .map(|g| WindowGroupEntry {
                    id: g.id,
                    name: g.name,
                    members: g.members,
                })
                .collect();
        }
        Ok(())
    }

    fn open_plugin_instance(
        &self,
        plugin_id: &str,
        instance_id: &str,
        geometry: Option<WindowGeometry>,
    ) -> Result<Option<WindowId>, String> {
        let mut state = self.inner.lock().expect("host");
        Self::prune_locked(&self.platform, &mut state);
        let Some(manifest) = state.plugins.get(plugin_id).cloned() else {
            // Missing Plugin after restore — skip gracefully.
            return Ok(None);
        };

        if manifest.is_privileged() && !state.sidecars.contains_key(plugin_id) {
            let binary = resolve_sidecar_binary(&manifest)?;
            let sidecar_id = self.platform.spawn_sidecar(plugin_id);
            let sidecar_proxy = self.bus.proxy(
                plugin_id.to_string(),
                manifest.permissions.clone(),
                manifest.contracts.clone(),
            );
            drop(state);
            let child = sidecar_bridge::spawn_and_attach(&binary, sidecar_proxy).map_err(|e| {
                self.platform.stop_sidecar(&sidecar_id);
                e
            })?;
            state = self.inner.lock().expect("host");
            state
                .sidecars
                .insert(plugin_id.to_string(), (sidecar_id, child));
        }

        let ui_entry = manifest.ui_entry_path();
        let window = self
            .platform
            .create_pure_ui_window(plugin_id, instance_id, &ui_entry);
        if let Some(geometry) = geometry {
            self.platform.set_window_geometry(&window, &geometry);
        }
        state.plugin_windows.push(PluginWindowEntry {
            id: window.clone(),
            plugin_id: plugin_id.to_string(),
            instance_id: instance_id.to_string(),
        });
        Ok(Some(window))
    }

    /// Plugin instance identities currently open (plugin_id, instance_id).
    pub fn plugin_instance_ids(&self) -> Vec<(String, String)> {
        let mut state = self.inner.lock().expect("host");
        Self::prune_locked(&self.platform, &mut state);
        state
            .plugin_windows
            .iter()
            .map(|e| (e.plugin_id.clone(), e.instance_id.clone()))
            .collect()
    }

    /// Workspace instance id for a Plugin window label (identity pointer for Plugin-owned state).
    pub fn instance_id_for_window_label(&self, label: &str) -> Option<String> {
        let state = self.inner.lock().expect("host");
        state.plugin_windows.iter().find_map(|entry| {
            if entry.id.0 == label {
                Some(entry.instance_id.clone())
            } else {
                None
            }
        })
    }

    pub fn set_window_geometry_for_label(&self, label: &str, geometry: WindowGeometry) {
        self.platform
            .set_window_geometry(&WindowId(label.to_string()), &geometry);
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
            .retain(|entry| !platform.is_window_destroyed(&entry.id));
        state
            .plugin_windows
            .retain(|entry| !platform.is_window_destroyed(&entry.id));

        // Stop Sidecars whose Plugin windows are all gone.
        let active: std::collections::HashSet<String> = state
            .plugin_windows
            .iter()
            .map(|entry| entry.plugin_id.clone())
            .collect();
        let orphaned: Vec<String> = state
            .sidecars
            .keys()
            .filter(|id| !active.contains(*id))
            .cloned()
            .collect();
        for plugin_id in orphaned {
            Self::stop_sidecar_locked(platform, state, &plugin_id);
        }
    }

    fn stop_sidecar_locked(
        platform: &Arc<dyn Platform>,
        state: &mut HostState,
        plugin_id: &str,
    ) {
        if let Some((sidecar_id, mut child)) = state.sidecars.remove(plugin_id) {
            let _ = child.kill();
            let _ = child.wait();
            platform.stop_sidecar(&sidecar_id);
        }
    }
}

fn grants_file(plugins_dir: &Path) -> PathBuf {
    plugins_dir.join(".spacecraft-grants.json")
}

fn load_grants(plugins_dir: &Path) -> HashMap<String, Vec<PermissionItem>> {
    let path = grants_file(plugins_dir);
    let Ok(raw) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn persist_grants(
    plugins_dir: &Path,
    plugin_id: &str,
    grants: &[PermissionItem],
) -> Result<(), String> {
    let mut all = load_grants(plugins_dir);
    all.insert(plugin_id.to_string(), grants.to_vec());
    let path = grants_file(plugins_dir);
    let raw = serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?;
    std::fs::write(path, raw).map_err(|e| e.to_string())
}

fn resolve_sidecar_binary(manifest: &Manifest) -> Result<PathBuf, String> {
    let name = manifest
        .sidecar
        .as_ref()
        .ok_or_else(|| "privileged Plugin missing Sidecar entry".to_string())?;

    if let Some(path) = option_env!("CARGO_BIN_EXE_echo-sidecar") {
        if name == "echo-sidecar" || manifest.id == "echo" {
            return Ok(PathBuf::from(path));
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
            #[cfg(windows)]
            {
                let candidate = dir.join(format!("{name}.exe"));
                if candidate.exists() {
                    return Ok(candidate);
                }
            }
        }
    }

    let debug = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/debug")
        .join(name);
    if debug.exists() {
        return Ok(debug);
    }

    if let Some(path) = manifest.sidecar_entry_path() {
        if path.exists() {
            return Ok(path);
        }
    }

    Err(format!(
        "Sidecar binary `{name}` not found for Plugin `{}`",
        manifest.id
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_platform::MemoryPlatform;
    use serde_json::json;
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

    fn wait_for_value(
        slot: &Arc<Mutex<Option<serde_json::Value>>>,
        timeout_ms: u64,
    ) -> Option<serde_json::Value> {
        let start = std::time::Instant::now();
        loop {
            if let Some(value) = slot.lock().expect("lock").clone() {
                return Some(value);
            }
            if start.elapsed().as_millis() as u64 >= timeout_ms {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
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
    fn host_lists_hello_and_echo_plugins_from_plugins_directory() {
        let (host, _) = boot();
        host.load_plugins_from(&fixture_plugins_dir());
        let listed = host.listed_plugins();
        assert!(listed.iter().any(|p| p.id == "hello" && p.name == "Hello"));
        assert!(listed.iter().any(|p| p.id == "echo" && p.name == "Echo"));
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
        assert!(!host.sidecar_running_for("hello"));
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

    #[test]
    fn privileged_echo_plugin_spawns_one_sidecar_tied_to_plugin() {
        let (host, platform) = boot();
        host.load_plugins_from(&fixture_plugins_dir());
        host.open_plugin("echo").expect("open echo");
        assert!(host.sidecar_running_for("echo"));
        assert_eq!(platform.running_sidecar_count_for("echo"), 1);

        host.open_plugin("echo").expect("open again");
        assert_eq!(
            platform.running_sidecar_count_for("echo"),
            1,
            "one Sidecar per privileged Plugin"
        );

        host.close_plugin_windows("echo");
        assert!(!host.sidecar_running_for("echo"));
        assert_eq!(platform.running_sidecar_count_for("echo"), 0);
    }

    #[test]
    fn pub_sub_round_trip_through_host_scoped_proxies() {
        let (host, _) = boot();
        host.load_plugins_from(&fixture_plugins_dir());
        host.open_plugin("echo").expect("open echo");

        let received = Arc::new(Mutex::new(None));
        let ui = host.scoped_bus("echo").expect("ui proxy");
        let r = Arc::clone(&received);
        ui.subscribe("echo.pong", move |payload| {
            *r.lock().expect("lock") = Some(payload);
        })
        .expect("subscribe");

        // Sidecar process answers ping asynchronously over stdio.
        ui.emit("echo.ping", json!({ "message": "hi" }))
            .expect("emit");
        assert_eq!(
            wait_for_value(&received, 1000),
            Some(json!({ "message": "hi" }))
        );
    }

    #[test]
    fn request_response_round_trip_with_validated_contract() {
        let (host, _) = boot();
        host.load_plugins_from(&fixture_plugins_dir());
        host.open_plugin("echo").expect("open echo");

        let ui = host.scoped_bus("echo").expect("ui proxy");
        let result = ui
            .call("echo.reflect", json!({ "value": "round-trip" }))
            .expect("call");
        assert_eq!(result, json!({ "value": "round-trip" }));
    }

    #[test]
    fn undeclared_bus_traffic_is_rejected() {
        let (host, _) = boot();
        host.load_plugins_from(&fixture_plugins_dir());
        host.open_plugin("echo").expect("open echo");

        let ui = host.scoped_bus("echo").expect("ui proxy");
        let emit_err = ui
            .emit("echo.forbidden", json!({ "message": "x" }))
            .expect_err("deny emit");
        assert!(matches!(
            emit_err,
            BusError::PermissionDenied { action: "emit", .. }
        ));

        let sub_err = ui
            .subscribe("echo.forbidden", |_| {})
            .expect_err("deny subscribe");
        assert!(matches!(
            sub_err,
            BusError::PermissionDenied {
                action: "subscribe",
                ..
            }
        ));

        let call_err = ui
            .call("echo.forbidden", json!({ "value": "x" }))
            .expect_err("deny call");
        assert!(matches!(
            call_err,
            BusError::PermissionDenied { action: "call", .. }
        ));
    }

    #[test]
    fn invalid_contract_payload_is_rejected() {
        let (host, _) = boot();
        host.load_plugins_from(&fixture_plugins_dir());
        host.open_plugin("echo").expect("open echo");

        let ui = host.scoped_bus("echo").expect("ui proxy");
        let err = ui
            .emit("echo.ping", json!({ "nope": true }))
            .expect_err("contract");
        assert!(matches!(err, BusError::ContractViolation(_)));
    }

    #[test]
    fn window_bus_commands_use_scoped_proxy_not_global_bus() {
        let (host, _) = boot();
        host.load_plugins_from(&fixture_plugins_dir());
        host.open_plugin("echo").expect("open echo");

        let label = host
            .open_windows()
            .into_iter()
            .find_map(|(id, kind)| match kind {
                WindowKind::PureUi { plugin_id } if plugin_id == "echo" => Some(id),
                _ => None,
            })
            .expect("echo window");

        let received = Arc::new(Mutex::new(None));
        let ui = host.scoped_bus("echo").expect("listener");
        let r = Arc::clone(&received);
        ui.subscribe("echo.pong", move |payload| {
            *r.lock().expect("lock") = Some(payload);
        })
        .expect("subscribe");

        host.bus_emit_from_window(&label, "echo.ping", json!({ "message": "scoped" }))
            .expect("window emit");
        assert_eq!(
            wait_for_value(&received, 1000),
            Some(json!({ "message": "scoped" }))
        );

        let reflected = host
            .bus_call_from_window(&label, "echo.reflect", json!({ "value": "via-window" }))
            .expect("window call");
        assert_eq!(reflected, json!({ "value": "via-window" }));

        let denied = host.bus_emit_from_window(
            &label,
            "echo.forbidden",
            json!({ "message": "nope" }),
        );
        assert!(denied.is_err());
    }

    #[test]
    fn pure_ui_hello_has_no_bus_permissions() {
        let (host, _) = boot();
        host.load_plugins_from(&fixture_plugins_dir());
        host.open_plugin("hello").expect("open hello");
        let ui = host.scoped_bus("hello").expect("proxy");
        let err = ui
            .emit("anything", json!({}))
            .expect_err("hello has empty permissions");
        assert!(matches!(err, BusError::PermissionDenied { .. }));
    }

    fn notes_package_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/packages/notes")
    }

    fn install_plugins_dir() -> PathBuf {
        let dir = temp_plugins_dir();
        dir
    }

    #[test]
    fn install_from_folder_requires_confirm_before_listing() {
        let plugins_dir = install_plugins_dir();
        let (host, _) = boot();
        host.load_plugins_from(&plugins_dir);
        assert!(!host.listed_plugins().iter().any(|p| p.id == "notes"));

        let proposal = host
            .propose_install_from_folder(&notes_package_dir())
            .expect("propose");
        assert_eq!(proposal.plugin_id, "notes");
        assert!(proposal.signature_present);
        assert!(!proposal.permissions.is_empty());
        assert!(proposal.permissions.iter().all(|p| p.sensitive));
        assert!(!host.listed_plugins().iter().any(|p| p.id == "notes"));

        let listed = host
            .confirm_install(&proposal.proposal_id)
            .expect("confirm");
        assert_eq!(listed.id, "notes");
        assert!(host.listed_plugins().iter().any(|p| p.id == "notes"));
        assert!(host.confirmed_grants_for("notes").is_some());
        assert!(plugins_dir.join("notes/manifest.json").is_file());
        assert!(plugins_dir.join(".spacecraft-grants.json").is_file());

        // Grants survive Host reload from the same plugins dir.
        let (host2, _) = boot();
        host2.load_plugins_from(&plugins_dir);
        assert!(host2.confirmed_grants_for("notes").is_some());
        let _ = fs::remove_dir_all(&plugins_dir);
    }

    #[test]
    fn install_from_zip_works_after_confirm() {
        let plugins_dir = install_plugins_dir();
        let zip_path = std::env::temp_dir().join(format!(
            "notes-{}.zip",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        crate::install::write_zip_from_dir(&notes_package_dir(), &zip_path).expect("zip");

        let (host, _) = boot();
        host.load_plugins_from(&plugins_dir);
        let proposal = host.propose_install_from_zip(&zip_path).expect("propose");
        assert_eq!(proposal.source_kind, "zip");
        assert_eq!(proposal.plugin_id, "notes");

        host.confirm_install(&proposal.proposal_id).expect("confirm");
        assert!(host.listed_plugins().iter().any(|p| p.id == "notes"));
        let _ = fs::remove_file(&zip_path);
        let _ = fs::remove_dir_all(&plugins_dir);
    }

    #[test]
    fn declined_install_leaves_workbench_unchanged() {
        let plugins_dir = install_plugins_dir();
        let (host, _) = boot();
        host.load_plugins_from(&plugins_dir);
        let before = host.listed_plugins();

        let proposal = host
            .propose_install_from_folder(&notes_package_dir())
            .expect("propose");
        host.decline_install(&proposal.proposal_id)
            .expect("decline");

        assert_eq!(host.listed_plugins(), before);
        assert!(host.pending_install().is_none());
        assert!(!plugins_dir.join("notes").exists());
        assert!(host.confirmed_grants_for("notes").is_none());
        let _ = fs::remove_dir_all(&plugins_dir);
    }

    #[test]
    fn reserved_signature_field_is_accepted_without_verification() {
        let manifest = Manifest::load_from_plugin_dir(&notes_package_dir()).expect("load");
        assert_eq!(
            manifest.signature.as_deref(),
            Some("reserved-not-verified")
        );
    }

    fn workspace_temp_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "spacecraft-workspace-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }

    #[test]
    fn stop_saves_open_window_layout_and_plugin_instance_ids() {
        let path = workspace_temp_path();
        let _ = fs::remove_file(&path);

        let (host, _) = boot();
        host.set_workspace_path(path.clone());
        host.load_plugins_from(&fixture_plugins_dir());
        host.open_blank_window();
        host.open_plugin("hello").expect("open hello");

        let before = host.plugin_instance_ids();
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].0, "hello");

        let hello_label = host
            .open_windows()
            .into_iter()
            .find_map(|(id, k)| match k {
                WindowKind::PureUi { plugin_id } if plugin_id == "hello" => Some(id),
                _ => None,
            })
            .expect("hello window");
        host.set_window_geometry_for_label(
            &hello_label,
            WindowGeometry {
                x: 120.0,
                y: 140.0,
                width: 640.0,
                height: 480.0,
            },
        );

        host.stop();
        assert!(path.is_file());

        let saved = Workspace::load_from_path(&path)
            .expect("load")
            .expect("workspace present");
        assert!(saved.windows.iter().any(|w| {
            matches!(&w.kind, WorkspaceWindowKind::Blank { .. })
        }));
        let plugin = saved
            .windows
            .iter()
            .find_map(|w| match &w.kind {
                WorkspaceWindowKind::Plugin {
                    plugin_id,
                    instance_id,
                } if plugin_id == "hello" => Some((instance_id.clone(), w.geometry)),
                _ => None,
            })
            .expect("hello in workspace");
        assert_eq!(plugin.0, before[0].1);
        assert_eq!(
            plugin.1,
            WindowGeometry {
                x: 120.0,
                y: 140.0,
                width: 640.0,
                height: 480.0,
            }
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn restore_reopens_plugins_with_prior_geometry_and_instance_ids() {
        let path = workspace_temp_path();
        let _ = fs::remove_file(&path);

        let (host, _) = boot();
        host.set_workspace_path(path.clone());
        host.load_plugins_from(&fixture_plugins_dir());
        host.open_plugin("hello").expect("open hello");
        let instances = host.plugin_instance_ids();
        let instance_id = instances[0].1.clone();
        let hello_label = host
            .open_windows()
            .into_iter()
            .find_map(|(id, k)| match k {
                WindowKind::PureUi { plugin_id } if plugin_id == "hello" => Some(id),
                _ => None,
            })
            .expect("hello window");
        let geometry = WindowGeometry {
            x: 200.0,
            y: 220.0,
            width: 900.0,
            height: 700.0,
        };
        host.set_window_geometry_for_label(&hello_label, geometry);
        host.save_workspace().expect("save");
        host.stop();

        let (host2, platform2) = boot();
        host2.set_workspace_path(path.clone());
        host2.load_plugins_from(&fixture_plugins_dir());
        host2.restore_workspace().expect("restore");

        assert_eq!(host2.plugin_instance_ids(), vec![("hello".into(), instance_id.clone())]);
        let restored_label = host2
            .open_windows()
            .into_iter()
            .find_map(|(id, k)| match k {
                WindowKind::PureUi { plugin_id } if plugin_id == "hello" => Some(id),
                _ => None,
            })
            .expect("restored hello");
        assert_eq!(
            platform2.window_geometry(&WindowId(restored_label.clone())),
            Some(geometry)
        );
        assert_eq!(
            platform2.instance_id_for(&WindowId(restored_label.clone())),
            Some(instance_id.clone())
        );
        assert_eq!(
            host2.instance_id_for_window_label(&restored_label),
            Some(instance_id)
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn workspace_snapshot_does_not_include_plugin_business_state() {
        let (host, _) = boot();
        host.load_plugins_from(&fixture_plugins_dir());
        host.open_plugin("hello").expect("open hello");
        let snapshot = host.capture_workspace();
        let raw = serde_json::to_value(&snapshot).expect("json");
        let obj = raw.as_object().expect("object");
        let mut keys: Vec<_> = obj.keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, vec!["groups".to_string(), "windows".to_string()]);
        assert!(raw["groups"].as_array().expect("groups").is_empty());
        for window in raw["windows"].as_array().expect("windows") {
            let keys: Vec<_> = window.as_object().expect("window").keys().cloned().collect();
            assert!(keys.contains(&"kind".to_string()));
            assert!(keys.contains(&"geometry".to_string()));
            assert_eq!(keys.len(), 2);
            let kind = &window["kind"];
            match kind["type"].as_str() {
                Some("blank") => {
                    let kind_obj = kind.as_object().expect("kind");
                    assert!(kind_obj.contains_key("type"));
                    assert!(kind_obj.contains_key("blankId"));
                    assert_eq!(kind_obj.len(), 2);
                }
                Some("plugin") => {
                    let kind_keys: Vec<_> =
                        kind.as_object().expect("kind").keys().cloned().collect();
                    assert!(kind_keys.contains(&"type".to_string()));
                    assert!(kind_keys.contains(&"pluginId".to_string()));
                    assert!(kind_keys.contains(&"instanceId".to_string()));
                    assert_eq!(kind_keys.len(), 3);
                }
                other => panic!("unexpected kind {other:?}"),
            }
        }
    }

    #[test]
    fn corrupt_workspace_still_allows_host_boot() {
        let path = workspace_temp_path();
        fs::write(&path, "{ this is not valid workspace json").expect("write corrupt");

        let (host, _) = boot();
        host.set_workspace_path(path.clone());
        host.load_plugins_from(&fixture_plugins_dir());
        host.restore_workspace().expect("restore soft-fails");
        assert!(host.is_running());
        assert!(host.is_tray_visible());
        assert!(host.open_windows().is_empty());
        // Host can still open Plugins after a bad Workspace file.
        host.open_plugin("hello").expect("open hello");
        assert!(host
            .open_windows()
            .iter()
            .any(|(_, k)| matches!(k, WindowKind::PureUi { plugin_id } if plugin_id == "hello")));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn create_named_window_group_from_open_windows() {
        let (host, _) = boot();
        host.load_plugins_from(&fixture_plugins_dir());
        host.open_plugin("hello").expect("open hello");
        host.open_blank_window();
        let labels: Vec<String> = host
            .list_content_windows()
            .into_iter()
            .map(|w| w.label)
            .collect();
        assert_eq!(labels.len(), 2);

        let group = host
            .create_window_group("Stack", &labels)
            .expect("create group");
        assert_eq!(group.name, "Stack");
        assert_eq!(group.member_count, 2);
        assert_eq!(host.list_window_groups().len(), 1);
    }

    #[test]
    fn opening_and_closing_window_group_toggles_member_windows() {
        let (host, _) = boot();
        host.load_plugins_from(&fixture_plugins_dir());
        let group = host
            .open_window_group_declared("Demo", &["hello".into(), "echo".into()])
            .expect("declare");
        assert_eq!(host.plugin_instance_ids().len(), 2);

        host.close_window_group(&group.id).expect("close");
        assert!(host.plugin_instance_ids().is_empty());
        assert_eq!(host.list_window_groups().len(), 1); // definition remains

        host.open_window_group(&group.id).expect("reopen");
        let open = host.plugin_instance_ids();
        assert_eq!(open.len(), 2);
        assert!(open.iter().any(|(id, _)| id == "hello"));
        assert!(open.iter().any(|(id, _)| id == "echo"));
    }

    #[test]
    fn window_groups_restore_with_workspace() {
        let path = workspace_temp_path();
        let _ = fs::remove_file(&path);

        let (host, _) = boot();
        host.set_workspace_path(path.clone());
        host.load_plugins_from(&fixture_plugins_dir());
        let group = host
            .open_window_group_declared("Persist", &["hello".into()])
            .expect("declare");
        let instances = host.plugin_instance_ids();
        host.save_workspace().expect("save");
        host.stop();

        let (host2, _) = boot();
        host2.set_workspace_path(path.clone());
        host2.load_plugins_from(&fixture_plugins_dir());
        host2.restore_workspace().expect("restore");

        let groups = host2.list_window_groups();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, group.id);
        assert_eq!(groups[0].name, "Persist");
        assert_eq!(host2.plugin_instance_ids(), instances);

        // Ungrouped windows still work.
        host2.open_blank_window();
        assert!(host2
            .open_windows()
            .iter()
            .any(|(_, k)| *k == WindowKind::Blank));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn ungrouped_windows_unaffected_by_other_groups() {
        let (host, _) = boot();
        host.load_plugins_from(&fixture_plugins_dir());
        host.open_plugin("hello").expect("hello");
        let solo = host.plugin_instance_ids()[0].1.clone();
        let group = host
            .open_window_group_declared("OnlyEcho", &["echo".into()])
            .expect("group");
        host.close_window_group(&group.id).expect("close group");

        assert_eq!(
            host.plugin_instance_ids(),
            vec![("hello".into(), solo)]
        );
    }
}
