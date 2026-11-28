/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Profile session persistence and `<leader>qs` (`session.restore`) snapshot rotation (`NAV-02`).

use crate::model::{BrowserModel, TabKind};
use std::path::{Path, PathBuf};

/// One persisted workspace entry in a session snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSpaceRecord {
    pub name: String,
    pub icon: String,
}

/// One persisted tab entry in a session snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTabRecord {
    pub url: String,
    pub title: String,
    pub custom_title: Option<String>,
    pub space_index: Option<usize>,
    pub kind: TabKind,
}

/// Persisted snapshot of open spaces and tabs from a browser run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub spaces: Vec<SessionSpaceRecord>,
    pub active_space_index: usize,
    pub tabs: Vec<SessionTabRecord>,
    pub active_tab_index: Option<usize>,
}

/// Returns `(current_session_path, last_session_path)` inside the user's Zendia config directory.
pub fn session_profile_paths() -> Option<(PathBuf, PathBuf)> {
    let dir = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        PathBuf::from(xdg).join("zendia")
    } else {
        let home = std::env::var("HOME").ok().filter(|h| !h.is_empty())?;
        PathBuf::from(home).join(".config").join("zendia")
    };
    Some((dir.join("session.conf"), dir.join("last-session.conf")))
}

fn escape_field(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

fn unescape_field(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Captures a [`SessionSnapshot`] from `model`.
pub fn snapshot_from_model(model: &BrowserModel) -> SessionSnapshot {
    let spaces: Vec<SessionSpaceRecord> = model
        .spaces
        .iter()
        .map(|s| SessionSpaceRecord {
            name: s.name.clone(),
            icon: s.icon.clone(),
        })
        .collect();
    let active_space_index = model
        .active_space
        .and_then(|id| model.spaces.iter().position(|s| s.id == id))
        .unwrap_or(0);

    let mut tabs = Vec::new();
    let mut active_tab_index = None;
    for tab in &model.tabs {
        if model.is_glance_tab(tab.id) {
            continue;
        }
        if tab.url.is_empty() || tab.url == "about:blank" || tab.url == "about:newtab" {
            continue;
        }
        if model.active_tab == Some(tab.id) {
            active_tab_index = Some(tabs.len());
        }
        let space_index = tab
            .space_id
            .and_then(|sid| model.spaces.iter().position(|s| s.id == sid));
        tabs.push(SessionTabRecord {
            url: tab.url.clone(),
            title: tab.title.clone(),
            custom_title: tab.custom_title.clone(),
            space_index,
            kind: tab.kind,
        });
    }

    SessionSnapshot {
        spaces,
        active_space_index,
        tabs,
        active_tab_index,
    }
}

/// Writes `snapshot` to `path`.
pub fn save_session_to(path: &Path, snapshot: &SessionSnapshot) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = String::new();
    out.push_str("version=1\n");
    out.push_str(&format!("active_space={}\n", snapshot.active_space_index));
    if let Some(active_tab) = snapshot.active_tab_index {
        out.push_str(&format!("active_tab={active_tab}\n"));
    }
    for space in &snapshot.spaces {
        out.push_str(&format!(
            "space\t{}\t{}\n",
            escape_field(&space.name),
            escape_field(&space.icon)
        ));
    }
    for tab in &snapshot.tabs {
        let kind_str = match tab.kind {
            TabKind::Normal => "normal",
            TabKind::Pinned => "pinned",
            TabKind::Essential => "essential",
        };
        let space_str = tab.space_index.map(|i| i.to_string()).unwrap_or_default();
        let custom_str = tab.custom_title.as_deref().map(escape_field).unwrap_or_default();
        out.push_str(&format!(
            "tab\t{kind_str}\t{space_str}\t{}\t{}\t{custom_str}\n",
            escape_field(&tab.url),
            escape_field(&tab.title),
        ));
    }
    std::fs::write(path, out)
}

/// Loads a [`SessionSnapshot`] from `path`.
pub fn load_session_from(path: &Path) -> Option<SessionSnapshot> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut snapshot = SessionSnapshot::default();
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("active_space=") {
            if let Ok(idx) = rest.trim().parse::<usize>() {
                snapshot.active_space_index = idx;
            }
        } else if let Some(rest) = line.strip_prefix("active_tab=") {
            if let Ok(idx) = rest.trim().parse::<usize>() {
                snapshot.active_tab_index = Some(idx);
            }
        } else if let Some(rest) = line.strip_prefix("space\t") {
            let parts: Vec<&str> = rest.splitn(2, '\t').collect();
            if parts.len() == 2 {
                snapshot.spaces.push(SessionSpaceRecord {
                    name: unescape_field(parts[0]),
                    icon: unescape_field(parts[1]),
                });
            }
        } else if let Some(rest) = line.strip_prefix("tab\t") {
            let parts: Vec<&str> = rest.splitn(5, '\t').collect();
            if parts.len() == 5 {
                let kind = match parts[0] {
                    "pinned" => TabKind::Pinned,
                    "essential" => TabKind::Essential,
                    _ => TabKind::Normal,
                };
                let space_index = if parts[1].is_empty() {
                    None
                } else {
                    parts[1].parse::<usize>().ok()
                };
                let url = unescape_field(parts[2]);
                if url.is_empty() {
                    continue;
                }
                let title = unescape_field(parts[3]);
                let custom_raw = unescape_field(parts[4]);
                let custom_title = (!custom_raw.is_empty()).then_some(custom_raw);
                snapshot.tabs.push(SessionTabRecord {
                    url,
                    title,
                    custom_title,
                    space_index,
                    kind,
                });
            }
        }
    }
    Some(snapshot)
}

/// Rotates `session.conf` -> `last-session.conf` at startup and returns the previous session snapshot if non-empty.
pub fn rotate_session_on_startup() -> Option<SessionSnapshot> {
    let (current_path, last_path) = session_profile_paths()?;
    if let Some(prev) = load_session_from(&current_path).filter(|s| !s.tabs.is_empty()) {
        let _ = save_session_to(&last_path, &prev);
        return Some(prev);
    }
    load_session_from(&last_path).filter(|s| !s.tabs.is_empty())
}
