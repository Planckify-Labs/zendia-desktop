/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Generic keyboard-driven picker model, built-in pickers ("tabs", "agents"), text filtering,
//! key-badge assignment, and list/grid cursor movement.
//!
//! Pure data and logic with no UI toolkit dependencies so commands, plugins, agents, headless
//! runners, and Freya share the same contract.

use crate::address_bar::display_url_for_result;
use crate::model::{BrowserModel, Tab, TabId};
use std::collections::BTreeSet;

/// Key labels assigned automatically in list layout (`j` and `k` stay free for vertical movement).
pub const LIST_KEY_LABELS: &str = "asdfghlqweruiopzxcvbnmty";

/// Key labels assigned automatically in grid layout (`h`, `j`, `k`, and `l` stay free for 2D movement).
pub const GRID_KEY_LABELS: &str = "asdfgqweruiopzxcvbnmty";

/// Default footer hint shown in list-layout pickers.
pub const DEFAULT_LIST_HINT: &str = "j/k move · Enter pick · label picks · Esc cancel";

/// Default footer hint shown in grid-layout pickers.
pub const DEFAULT_GRID_HINT: &str = "h/j/k/l move · Enter pick · label picks · Esc cancel";

/// Default number of columns in a grid-layout picker (matching Zendia's 3-column grid).
pub const DEFAULT_GRID_COLUMNS: usize = 3;

/// Stable identifier for an open or registered picker request.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PickerRequestId(pub String);

impl PickerRequestId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Visual layout for a Zendia picker modal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PickerLayout {
    /// Vertical rows with optional section headings, key badges, icons, titles, and right-aligned details.
    #[default]
    List,
    /// Multi-column icon tiles with corner key badges, icon plates, centered titles, and details.
    Grid,
}

impl PickerLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Grid => "grid",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "list" => Some(Self::List),
            "grid" => Some(Self::Grid),
            _ => None,
        }
    }
}

/// One selectable (or dimmed/disabled) item in a [`PickerRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerItem {
    /// Stable identifier passed back in `picker.choose`.
    pub id: String,
    /// Primary display title.
    pub title: String,
    /// Optional muted detail (URL/status in list layout, version/install status in grid layout).
    pub detail: Option<String>,
    /// Optional icon name from `ICON_TABLE` or a known brand emblem id (e.g. `agent-claude`).
    pub icon: Option<String>,
    /// Optional favicon/brand badge RGB tint for list-row icon badges.
    pub icon_badge_rgb: Option<(u8, u8, u8)>,
    /// Optional per-item accent color used for the glow on a selected grid tile.
    pub accent_rgb: Option<(u8, u8, u8)>,
    /// When true in grid layout, the icon fills 100% of its rounded icon plate (`zendia.css:895`).
    pub icon_fill: bool,
    /// Optional section heading shown above the first item of a contiguous run sharing it.
    pub section: Option<String>,
    /// Optional shortcut jump key badge (`a`, `s`, `d`, ...). Assigned automatically if `None`.
    pub key: Option<String>,
    /// Whether the item can be activated. Disabled items render dimmed and ignore selection commands.
    pub enabled: bool,
    /// Whether the item renders visually dimmed (e.g. an uninstalled agent).
    pub dimmed: bool,
    /// Optional tooltip shown on hover (such as an install command for an unavailable agent).
    pub tooltip: Option<String>,
    /// Dotted command name executed when this item is chosen.
    pub command: String,
    /// Key/value string arguments passed to `command`.
    pub command_args: Vec<(String, String)>,
}

