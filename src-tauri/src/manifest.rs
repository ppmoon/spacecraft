//! Declarative Manifest for a Plugin package.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub ui: String,
    pub window_type: WindowType,
    pub root: PathBuf,
    /// Relative Sidecar entry; when set the Plugin is privileged.
    pub sidecar: Option<String>,
    pub permissions: ManifestPermissions,
    pub contracts: HashMap<String, BusContract>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowType {
    /// Local UI entry.
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ManifestPermissions {
    pub emit: Vec<String>,
    pub subscribe: Vec<String>,
    pub call: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusContract {
    pub request: Value,
    pub response: Value,
}

#[derive(Debug, Deserialize)]
struct ManifestFile {
    id: String,
    name: String,
    version: String,
    ui: String,
    window: ManifestWindow,
    #[serde(default)]
    sidecar: Option<String>,
    #[serde(default)]
    permissions: Option<ManifestPermissionsFile>,
    #[serde(default)]
    contracts: Option<HashMap<String, BusContractFile>>,
}

#[derive(Debug, Deserialize)]
struct ManifestWindow {
    #[serde(rename = "type")]
    window_type: String,
}

#[derive(Debug, Deserialize, Default)]
struct ManifestPermissionsFile {
    #[serde(default)]
    emit: Vec<String>,
    #[serde(default)]
    subscribe: Vec<String>,
    #[serde(default)]
    call: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BusContractFile {
    #[serde(default = "empty_object")]
    request: Value,
    #[serde(default = "empty_object")]
    response: Value,
}

fn empty_object() -> Value {
    Value::Object(Default::default())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    MissingFile,
    InvalidJson,
    InvalidField(&'static str),
    MissingUiEntry,
    MissingSidecarEntry,
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::MissingFile => write!(f, "manifest.json missing"),
            ManifestError::InvalidJson => write!(f, "manifest.json is not valid JSON"),
            ManifestError::InvalidField(field) => write!(f, "manifest field invalid: {field}"),
            ManifestError::MissingUiEntry => write!(f, "ui entry file missing"),
            ManifestError::MissingSidecarEntry => write!(f, "sidecar entry missing"),
        }
    }
}

impl Manifest {
    pub fn load_from_plugin_dir(dir: &Path) -> Result<Self, ManifestError> {
        let path = dir.join("manifest.json");
        let raw = fs::read_to_string(&path).map_err(|_| ManifestError::MissingFile)?;
        let parsed: ManifestFile =
            serde_json::from_str(&raw).map_err(|_| ManifestError::InvalidJson)?;

        validate_non_empty(&parsed.id, "id")?;
        validate_non_empty(&parsed.name, "name")?;
        validate_non_empty(&parsed.version, "version")?;
        validate_non_empty(&parsed.ui, "ui")?;

        if parsed.window.window_type != "local" {
            return Err(ManifestError::InvalidField("window.type"));
        }

        let ui_path = resolve_under_root(dir, &parsed.ui)?;
        if !ui_path.is_file() {
            return Err(ManifestError::MissingUiEntry);
        }

        if let Some(sidecar) = &parsed.sidecar {
            validate_non_empty(sidecar, "sidecar")?;
            // Bare binary names (no path separators) are resolved by the Host at spawn time.
            if sidecar.contains('/') || sidecar.contains('\\') {
                let sidecar_path = resolve_under_root(dir, sidecar)?;
                if !sidecar_path.exists() {
                    return Err(ManifestError::MissingSidecarEntry);
                }
            }
        }

        let permissions = parsed
            .permissions
            .map(|p| ManifestPermissions {
                emit: p.emit,
                subscribe: p.subscribe,
                call: p.call,
            })
            .unwrap_or_default();

        let contracts = parsed
            .contracts
            .unwrap_or_default()
            .into_iter()
            .map(|(topic, c)| {
                (
                    topic,
                    BusContract {
                        request: c.request,
                        response: c.response,
                    },
                )
            })
            .collect();

        Ok(Self {
            id: parsed.id,
            name: parsed.name,
            version: parsed.version,
            ui: parsed.ui,
            window_type: WindowType::Local,
            root: dir.to_path_buf(),
            sidecar: parsed.sidecar,
            permissions,
            contracts,
        })
    }

    pub fn ui_entry_path(&self) -> PathBuf {
        self.root.join(&self.ui)
    }

    pub fn sidecar_entry_path(&self) -> Option<PathBuf> {
        self.sidecar.as_ref().map(|s| self.root.join(s))
    }

    pub fn is_privileged(&self) -> bool {
        self.sidecar.is_some()
    }
}

fn validate_non_empty(value: &str, field: &'static str) -> Result<(), ManifestError> {
    if value.trim().is_empty() {
        return Err(ManifestError::InvalidField(field));
    }
    Ok(())
}

fn resolve_under_root(root: &Path, relative: &str) -> Result<PathBuf, ManifestError> {
    if relative.split(['/', '\\']).any(|p| p == "..") {
        return Err(ManifestError::InvalidField("path"));
    }
    Ok(root.join(relative))
}
