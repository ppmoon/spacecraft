//! Tauri-backed Platform — tray, windows, global shortcuts.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::platform::{Platform, TrayId, WindowId, WindowKind};

pub struct TauriPlatform {
    app: AppHandle,
    state: Mutex<TauriPlatformState>,
}

struct TauriPlatformState {
    next_id: u64,
    shortcuts: HashMap<String, Shortcut>,
}

impl TauriPlatform {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            state: Mutex::new(TauriPlatformState {
                next_id: 0,
                shortcuts: HashMap::new(),
            }),
        }
    }

    fn page_for(kind: WindowKind) -> &'static str {
        match kind {
            WindowKind::Launcher => "launcher.html",
            WindowKind::Palette => "palette.html",
            WindowKind::Blank => "blank.html",
        }
    }

    fn size_for(kind: WindowKind) -> (f64, f64) {
        match kind {
            WindowKind::Palette => (560.0, 280.0),
            WindowKind::Launcher => (420.0, 240.0),
            WindowKind::Blank => (800.0, 600.0),
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
        let _tray = TrayIconBuilder::new()
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

        TrayId(id)
    }

    fn destroy_tray(&self, _id: &TrayId) {
        // Tray icon is tied to app lifetime for Phase 1.01; Host stop still clears state.
    }

    fn create_window(&self, kind: WindowKind) -> WindowId {
        let mut state = self.state.lock().expect("tauri platform");
        state.next_id += 1;
        let label = format!("win-{}", state.next_id);
        drop(state);

        let (width, height) = Self::size_for(kind);
        let url = WebviewUrl::App(Self::page_for(kind).into());
        let mut builder = WebviewWindowBuilder::new(&self.app, &label, url)
            .title(match kind {
                WindowKind::Launcher => "Launcher",
                WindowKind::Palette => "Command palette",
                WindowKind::Blank => "Blank window",
            })
            .inner_size(width, height)
            .resizable(kind == WindowKind::Blank);

        if kind == WindowKind::Palette || kind == WindowKind::Launcher {
            builder = builder.always_on_top(true);
        }
        if kind == WindowKind::Palette {
            builder = builder.decorations(false);
        }

        builder.build().expect("create window");
        WindowId(label)
    }

    fn close_window(&self, id: &WindowId) {
        if let Some(win) = self.app.get_webview_window(&id.0) {
            let _ = win.close();
        }
    }

    fn is_window_destroyed(&self, id: &WindowId) -> bool {
        self.app.get_webview_window(&id.0).is_none()
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