impl PickerItem {
    pub fn new(id: impl Into<String>, title: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            detail: None,
            icon: None,
            icon_badge_rgb: None,
            accent_rgb: None,
            icon_fill: false,
            section: None,
            key: None,
            enabled: true,
            dimmed: false,
            tooltip: None,
            command: command.into(),
            command_args: Vec::new(),
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn icon_badge_rgb(mut self, rgb: (u8, u8, u8)) -> Self {
        self.icon_badge_rgb = Some(rgb);
        self
    }

    pub fn accent_rgb(mut self, rgb: (u8, u8, u8)) -> Self {
        self.accent_rgb = Some(rgb);
        self
    }

    pub fn icon_fill(mut self, fill: bool) -> Self {
        self.icon_fill = fill;
        self
    }

    pub fn section(mut self, section: impl Into<String>) -> Self {
        self.section = Some(section.into());
        self
    }

    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn dimmed(mut self, dimmed: bool) -> Self {
        self.dimmed = dimmed;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn arg(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.command_args.push((key.into(), value.into()));
        self
    }
}

/// Complete picker specification that any command, plugin, or agent can open through `picker.open`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerRequest {
    pub id: PickerRequestId,
    pub title: String,
    pub layout: PickerLayout,
    pub sections: Vec<String>,
    pub items: Vec<PickerItem>,
    pub initial_selected: usize,
    pub hint: String,
    pub filterable: bool,
}

impl PickerRequest {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        layout: PickerLayout,
        mut items: Vec<PickerItem>,
    ) -> Self {
        assign_key_badges(&mut items, layout);
        let sections = collect_sections(&items);
        let hint = match layout {
            PickerLayout::List => DEFAULT_LIST_HINT.to_string(),
            PickerLayout::Grid => DEFAULT_GRID_HINT.to_string(),
        };
        Self {
            id: PickerRequestId::new(id),
            title: title.into(),
            layout,
            sections,
            items,
            initial_selected: 0,
            hint,
            filterable: true,
        }
    }

    pub fn initial_selected(mut self, index: usize) -> Self {
        self.initial_selected = if self.items.is_empty() {
            0
        } else {
            index.min(self.items.len() - 1)
        };
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    pub fn filterable(mut self, filterable: bool) -> Self {
        self.filterable = filterable;
        self
    }

    /// Finds an item by its stable `item_id`.
    pub fn find_item(&self, item_id: &str) -> Option<&PickerItem> {
        self.items.iter().find(|item| item.id == item_id)
    }

    /// Returns a filtered copy of the picker's items matching `query`.
    pub fn filtered_items(&self, query: &str) -> Vec<PickerItem> {
        filter_picker_items(self, query)
    }
}

/// Assigns single-character jump key badges to items that did not specify an explicit `key`.
pub fn assign_key_badges(items: &mut [PickerItem], layout: PickerLayout) {
    let reserved: BTreeSet<String> = items
        .iter()
        .filter_map(|item| item.key.as_ref().filter(|k| !k.is_empty()).cloned())
        .collect();
    let pool = match layout {
        PickerLayout::List => LIST_KEY_LABELS,
        PickerLayout::Grid => GRID_KEY_LABELS,
    };
    let mut free = pool
        .chars()
        .map(|ch| ch.to_string())
        .filter(|label| !reserved.contains(label));

    for item in items.iter_mut() {
        if item.key.as_ref().is_none_or(|k| k.is_empty()) {
            item.key = free.next();
        }
    }
}

fn collect_sections(items: &[PickerItem]) -> Vec<String> {
    let mut sections = Vec::new();
    for item in items {
        if let Some(section) = &item.section
            && !sections.contains(section)
        {
            sections.push(section.clone());
        }
    }
    sections
}

/// Groups contiguous runs of items sharing the same `section` into `(section_title, item_indices)` blocks.
pub fn group_items_by_section(items: &[PickerItem]) -> Vec<(Option<String>, Vec<usize>)> {
    let mut groups: Vec<(Option<String>, Vec<usize>)> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if let Some((last_section, indices)) = groups.last_mut()
            && *last_section == item.section
        {
            indices.push(index);
            continue;
        }
        groups.push((item.section.clone(), vec![index]));
    }
    groups
}

/// Filters `request.items` by case-insensitive whitespace-separated tokens across title, detail, and section.
///
/// Preserves section grouping order so sectioned pickers (such as Tabs) keep their headings intact.
pub fn filter_picker_items(request: &PickerRequest, query: &str) -> Vec<PickerItem> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|tok| tok.to_ascii_lowercase())
        .filter(|tok| !tok.is_empty())
        .collect();
    if tokens.is_empty() {
        return request.items.clone();
    }

    request
        .items
        .iter()
        .filter(|item| {
            let title = item.title.to_ascii_lowercase();
            let detail = item.detail.as_deref().unwrap_or_default().to_ascii_lowercase();
            let section = item.section.as_deref().unwrap_or_default().to_ascii_lowercase();
            tokens
                .iter()
                .all(|tok| title.contains(tok) || detail.contains(tok) || section.contains(tok))
        })
        .cloned()
        .collect()
}

