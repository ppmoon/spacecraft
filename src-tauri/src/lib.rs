mod bus;
mod host;
mod install;
mod manifest;
mod memory_platform;
mod platform;
mod sidecar_bridge;
mod tauri_platform;
mod workspace;

pub use host::{Host, ListedPlugin};
pub use install::InstallProposal;
pub use memory_platform::MemoryPlatform;
pub use platform::{Platform, TrayId, WindowId, WindowKind};

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;
use tauri::{Emitter, RunEvent, WebviewWindow};

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

#[tauri::command]
fn propose_install(path: String) -> Result<InstallProposal, String> {
    let source = PathBuf::from(&path);
    with_host(|h| {
        if source
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
        {
            h.propose_install_from_zip(&source)
        } else {
            h.propose_install_from_folder(&source)
        }
    })
}

#[tauri::command]
fn confirm_install(proposal_id: String) -> Result<ListedPlugin, String> {
    with_host(|h| h.confirm_install(&proposal_id))
}

#[tauri::command]
fn decline_install(proposal_id: String) -> Result<(), String> {
    with_host(|h| h.decline_install(&proposal_id))
}

#[tauri::command]
fn pending_install() -> Option<InstallProposal> {
    with_host(|h| h.pending_install())
}

/// Scoped Bus emit — window identity selects the Plugin; no raw global Bus.
#[tauri::command]
fn bus_emit(window: WebviewWindow, topic: String, payload: Value) -> Result<(), String> {
    with_host(|h| h.bus_emit_from_window(window.label(), &topic, payload))
}

#[tauri::command]
fn bus_call(window: WebviewWindow, topic: String, payload: Value) -> Result<Value, String> {
    with_host(|h| h.bus_call_from_window(window.label(), &topic, payload))
}

/// Subscribe via scoped proxy; events are pushed to this window only as `bus://event`.
#[tauri::command]
fn bus_subscribe(window: WebviewWindow, topic: String) -> Result<(), String> {
    let label = window.label().to_string();
    let host = current_host().ok_or_else(|| "Host is not started".to_string())?;
    let plugin_id = host
        .plugin_id_for_window_label(&label)
        .ok_or_else(|| "window is not a Plugin surface".to_string())?;
    let proxy = host.scoped_bus(&plugin_id).map_err(|e| e.to_string())?;
    let win = window.clone();
    let topic_for_event = topic.clone();
    proxy
        .subscribe(&topic, move |payload| {
            let envelope = serde_json::json!({
                "topic": topic_for_event,
                "payload": payload,
            });
            let _ = win.emit("bus://event", envelope);
        })
        .map_err(|e| e.to_string())?;
    Ok(())
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
            open_plugin,
            propose_install,
            confirm_install,
            decline_install,
            pending_install,
            bus_emit,
            bus_call,
            bus_subscribe
        ])
        .setup(move |app| {
            let plugins_dir = default_plugins_dir();
            let workspace_path = plugins_dir.join(".spacecraft-workspace.json");
            let platform = Arc::new(tauri_platform::TauriPlatform::new(app.handle().clone()));
            let host = Arc::new(Host::new(platform));
            host.set_workspace_path(workspace_path);
            host.load_plugins_from(&plugins_dir);
            host.start();
            if let Err(e) = host.restore_workspace() {
                eprintln!("spacecraft: workspace restore skipped: {e}");
            }
            *host_slot().lock().expect("host slot") = Some(Arc::clone(&host));

            if smoke {
                host.open_launcher();
                host.open_command_palette();
                host.open_blank_window();
                let _ = host.open_plugin("hello");
                let _ = host.open_plugin("echo");
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
