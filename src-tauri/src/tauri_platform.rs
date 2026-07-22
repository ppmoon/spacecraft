//! Tauri-backed Platform — tray, windows, global shortcuts.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::platform::{Platform, SidecarId, TrayId, WindowId, WindowKind};

pub struct TauriPlatform {
    app: AppHandle,
    state: Mutex<TauriPlatformState>,
}

struct TauriPlatformState {
    next_id: u64,
    shortcuts: HashMap<String, Shortcut>,
    trays: HashMap<String, TrayIcon>,
    privileged: HashMap<String, bool>,
    sidecars: HashMap<String, ()>,
}

impl TauriPlatform {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            state: Mutex::new(TauriPlatformState {
                next_id: 0,
                shortcuts: HashMap::new(),
                trays: HashMap::new(),
                privileged: HashMap::new(),
                sidecars: HashMap::new(),
            }),
        }
    }

    fn page_for(kind: &WindowKind) -> &'static str {
        match kind {
            WindowKind::Launcher => "launcher.html",
            WindowKind::Palette => "palette.html",
            WindowKind::Blank => "blank.html",
            WindowKind::PureUi { .. } => unreachable!("Pure-UI uses create_pure_ui_window"),
        }
    }

    fn size_for(kind: &WindowKind) -> (f64, f64) {
        match kind {
            WindowKind::Palette => (560.0, 280.0),
            WindowKind::Launcher => (420.0, 320.0),
            WindowKind::Blank | WindowKind::PureUi { .. } => (800.0, 600.0),
        }
    }

    fn title_for(kind: &WindowKind) -> String {
        match kind {
            WindowKind::Launcher => "Launcher".into(),
            WindowKind::Palette => "Command palette".into(),
            WindowKind::Blank => "Blank window".into(),
            WindowKind::PureUi { plugin_id } => plugin_id.clone(),
        }
    }
}

impl Platform for TauriPlatform {
    fn create_tray(&self, on_quit: Box<dyn Fn() + Send + Sync>) -> TrayId {
        let mut state = self.state.lock().expect("tauri platform");
        state.next_id += 1;
        let id = format!("tray-{}", state.next_id);
        drop(state);

        let on_quit = Arc::new(on_quit);

        let quit_item =
            MenuItem::with_id(&self.app, "quit", "Quit", true, None::<&str>).expect("quit item");
        let launcher_item = MenuItem::with_id(
            &self.app,
            "open-launcher",
            "Open Launcher",
            true,
            None::<&str>,
        )
        .expect("launcher item");
        let palette_item = MenuItem::with_id(
            &self.app,
            "command-palette",
            "Command Palette",
            true,
            None::<&str>,
        )
        .expect("palette item");

        let menu = Menu::with_items(&self.app, &[&launcher_item, &palette_item, &quit_item])
            .expect("tray menu");

        let on_quit_menu = Arc::clone(&on_quit);
        let tray = TrayIconBuilder::new()
            .icon(self.app.default_window_icon().expect("app icon").clone())
            .menu(&menu)
            .tooltip("Spacecraft")
            .on_menu_event(move |_app, event| match event.id.as_ref() {
                "quit" => on_quit_menu(),
                "open-launcher" => {
                    if let Some(host) = crate::current_host() {
                        host.open_launcher();
                    }
                }
                "command-palette" => {
                    if let Some(host) = crate::current_host() {
                        host.open_command_palette();
                    }
                }
                _ => {}
            })
            .build(&self.app)
            .expect("tray");

        let mut state = self.state.lock().expect("tauri platform");
        state.trays.insert(id.clone(), tray);
        TrayId(id)
    }

    fn destroy_tray(&self, id: &TrayId) {
        let mut state = self.state.lock().expect("tauri platform");
        state.trays.remove(&id.0);
    }