/// Finds the index of the first item matching a single-key jump badge (case-sensitive, falling back to ASCII lowercase).
pub fn find_item_by_key_badge(items: &[PickerItem], pressed: &str) -> Option<usize> {
    if pressed.is_empty() {
        return None;
    }
    items
        .iter()
        .position(|item| item.key.as_deref() == Some(pressed))
        .or_else(|| {
            let lower = pressed.to_ascii_lowercase();
            items
                .iter()
                .position(|item| item.key.as_deref().is_some_and(|k| k.eq_ignore_ascii_case(&lower)))
        })
}

/// Steps the cursor inside a list or grid picker by `(dx, dy)` with clean wrapping.
pub fn step_picker_cursor(
    current: usize,
    count: usize,
    layout: PickerLayout,
    columns: usize,
    dx: isize,
    dy: isize,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let idx = current.min(count - 1);
    match layout {
        PickerLayout::List => {
            let delta = if dy != 0 { dy } else { dx };
            let next = (idx as isize + delta).rem_euclid(count as isize) as usize;
            Some(next)
        }
        PickerLayout::Grid => {
            let cols = columns.max(1);
            if dx != 0 {
                let next = (idx as isize + dx).rem_euclid(count as isize) as usize;
                return Some(next);
            }
            if dy != 0 {
                let rows = count.div_ceil(cols);
                let col = idx % cols;
                let mut row = idx / cols;
                for _ in 0..rows {
                    row = (row as isize + dy).rem_euclid(rows as isize) as usize;
                    let candidate = row * cols + col;
                    if candidate < count {
                        return Some(candidate);
                    }
                }
            }
            Some(idx)
        }
    }
}

fn tab_picker_detail(tab: &Tab, active_tab: Option<TabId>, suffix: Option<&str>) -> String {
    let url = display_url_for_result(&tab.url);
    let is_active = active_tab == Some(tab.id);
    let mut parts: Vec<&str> = Vec::new();
    if !url.is_empty() {
        parts.push(&url);
    }
    if let Some(tag) = suffix {
        parts.push(tag);
    }
    if is_active {
        parts.push("active");
    }
    parts.join(" · ")
}

fn tab_picker_item(tab: &Tab, active_tab: Option<TabId>, section: &str, suffix: Option<&str>) -> PickerItem {
    let mut item = PickerItem::new(
        format!("tab.{}", tab.id.0),
        tab.display_title(),
        "address_bar.switch_to_tab",
    )
    .section(section)
    .arg("tab_id", tab.id.0.to_string());

    let detail = tab_picker_detail(tab, active_tab, suffix);
    if !detail.is_empty() {
        item = item.detail(detail);
    }
    if let Some(icon) = &tab.favicon {
        item = item.icon(icon.clone());
    } else {
        item = item.icon("globe");
    }
    item
}

/// Builds the built-in `"tabs"` list picker from the current [`BrowserModel`].
///
/// Groups tabs into `"Open Tabs"`, `"Pinned"`, and `"Essentials"`, matching Zendia's reference picker.
pub fn build_tabs_picker(model: &BrowserModel) -> PickerRequest {
    let mut items = Vec::new();
    let active_tab = model.active_tab;

    if let Some(space_id) = model.active_space {
        // Normal tabs in the active space (most recent / active first if present, matching Zen's sidebar order).
        for tab in model.normal_tabs_in_space(space_id) {
            items.push(tab_picker_item(tab, active_tab, "Open Tabs", None));
        }
        for folder in model.folders_in_space(space_id) {
            for tab in model.folder_tabs(folder.id) {
                if !model.is_glance_tab(tab.id) {
                    items.push(tab_picker_item(tab, active_tab, "Open Tabs", Some(&folder.name)));
                }
            }
        }
        for tab in model.pinned_tabs_in_space(space_id) {
            items.push(tab_picker_item(tab, active_tab, "Pinned", Some("pinned")));
        }
    } else {
        for tab in &model.tabs {
            if !model.is_glance_tab(tab.id) && tab.kind == crate::model::TabKind::Normal {
                items.push(tab_picker_item(tab, active_tab, "Open Tabs", None));
            }
        }
        for tab in &model.tabs {
            if !model.is_glance_tab(tab.id) && tab.kind == crate::model::TabKind::Pinned {
                items.push(tab_picker_item(tab, active_tab, "Pinned", Some("pinned")));
            }
        }
    }

    for tab in model.essential_tabs() {
        items.push(tab_picker_item(tab, active_tab, "Essentials", Some("essential")));
    }

    let initial_selected = active_tab
        .and_then(|id| {
            let target = format!("tab.{}", id.0);
            items.iter().position(|item| item.id == target)
        })
        .unwrap_or(0);

    PickerRequest::new("builtin.tabs", "Tabs", PickerLayout::List, items).initial_selected(initial_selected)
}

