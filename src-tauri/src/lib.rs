mod host;
mod memory_platform;
mod platform;
mod tauri_platform;

pub use host::Host;
pub use memory_platform::MemoryPlatform;
pub use platform::{Platform, TrayId, WindowId, WindowKind};

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let smoke = std::env::var("SPACECRAFT_SMOKE").ok().as_deref() == Some("1");

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            open_blank_window,
            close_launcher,
            close_command_palette
        ])
        .setup(move |app| {
            let platform = Arc::new(tauri_platform::TauriPlatform::new(app.handle().clone()));
            let host = Arc::new(Host::new(platform));
            host.start();
            *host_slot().lock().expect("host slot") = Some(Arc::clone(&host));

            if smoke {
                host.open_launcher();
                host.open_command_palette();
                host.open_blank_window();
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
