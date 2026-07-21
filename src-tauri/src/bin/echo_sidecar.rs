//! Echo Sidecar — out-of-process privileged Plugin worker (stdio NDJSON).
//! Speaks only with the Host; never a Node runtime.

use std::io::{BufRead, BufReader, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum HostMessage {
    Event { topic: String, payload: Value },
    Call { id: String, topic: String, payload: Value },
    Shutdown,
}

#[derive(Debug, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum SidecarMessage {
    Ready,
    Emit { topic: String, payload: Value },
    Response { id: String, payload: Value },
    Error { id: Option<String>, message: String },
}

fn main() {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    write_msg(&mut out, &SidecarMessage::Ready);

    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin.lock());
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let msg: HostMessage = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                write_msg(
                    &mut out,
                    &SidecarMessage::Error {
                        id: None,
                        message: format!("bad Host message: {e}"),
                    },
                );
                continue;
            }
        };
        match msg {
            HostMessage::Shutdown => break,
            HostMessage::Event { topic, payload } if topic == "echo.ping" => {
                write_msg(
                    &mut out,
                    &SidecarMessage::Emit {
                        topic: "echo.pong".into(),
                        payload,
                    },
                );
            }
            HostMessage::Event { .. } => {}
            HostMessage::Call { id, topic, payload } if topic == "echo.reflect" => {
                write_msg(
                    &mut out,
                    &SidecarMessage::Response {
                        id,
                        payload,
                    },
                );
            }
            HostMessage::Call { id, topic, .. } => {
                write_msg(
                    &mut out,
                    &SidecarMessage::Error {
                        id: Some(id),
                        message: format!("unhandled call `{topic}`"),
                    },
                );
            }
        }
    }
}

fn write_msg(out: &mut impl Write, msg: &SidecarMessage) {
    if let Ok(line) = serde_json::to_string(msg) {
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }
}