    fn create_window(&self, kind: WindowKind) -> WindowId {
        let mut state = self.state.lock().expect("tauri platform");
        state.next_id += 1;
        let label = format!("win-{}", state.next_id);
        state.privileged.insert(label.clone(), true);
        drop(state);

        let (width, height) = Self::size_for(&kind);
        let url = WebviewUrl::App(Self::page_for(&kind).into());
        let mut builder = WebviewWindowBuilder::new(&self.app, &label, url)
            .title(Self::title_for(&kind))
            .inner_size(width, height)
            .resizable(matches!(kind, WindowKind::Blank));

        if matches!(kind, WindowKind::Palette | WindowKind::Launcher) {
            builder = builder.always_on_top(true);
        }
        if matches!(kind, WindowKind::Palette) {
            builder = builder.decorations(false);
        }

        builder.build().expect("create window");
        WindowId(label)
    }

    fn create_pure_ui_window(&self, plugin_id: &str, ui_entry: &Path) -> WindowId {
        let mut state = self.state.lock().expect("tauri platform");
        state.next_id += 1;
        let label = format!("plugin-{}-{}", plugin_id, state.next_id);
        state.privileged.insert(label.clone(), false);
        drop(state);

        let file_url = url::Url::from_file_path(ui_entry)
            .unwrap_or_else(|_| panic!("ui entry is not a valid file path: {ui_entry:?}"));
        let kind = WindowKind::PureUi {
            plugin_id: plugin_id.to_string(),
        };
        let (width, height) = Self::size_for(&kind);

        WebviewWindowBuilder::new(&self.app, &label, WebviewUrl::External(file_url))
            .title(Self::title_for(&kind))
            .inner_size(width, height)
            .resizable(true)
            .build()
            .expect("create Pure-UI window");

        WindowId(label)
    }

    fn close_window(&self, id: &WindowId) {
        if let Some(win) = self.app.get_webview_window(&id.0) {
            let _ = win.close();
        }
        let mut state = self.state.lock().expect("tauri platform");
        state.privileged.remove(&id.0);
    }

    fn is_window_destroyed(&self, id: &WindowId) -> bool {
        self.app.get_webview_window(&id.0).is_none()
    }

    fn window_allows_privileged_apis(&self, id: &WindowId) -> bool {
        let state = self.state.lock().expect("tauri platform");
        state.privileged.get(&id.0).copied().unwrap_or(false)
    }

    fn spawn_sidecar(&self, plugin_id: &str) -> SidecarId {
        let mut state = self.state.lock().expect("tauri platform");
        state.next_id += 1;
        let id = format!("sidecar-{}-{}", plugin_id, state.next_id);
        // Lifecycle bookkeeping only — Host owns the out-of-process Child via sidecar_bridge.
        state.sidecars.insert(id.clone(), ());
        SidecarId(id)
    }

    fn stop_sidecar(&self, id: &SidecarId) {
        let mut state = self.state.lock().expect("tauri platform");
        state.sidecars.remove(&id.0);
    }

    fn is_sidecar_running(&self, id: &SidecarId) -> bool {
        let state = self.state.lock().expect("tauri platform");
        state.sidecars.contains_key(&id.0)
    }

    fn register_shortcut(&self, accelerator: &str, handler: Box<dyn Fn() + Send + Sync>) {
        let shortcut: Shortcut = accelerator
            .parse()
            .unwrap_or_else(|_| panic!("invalid shortcut: {accelerator}"));
        let handler = Arc::new(handler);
        self.app
            .global_shortcut()
            .on_shortcut(shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    handler();
                }
            })
            .expect("register shortcut");

        let mut state = self.state.lock().expect("tauri platform");
        state.shortcuts.insert(accelerator.to_string(), shortcut);
    }

    fn unregister_all_shortcuts(&self) {
        let mut state = self.state.lock().expect("tauri platform");
        let shortcuts: Vec<Shortcut> = state.shortcuts.values().copied().collect();
        state.shortcuts.clear();
        drop(state);
        for shortcut in shortcuts {
            let _ = self.app.global_shortcut().unregister(shortcut);
        }
    }

    fn quit(&self) {
        self.app.exit(0);
    }
}
