//! Declarative Manifest for a Plugin package.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub ui: String,
    pub window_type: WindowType,
    pub root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowType {
    /// Local UI entry — Pure-UI Plugin (no Sidecar).
    Local,
}

#[derive(Debug, Deserialize)]
struct ManifestFile {
    id: String,
    name: String,
    version: String,
    ui: String,
    window: ManifestWindow,
}

#[derive(Debug, Deserialize)]
struct ManifestWindow {
    #[serde(rename = "type")]
    window_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    MissingFile,
    InvalidJson,
    InvalidField(&'static str),
    MissingUiEntry,
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::MissingFile => write!(f, "manifest.json missing"),
            ManifestError::InvalidJson => write!(f, "manifest.json is not valid JSON"),
            ManifestError::InvalidField(field) => write!(f, "manifest field invalid: {field}"),
            ManifestError::MissingUiEntry => write!(f, "ui entry file missing"),
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

        let ui_path = resolve_ui_entry(dir, &parsed.ui)?;
        if !ui_path.is_file() {
            return Err(ManifestError::MissingUiEntry);
        }

        Ok(Self {
            id: parsed.id,
            name: parsed.name,
            version: parsed.version,
            ui: parsed.ui,
            window_type: WindowType::Local,
            root: dir.to_path_buf(),
        })
    }

    pub fn ui_entry_path(&self) -> PathBuf {
        self.root.join(&self.ui)
    }
}

fn validate_non_empty(value: &str, field: &'static str) -> Result<(), ManifestError> {
    if value.trim().is_empty() {
        return Err(ManifestError::InvalidField(field));
    }
    Ok(())
}

fn resolve_ui_entry(root: &Path, ui: &str) -> Result<PathBuf, ManifestError> {
    let candidate = root.join(ui);
    let root_canon = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf());
    let ui_canon = candidate
        .canonicalize()
        .unwrap_or_else(|_| candidate.clone());
    if !ui_canon.starts_with(&root_canon) {
        return Err(ManifestError::InvalidField("ui"));
    }
    Ok(candidate)
}