/// Builds the built-in `"agents"` grid picker showing installed and dimmed uninstalled agents.
pub fn build_agents_picker(model: &BrowserModel) -> PickerRequest {
    struct AgentSpec {
        id: &'static str,
        name: &'static str,
        icon: &'static str,
        version: Option<&'static str>,
        accent_rgb: (u8, u8, u8),
        icon_fill: bool,
        install_hint: &'static str,
    }

    const CATALOG: &[AgentSpec] = &[
        AgentSpec {
            id: "claude-code",
            name: "Claude Code",
            icon: "agent-claude",
            version: Some("1.0.24"),
            accent_rgb: (217, 119, 87),
            icon_fill: false,
            install_hint: "npm i -g @anthropic-ai/claude-code",
        },
        AgentSpec {
            id: "codex",
            name: "Codex",
            icon: "agent-codex",
            version: Some("0.9.1"),
            accent_rgb: (66, 133, 244),
            icon_fill: false,
            install_hint: "npm i -g @openai/codex",
        },
        AgentSpec {
            id: "cursor",
            name: "Cursor Agent",
            icon: "agent-cursor",
            version: None,
            accent_rgb: (163, 163, 163),
            icon_fill: false,
            install_hint: "curl https://cursor.com/install -fsS | bash",
        },
        AgentSpec {
            id: "antigravity",
            name: "Antigravity",
            icon: "agent-antigravity",
            version: Some("2.0.0"),
            accent_rgb: (66, 133, 244),
            icon_fill: false,
            install_hint: "Install Antigravity CLI",
        },
        AgentSpec {
            id: "opencode",
            name: "OpenCode",
            icon: "agent-opencode",
            version: None,
            accent_rgb: (245, 167, 66),
            icon_fill: false,
            install_hint: "npm i -g opencode-ai",
        },
        AgentSpec {
            id: "kiro",
            name: "Kiro",
            icon: "agent-kiro",
            version: None,
            accent_rgb: (144, 70, 255),
            icon_fill: true,
            install_hint: "Install Kiro CLI",
        },
    ];

    let current_agent_id = model
        .conversations
        .first()
        .map(|conv| conv.agent_id.0.as_str())
        .or_else(|| model.agents.first().map(|agent| agent.id.0.as_str()))
        .unwrap_or("claude-code");

    let items: Vec<PickerItem> = CATALOG
        .iter()
        .map(|spec| {
            let model_override = model.agents.iter().find(|a| a.id.0 == spec.id);
            let installed = model_override.map(|a| a.available).unwrap_or(spec.version.is_some());
            let is_current = installed && spec.id == current_agent_id;
            let detail = if installed {
                let ver = spec.version.unwrap_or("1.0.0");
                if is_current {
                    format!("{ver} · current")
                } else {
                    ver.to_string()
                }
            } else {
                "Not installed".to_string()
            };

            let mut item = PickerItem::new(format!("agent.{}", spec.id), spec.name, "panel.toggle")
                .detail(detail)
                .icon(spec.icon)
                .accent_rgb(spec.accent_rgb)
                .icon_fill(spec.icon_fill)
                .enabled(installed)
                .dimmed(!installed)
                .arg("panel_id", "zendia.agent");

            if !installed {
                item = item.tooltip(spec.install_hint);
            }
            item
        })
        .collect();

    PickerRequest::new("builtin.agents", "Agents", PickerLayout::Grid, items)
}

