//! Host-persisted Workspace: window layout + Plugin instance identities + Window Groups.
//! Plugins own their own business state — never snapshotted here.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Default for WindowGeometry {
    fn default() -> Self {
        Self {
            x: 80.0,
            y: 80.0,
            width: 800.0,
            height: 600.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WorkspaceWindowKind {
    #[serde(rename_all = "camelCase")]
    Blank {
        /// Stable identity so Window Groups can rebind blanks across restarts.
        #[serde(default)]
        blank_id: String,
    },
    #[serde(rename_all = "camelCase")]
    Plugin {
        plugin_id: String,
        instance_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceWindow {
    pub kind: WorkspaceWindowKind,
    pub geometry: WindowGeometry,
}

/// Named Window Group membership recipe (survives member windows being closed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WorkspaceGroupMember {
    #[serde(rename_all = "camelCase")]
    Blank { blank_id: String },
    #[serde(rename_all = "camelCase")]
    Plugin {
        plugin_id: String,
        instance_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceGroup {
    pub id: String,
    pub name: String,
    pub members: Vec<WorkspaceGroupMember>,
}

/// Serializable Workspace snapshot owned by the Host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Workspace {
    pub windows: Vec<WorkspaceWindow>,
    #[serde(default)]
    pub groups: Vec<WorkspaceGroup>,
}

impl Workspace {
    pub fn save_to_path(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, raw).map_err(|e| e.to_string())
    }

    /// Load Workspace. Corrupt / partial / missing files return `Ok(None)` so boot can continue.
    pub fn load_from_path(path: &Path) -> Result<Option<Self>, String> {
        if !path.exists() {
            return Ok(None);
        }
        let raw = match fs::read_to_string(path) {
            Ok(r) => r,
            Err(_) => return Ok(None),
        };
        match serde_json::from_str::<Workspace>(&raw) {
            Ok(ws) => Ok(Some(ws)),
            Err(_) => Ok(None), // corrupt → graceful empty
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "spacecraft-ws-unit-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }

    #[test]
    fn missing_file_loads_as_none() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        assert_eq!(Workspace::load_from_path(&path).unwrap(), None);
    }

    #[test]
    fn corrupt_file_loads_as_none() {
        let path = temp_path();
        fs::write(&path, "not-json").unwrap();
        assert_eq!(Workspace::load_from_path(&path).unwrap(), None);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn round_trip_preserves_layout_groups_and_instance_ids() {
        let path = temp_path();
        let ws = Workspace {
            windows: vec![
                WorkspaceWindow {
                    kind: WorkspaceWindowKind::Blank {
                        blank_id: "blank-1".into(),
                    },
                    geometry: WindowGeometry {
                        x: 10.0,
                        y: 20.0,
                        width: 300.0,
                        height: 200.0,
                    },
                },
                WorkspaceWindow {
                    kind: WorkspaceWindowKind::Plugin {
                        plugin_id: "hello".into(),
                        instance_id: "inst-1".into(),
                    },
                    geometry: WindowGeometry {
                        x: 40.0,
                        y: 50.0,
                        width: 800.0,
                        height: 600.0,
                    },
                },
            ],
            groups: vec![WorkspaceGroup {
                id: "g-1".into(),
                name: "Morning".into(),
                members: vec![
                    WorkspaceGroupMember::Blank {
                        blank_id: "blank-1".into(),
                    },
                    WorkspaceGroupMember::Plugin {
                        plugin_id: "hello".into(),
                        instance_id: "inst-1".into(),
                    },
                ],
            }],
        };
        ws.save_to_path(&path).unwrap();
        let loaded = Workspace::load_from_path(&path).unwrap().unwrap();
        assert_eq!(loaded, ws);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn legacy_workspace_without_groups_still_loads() {
        let path = temp_path();
        fs::write(
            &path,
            r#"{
              "windows": [
                { "kind": { "type": "blank" }, "geometry": { "x": 1, "y": 2, "width": 3, "height": 4 } },
                {
                  "kind": { "type": "plugin", "pluginId": "hello", "instanceId": "i1" },
                  "geometry": { "x": 5, "y": 6, "width": 7, "height": 8 }
                }
              ]
            }"#,
        )
        .unwrap();
        let loaded = Workspace::load_from_path(&path).unwrap().unwrap();
        assert!(loaded.groups.is_empty());
        assert_eq!(loaded.windows.len(), 2);
        assert!(matches!(
            &loaded.windows[0].kind,
            WorkspaceWindowKind::Blank { blank_id } if blank_id.is_empty()
        ));
        let _ = fs::remove_file(&path);
    }
}
