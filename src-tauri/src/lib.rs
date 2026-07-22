mod host;
mod manifest;
mod memory_platform;
mod platform;
mod tauri_platform;

pub use host::{Host, ListedPlugin};
pub use memory_platform::MemoryPlatform;
pub use platform::{Platform, TrayId, WindowId, WindowKind};

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use tauri::RunEvent;

static HOST: OnceLock<Mutex<Option<Arc<Host>>>> = OnceLock::new();

fn host_slot() -> &'static Mutex<Option<Arc<Host>>> {
    HOST.get_or_init(|| Mutex::new(None))
}

fn with_host<F, R>(f: F) -> R
where
    F: FnOnce(&Arc<Host>) -> R,
{
    let guard = host_slot().lock().expect("host slot");
    let host = guard.as_ref().expect("Host is not started");
    f(host)
}

pub(crate) fn current_host() -> Option<Arc<Host>> {
    host_slot().lock().expect("host slot").clone()
}

fn default_plugins_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("SPACECRAFT_PLUGINS_DIR") {
        return PathBuf::from(dir);
    }
    // Dev / repo layout: <repo>/plugins next to src-tauri
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins")
}

#[tauri::command]
fn open_blank_window() {
    with_host(|h| h.open_blank_window());
}

#[tauri::command]
fn close_launcher() {
    with_host(|h| h.close_launcher());
}

#[tauri::command]
fn close_command_palette() {
    with_host(|h| h.close_command_palette());
}

#[tauri::command]
fn list_plugins() -> Vec<ListedPlugin> {
    with_host(|h| h.listed_plugins())
}

#[tauri::command]
fn open_plugin(id: String) -> Result<(), String> {
    with_host(|h| h.open_plugin(&id))
}

pub fn run() {
    let smoke = std::env::var("SPACECRAFT_SMOKE").ok().as_deref() == Some("1");

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            open_blank_window,
            close_launcher,
            close_command_palette,
            list_plugins,
            open_plugin
        ])
        .setup(move |app| {
            let platform = Arc::new(tauri_platform::TauriPlatform::new(app.handle().clone()));
            let host = Arc::new(Host::new(platform));
            host.load_plugins_from(&default_plugins_dir());
            host.start();
            *host_slot().lock().expect("host slot") = Some(Arc::clone(&host));

            if smoke {
                host.open_launcher();
                host.open_command_palette();
                host.open_blank_window();
                let _ = host.open_plugin("hello");
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    if let Some(h) = host_slot().lock().expect("host slot").as_ref() {
                        h.stop();
                    }
                    handle.exit(0);
                });
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Spacecraft")
        .run(|app_handle, event| {
            if let RunEvent::ExitRequested { .. } = event {
                if let Some(h) = host_slot().lock().expect("host slot").as_ref() {
                    h.stop();
                }
                let _ = app_handle;
            }
        });
}
