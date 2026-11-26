/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Extension store installer (Chrome Web Store and addons.mozilla.org) running inside `ExtensionServer`.
//!
//! Keeps downloaded `.crx`/`.xpi` packages in `<data_dir>/ExtensionPackages` with an `index.json` manifest so
//! store-installed extensions reload across restarts. Network downloads are delegated to the Hub's RequestServer
//! over the embedder channel so `ExtensionServer` requires no direct network access in its sandbox.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Kept package entry stored in `ExtensionPackages/index.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeptPackage {
    pub id: String,
    pub file: String,
    pub store: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct PackageIndex {
    #[serde(default)]
    extensions: Vec<KeptPackage>,
}

/// Manages kept store packages and in-flight Chrome Web Store / addons.mozilla.org installs.
pub struct StoreState {
    packages_dir: PathBuf,
    pub kept: Vec<KeptPackage>,
    /// Chrome Web Store installs approved in `beginInstallWithManifest3`, waiting for `completeInstall`: id -> manifest.
    pub approved_cws_installs: HashMap<String, String>,
    pub installs_in_progress: HashSet<String>,
}

impl StoreState {
    pub fn new(packages_dir: PathBuf) -> Self {
        Self {
            packages_dir,
            kept: Vec::new(),
            approved_cws_installs: HashMap::new(),
            installs_in_progress: HashSet::new(),
        }
    }

    pub fn packages_dir(&self) -> &Path {
        &self.packages_dir
    }

    fn index_path(&self) -> PathBuf {
        self.packages_dir.join("index.json")
    }

    /// Reads `index.json` and returns `(entry, bytes)` for every kept package readable on disk.
    pub fn read_kept_packages(&self) -> Vec<(KeptPackage, Vec<u8>)> {
        let Ok(text) = fs::read_to_string(self.index_path()) else {
            return Vec::new();
        };
        let Ok(index) = serde_json::from_str::<PackageIndex>(&text) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in index.extensions {
            if entry.file.contains('/') || entry.file.contains('\\') || entry.file.starts_with('.') {
                continue;
            }
            let path = self.packages_dir.join(&entry.file);
            match fs::read(&path) {
                Ok(bytes) => out.push((entry, bytes)),
                Err(error) => eprintln!(
                    "zendia-extension-server: cannot read kept package {}: {error}",
                    path.display()
                ),
            }
        }
        out
    }

    /// Persists a verified package to `ExtensionPackages/` and updates `index.json`.
    pub fn save_kept_package(&mut self, extension_id: &str, store: &str, bytes: &[u8]) -> Result<(), String> {
        fs::create_dir_all(&self.packages_dir).map_err(|error| format!("cannot create packages directory: {error}"))?;
        let ext = if store == "chrome" { "crx" } else { "xpi" };
        let file_name = safe_package_filename(extension_id, ext);
        let path = self.packages_dir.join(&file_name);
        let tmp = self.packages_dir.join(format!("{file_name}.part"));
        fs::write(&tmp, bytes)
            .and_then(|()| fs::rename(&tmp, &path))
            .map_err(|error| {
                let _ = fs::remove_file(&tmp);
                format!("cannot save extension package: {error}")
            })?;

        self.kept.retain(|k| k.id != extension_id);
        self.kept.push(KeptPackage {
            id: extension_id.to_string(),
            file: file_name,
            store: store.to_string(),
        });
        self.write_index();
        Ok(())
    }

    /// Removes a kept package file from disk if present.
    pub fn remove_kept_package(&mut self, extension_id: &str) -> bool {
        let Some(pos) = self.kept.iter().position(|k| k.id == extension_id) else {
            return false;
        };
        let entry = self.kept.remove(pos);
        let path = self.packages_dir.join(&entry.file);
        if let Err(error) = fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("zendia-extension-server: cannot remove {}: {error}", path.display());
        }
        self.write_index();
        true
    }

    pub fn write_index(&self) {
        let _ = fs::create_dir_all(&self.packages_dir);
        let index = PackageIndex {
            extensions: self.kept.clone(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&index) {
            let path = self.index_path();
            let tmp = path.with_extension("json.part");
            if let Err(error) = fs::write(&tmp, json.as_bytes()).and_then(|()| fs::rename(&tmp, &path)) {
                eprintln!("zendia-extension-server: cannot write {}: {error}", path.display());
                let _ = fs::remove_file(&tmp);
            }
        }
    }
}

pub fn store_page_key(store: &str) -> String {
    format!("{}{store}", webext::store::STORE_BRIDGE_KEY)
}

pub fn store_display_name(store: &str) -> &'static str {
    if store == "chrome" {
        "the Chrome Web Store"
    } else {
        "addons.mozilla.org"
    }
}

pub fn cws_installed_event(management_info: &Value) -> Value {
    json!({
        "kind": "event",
        "name": "management.onInstalled",
        "args": [management_info],
    })
}

pub fn cws_uninstalled_event(extension_id: &str) -> Value {
    json!({
        "kind": "event",
        "name": "management.onUninstalled",
        "args": [extension_id],
    })
}

pub fn amo_event(name: &str, extension_id: &str) -> Value {
    json!({
        "kind": "event",
        "name": name,
        "id": extension_id,
    })
}

pub fn amo_install_progress(
    install_id: i64,
    event: &str,
    state: &str,
    error: Option<&str>,
    size: Option<usize>,
) -> Value {
    let mut obj = json!({
        "kind": "install",
        "installId": install_id,
        "event": event,
        "state": state,
    });
    if let Some(err) = error {
        obj["error"] = json!(err);
    }
    if let Some(sz) = size {
        obj["progress"] = json!(sz);
        obj["maxProgress"] = json!(sz);
    }
    obj
}

fn safe_package_filename(extension_id: &str, ext: &str) -> String {
    let mut out = String::with_capacity(extension_id.len() + ext.len() + 2);
    out.push('_');
    for ch in extension_id.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '@') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out.push('.');
    out.push_str(ext);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_state_persists_and_removes_packages() {
        let dir = std::env::temp_dir().join(format!("zendia-store-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut store = StoreState::new(dir.clone());
        store
            .save_kept_package("ext@example", "firefox", b"PK\x03\x04test")
            .unwrap();
        let loaded = store.read_kept_packages();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].0.id, "ext@example");
        assert_eq!(loaded[0].1, b"PK\x03\x04test");
        assert!(store.remove_kept_package("ext@example"));
        assert!(store.read_kept_packages().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
