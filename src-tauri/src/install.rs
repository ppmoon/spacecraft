//! Local Plugin install: propose → confirm permissions → copy into plugins dir.

use std::fs::{self, File};
use std::io::{copy, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::manifest::Manifest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionItem {
    pub kind: String,
    pub detail: String,
    pub sensitive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProposal {
    pub proposal_id: String,
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub permissions: Vec<PermissionItem>,
    pub signature_present: bool,
    pub source_kind: String,
}

#[derive(Debug, Clone)]
pub struct PendingInstall {
    pub proposal: InstallProposal,
    pub staging_dir: PathBuf,
    pub manifest: Manifest,
}

pub fn permission_items(manifest: &Manifest) -> Vec<PermissionItem> {
    let mut items = Vec::new();
    if manifest.is_privileged() {
        items.push(PermissionItem {
            kind: "sidecar".into(),
            detail: "Runs a privileged Sidecar process".into(),
            sensitive: true,
        });
    }
    for topic in &manifest.permissions.emit {
        items.push(PermissionItem {
            kind: "emit".into(),
            detail: topic.clone(),
            sensitive: true,
        });
    }
    for topic in &manifest.permissions.subscribe {
        items.push(PermissionItem {
            kind: "subscribe".into(),
            detail: topic.clone(),
            sensitive: true,
        });
    }
    for topic in &manifest.permissions.call {
        items.push(PermissionItem {
            kind: "call".into(),
            detail: topic.clone(),
            sensitive: true,
        });
    }
    items
}

pub fn stage_from_folder(source: &Path, staging_root: &Path) -> Result<(PathBuf, Manifest), String> {
    if !source.is_dir() {
        return Err(format!("not a Plugin folder: {}", source.display()));
    }
    let manifest = Manifest::load_from_plugin_dir(source).map_err(|e| e.to_string())?;
    let staging = staging_root.join(format!("folder-{}", manifest.id));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    }
    copy_dir(source, &staging)?;
    let staged = Manifest::load_from_plugin_dir(&staging).map_err(|e| e.to_string())?;
    Ok((staging, staged))
}

pub fn stage_from_zip(source: &Path, staging_root: &Path) -> Result<(PathBuf, Manifest), String> {
    if !source.is_file() {
        return Err(format!("not a zip file: {}", source.display()));
    }
    let file = File::open(source).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    let extract_root = staging_root.join("zip-extract");
    if extract_root.exists() {
        fs::remove_dir_all(&extract_root).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&extract_root).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = file
            .enclosed_name()
            .ok_or_else(|| "zip entry has unsafe path".to_string())?
            .to_path_buf();
        let out = extract_root.join(&name);
        if file.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = File::create(&out).map_err(|e| e.to_string())?;
            copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }

    let plugin_root = find_manifest_root(&extract_root)
        .ok_or_else(|| "zip does not contain a Plugin Manifest".to_string())?;
    let manifest = Manifest::load_from_plugin_dir(&plugin_root).map_err(|e| e.to_string())?;
    let staging = staging_root.join(format!("zip-{}", manifest.id));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    }
    copy_dir(&plugin_root, &staging)?;
    let staged = Manifest::load_from_plugin_dir(&staging).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&extract_root);
    Ok((staging, staged))
}

pub fn commit_staged_install(
    staging_dir: &Path,
    plugins_dir: &Path,
    plugin_id: &str,
) -> Result<PathBuf, String> {
    fs::create_dir_all(plugins_dir).map_err(|e| e.to_string())?;
    let dest = plugins_dir.join(plugin_id);
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    copy_dir(staging_dir, &dest)?;
    Ok(dest)
}

#[allow(dead_code)] // used by Host-seam tests to build zip fixtures
pub fn write_zip_from_dir(dir: &Path, zip_path: &Path) -> Result<(), String> {
    let file = File::create(zip_path).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let dir = dir.canonicalize().map_err(|e| e.to_string())?;

    for entry in WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = path
            .strip_prefix(&dir)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if name.is_empty() {
            continue;
        }
        if path.is_dir() {
            zip.add_directory(format!("{name}/"), options)
                .map_err(|e| e.to_string())?;
        } else {
            zip.start_file(&name, options).map_err(|e| e.to_string())?;
            let mut f = File::open(path).map_err(|e| e.to_string())?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            zip.write_all(&buf).map_err(|e| e.to_string())?;
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn find_manifest_root(root: &Path) -> Option<PathBuf> {
    if root.join("manifest.json").is_file() {
        return Some(root.to_path_buf());
    }
    let mut dirs = fs::read_dir(root).ok()?.flatten().filter(|e| e.path().is_dir());
    let first = dirs.next()?.path();
    if dirs.next().is_none() && first.join("manifest.json").is_file() {
        return Some(first);
    }
    for entry in WalkDir::new(root).max_depth(3).into_iter().filter_map(|e| e.ok()) {
        if entry.file_name() == "manifest.json" {
            return entry.path().parent().map(|p| p.to_path_buf());
        }
    }
    None
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in WalkDir::new(from).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let rel = path.strip_prefix(from).map_err(|e| e.to_string())?;
        let dest = to.join(rel);
        if path.is_dir() {
            fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(path, &dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