/// Resolves a built-in picker name (`"tabs"`, `"agents"`) into a ready-to-display [`PickerRequest`].
pub fn build_builtin_picker(name: &str, model: &BrowserModel) -> Option<PickerRequest> {
    match name.trim().to_ascii_lowercase().as_str() {
        "tabs" => Some(build_tabs_picker(model)),
        "agents" => Some(build_agents_picker(model)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::sample_model;

    #[test]
    fn list_and_grid_key_badge_assignment_respects_reserved_and_movement_keys() {
        let mut list_items = vec![
            PickerItem::new("1", "First", "noop").key("s"),
            PickerItem::new("2", "Second", "noop"),
            PickerItem::new("3", "Third", "noop"),
        ];
        assign_key_badges(&mut list_items, PickerLayout::List);
        assert_eq!(list_items[0].key.as_deref(), Some("s"));
        assert_eq!(list_items[1].key.as_deref(), Some("a"));
        assert_eq!(list_items[2].key.as_deref(), Some("d"));

        let mut grid_items: Vec<PickerItem> = (0..6)
            .map(|i| PickerItem::new(format!("g{i}"), format!("Item {i}"), "noop"))
            .collect();
        assign_key_badges(&mut grid_items, PickerLayout::Grid);
        let keys: Vec<&str> = grid_items.iter().filter_map(|i| i.key.as_deref()).collect();
        assert_eq!(keys, vec!["a", "s", "d", "f", "g", "q"]);
        assert!(!keys.contains(&"h") && !keys.contains(&"j") && !keys.contains(&"k") && !keys.contains(&"l"));
    }

    #[test]
    fn tabs_picker_builds_sections_from_model_and_filters_by_text() {
        let model = sample_model(true);
        let picker = build_tabs_picker(&model);
        assert_eq!(picker.title, "Tabs");
        assert_eq!(picker.layout, PickerLayout::List);
        assert!(picker.sections.contains(&"Open Tabs".to_string()));
        assert!(picker.sections.contains(&"Pinned".to_string()));
        assert!(picker.sections.contains(&"Essentials".to_string()));
        assert!(!picker.items.is_empty());

        let filtered = picker.filtered_items("architecture");
        assert!(!filtered.is_empty());
        assert!(filtered.iter().all(|item| {
            item.title.to_ascii_lowercase().contains("architecture")
                || item
                    .detail
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains("architecture")
        }));

        let empty = picker.filtered_items("nonexistent-xyz-query");
        assert!(empty.is_empty());
    }

    #[test]
    fn agents_picker_has_grid_layout_with_enabled_and_dimmed_items() {
        let model = sample_model(true);
        let picker = build_agents_picker(&model);
        assert_eq!(picker.title, "Agents");
        assert_eq!(picker.layout, PickerLayout::Grid);
        assert_eq!(picker.items.len(), 6);
        assert!(picker.items.iter().any(|item| item.enabled && !item.dimmed));
        assert!(picker.items.iter().any(|item| !item.enabled && item.dimmed));
    }

    #[test]
    fn cursor_stepping_handles_list_and_grid_wrapping() {
        // List wraps cleanly.
        assert_eq!(step_picker_cursor(0, 4, PickerLayout::List, 1, 0, -1), Some(3));
        assert_eq!(step_picker_cursor(3, 4, PickerLayout::List, 1, 0, 1), Some(0));
        assert_eq!(step_picker_cursor(0, 0, PickerLayout::List, 1, 0, 1), None);

        // 3x2 grid (6 items): h/l move horizontally, j/k move across rows in same column.
        assert_eq!(step_picker_cursor(0, 6, PickerLayout::Grid, 3, 1, 0), Some(1));
        assert_eq!(step_picker_cursor(0, 6, PickerLayout::Grid, 3, -1, 0), Some(5));
        assert_eq!(step_picker_cursor(1, 6, PickerLayout::Grid, 3, 0, 1), Some(4));
        assert_eq!(step_picker_cursor(4, 6, PickerLayout::Grid, 3, 0, 1), Some(1));

        // Partial last row (5 items in 3 columns): column 2 only has row 0 (index 2).
        assert_eq!(step_picker_cursor(2, 5, PickerLayout::Grid, 3, 0, 1), Some(2));
        assert_eq!(step_picker_cursor(1, 5, PickerLayout::Grid, 3, 0, 1), Some(4));
    }

    #[test]
    fn grouping_preserves_contiguous_sections() {
        let items = vec![
            PickerItem::new("1", "A", "noop").section("One"),
            PickerItem::new("2", "B", "noop").section("One"),
            PickerItem::new("3", "C", "noop").section("Two"),
        ];
        let groups = group_items_by_section(&items);
        assert_eq!(
            groups,
            vec![
                (Some("One".to_string()), vec![0, 1]),
                (Some("Two".to_string()), vec![2]),
            ]
        );
    }
}
