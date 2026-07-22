//! Bridge between a Sidecar child process (stdio NDJSON) and the Host Bus.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::bus::BusProxy;

#[derive(Debug, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum HostToSidecar {
    Event { topic: String, payload: Value },
    Call { id: String, topic: String, payload: Value },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum SidecarToHost {
    Ready,
    Emit { topic: String, payload: Value },
    Response { id: String, payload: Value },
    Error { id: Option<String>, message: String },
}

struct PendingCalls {
    next: u64,
    waiters: std::collections::HashMap<String, std::sync::mpsc::Sender<Result<Value, String>>>,
}

/// Spawn the Sidecar binary and wire it to a scoped Bus proxy.
pub fn spawn_and_attach(
    binary: &std::path::Path,
    proxy: BusProxy,
) -> Result<Child, String> {
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to spawn Sidecar {}: {e}", binary.display()))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Sidecar missing stdout".to_string())?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Sidecar missing stdin".to_string())?;
    let stdin = Arc::new(Mutex::new(stdin));
    let pending = Arc::new(Mutex::new(PendingCalls {
        next: 1,
        waiters: Default::default(),
    }));

    // Forward Sidecar → Bus
    let bus_proxy = proxy.clone();
    let pending_out = Arc::clone(&pending);
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            let Ok(msg) = serde_json::from_str::<SidecarToHost>(&line) else {
                continue;
            };
            match msg {
                SidecarToHost::Ready => {}
                SidecarToHost::Emit { topic, payload } => {
                    let _ = bus_proxy.emit(&topic, payload);
                }
                SidecarToHost::Response { id, payload } => {
                    if let Some(tx) = pending_out.lock().expect("pending").waiters.remove(&id) {
                        let _ = tx.send(Ok(payload));
                    }
                }
                SidecarToHost::Error { id, message } => {
                    if let Some(id) = id {
                        if let Some(tx) = pending_out.lock().expect("pending").waiters.remove(&id) {
                            let _ = tx.send(Err(message));
                        }
                    }
                }
            }
        }
    });

    // Forward Bus events the Sidecar subscribed to → Sidecar stdin
    let stdin_events = Arc::clone(&stdin);
    proxy
        .subscribe("echo.ping", move |payload| {
            let _ = write_msg(
                &stdin_events,
                &HostToSidecar::Event {
                    topic: "echo.ping".into(),
                    payload,
                },
            );
        })
        .map_err(|e| e.to_string())?;

    // Serve calls by asking the Sidecar
    let stdin_calls = Arc::clone(&stdin);
    let pending_calls = Arc::clone(&pending);
    proxy
        .serve("echo.reflect", move |payload| {
            let (id, rx) = {
                let mut state = pending_calls.lock().expect("pending");
                let id = state.next.to_string();
                state.next += 1;
                let (tx, rx) = std::sync::mpsc::channel();
                state.waiters.insert(id.clone(), tx);
                (id, rx)
            };
            write_msg(
                &stdin_calls,
                &HostToSidecar::Call {
                    id,
                    topic: "echo.reflect".into(),
                    payload,
                },
            )?;
            rx.recv()
                .map_err(|_| "Sidecar call channel closed".to_string())?
        })
        .map_err(|e| e.to_string())?;

    Ok(child)
}

fn write_msg(stdin: &Arc<Mutex<ChildStdin>>, msg: &HostToSidecar) -> Result<(), String> {
    let mut guard = stdin.lock().map_err(|_| "Sidecar stdin poisoned".to_string())?;
    let line = serde_json::to_string(msg).map_err(|e| e.to_string())?;
    writeln!(guard, "{line}").map_err(|e| e.to_string())?;
    guard.flush().map_err(|e| e.to_string())
}
