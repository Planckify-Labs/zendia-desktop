/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! On-disk persistence for extension storage blobs (`local`, `sync`, `permissions`, `menus`, `ui`, `dnr`).

use std::fs;
use std::path::PathBuf;

/// Keeps one JSON file per `(extension_id, key)` inside `<root>/<sanitized_id>/<sanitized_key>.json`.
#[derive(Clone, Debug)]
pub struct ExtensionStorage {
    root: PathBuf,
}

impl ExtensionStorage {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn path_for(&self, extension_id: &str, key: &str) -> PathBuf {
        self.root
            .join(sanitize_component(extension_id))
            .join(format!("{}.json", sanitize_component(key)))
    }

    pub fn read(&self, extension_id: &str, key: &str) -> Option<String> {
        let path = self.path_for(extension_id, key);
        let text = fs::read_to_string(&path).ok()?;
        // Verify valid JSON before handing back to cgx-webext.
        if serde_json::from_str::<serde_json::Value>(&text).is_err() {
            return None;
        }
        Some(text)
    }

    pub fn write(&self, extension_id: &str, key: &str, json: &str) {
        if serde_json::from_str::<serde_json::Value>(json).is_err() {
            eprintln!("zendia-extension-server: refusing to persist invalid JSON for {extension_id}/{key}");
            return;
        }
        let path = self.path_for(extension_id, key);
        if let Some(parent) = path.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            eprintln!("zendia-extension-server: cannot create {}: {error}", parent.display());
            return;
        }
        let tmp = path.with_extension("json.part");
        if let Err(error) = fs::write(&tmp, json.as_bytes()).and_then(|()| fs::rename(&tmp, &path)) {
            eprintln!("zendia-extension-server: cannot persist {}: {error}", path.display());
            let _ = fs::remove_file(&tmp);
        }
    }

    pub fn remove_all(&self, extension_id: &str) {
        let dir = self.root.join(sanitize_component(extension_id));
        if dir.is_dir()
            && let Err(error) = fs::remove_dir_all(&dir)
        {
            eprintln!("zendia-extension-server: cannot remove {}: {error}", dir.display());
        }
    }
}

/// Restricts extension-provided IDs and keys to a safe filename alphabet so they can never escape `root`.
pub fn sanitize_component(component: &str) -> String {
    let sanitized: String = component
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        "_".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_blocks_path_traversal() {
        assert_eq!(sanitize_component(".."), "_");
        assert_eq!(sanitize_component("."), "_");
        assert_eq!(sanitize_component(""), "_");
        assert_eq!(sanitize_component("../etc/passwd"), ".._etc_passwd");
        assert_eq!(sanitize_component("ublock@raymondhill.net"), "ublock_raymondhill.net");
    }

    #[test]
    fn storage_round_trips_and_removes() {
        let dir = std::env::temp_dir().join(format!("zendia-ext-storage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = ExtensionStorage::new(dir.clone());
        assert_eq!(store.read("ext1", "local"), None);
        store.write("ext1", "local", r#"{"count":42}"#);
        assert_eq!(store.read("ext1", "local").as_deref(), Some(r#"{"count":42}"#));
        store.remove_all("ext1");
        assert_eq!(store.read("ext1", "local"), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
