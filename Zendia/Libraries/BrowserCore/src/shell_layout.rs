/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Pure shell layout rules, compact-mode normalization, sidebar geometry/clamping, sidebar/toolbar context menu
//! construction, and profile persistence for layout and side-panel preferences.

use crate::agent::{AGENTS_PANEL_ID, SidePanelPrefs, WEBCHAT_PANEL_DEFAULT_WIDTH};
use crate::contributions::{ContributionId, MenuEntryKind, SidePanelId, SidePanelLayout, SidePanelSide};
use crate::menus::ResolvedMenuItem;
use crate::model::{
    BrowserModel, CompactHide, LayoutSettings, SIDEBAR_COLLAPSED_WIDTH, SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MAX_WIDTH,
    SIDEBAR_MIN_WIDTH, SidebarSide, ToolbarLayout,
};
use crate::webchat::WEBCHAT_PANEL_ID;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use zendia_keymap::KeyTable;

/// Minimum expanded sidebar width when Single Toolbar + Right Sidebar places window controls in the sidebar top row
/// (matching Zen's `117 + captionButtonsWidth`).
pub const SIDEBAR_RIGHT_SINGLE_MIN_WIDTH: f32 = 231.5;

/// Minimum logical page width preserved when clamping a wide sidebar inside a narrow browser window.
pub const NARROW_WINDOW_MIN_PAGE_WIDTH: f32 = 360.0;

/// Dragging the sidebar resize handle narrower than this threshold collapses into compact mode.
pub const SIDEBAR_DRAG_COMPACT_THRESHOLD: f32 = 76.0;

/// Returns whether `Hide Toolbar Only` is illegal for `layout` on Linux/Windows (where window caption buttons sit on
/// the right edge, making `Collapsed + Right + Hide Toolbar Only` unreachable).
pub fn is_illegal_compact_toolbar_only(layout: &LayoutSettings) -> bool {
    !layout.sidebar_expanded && layout.sidebar_side == SidebarSide::Right
}

/// Normalizes `layout` so illegal compact states cannot persist (`Collapsed + Right + Toolbar` falls back to
/// `CompactHide::Sidebar`).
pub fn normalize_compact_layout(layout: &mut LayoutSettings) {
    if is_illegal_compact_toolbar_only(layout) && layout.compact_hide == CompactHide::Toolbar {
        layout.compact_hide = CompactHide::Sidebar;
    }
}

/// Whether compact mode currently hides the sidebar for `layout`.
///
/// In Single Toolbar layout there is no top toolbar, so enabling compact mode always hides the sidebar regardless of
/// `compact_hide`.
pub fn effective_compact_hides_sidebar(layout: &LayoutSettings) -> bool {
    layout.compact_mode
        && (matches!(layout.compact_hide, CompactHide::Both | CompactHide::Sidebar) || !layout.has_top_toolbar())
}

/// Whether compact mode currently hides the top toolbar for `layout`.
pub fn effective_compact_hides_toolbar(layout: &LayoutSettings) -> bool {
    layout.compact_mode
        && layout.has_top_toolbar()
        && matches!(layout.compact_hide, CompactHide::Both | CompactHide::Toolbar)
}

/// Whether single-toolbar compact mode shows the collapsible compact top bar holding Agents, Chat, and window controls
/// (matching `hasCompactTopBar` in `ZendiaChatButton.sys.mjs`).
pub fn effective_has_compact_top_bar(layout: &LayoutSettings) -> bool {
    layout.compact_mode && !layout.has_top_toolbar()
}

/// Returns the minimum expanded sidebar width for `layout`, widening when window caption buttons share the sidebar's
/// top row (`Single + Right`).
pub fn min_sidebar_width_for_layout(layout: &LayoutSettings) -> f32 {
    // Zen keeps this width in compact mode too, so the floating sidebar is as wide as the docked one.
    if !layout.has_top_toolbar() && layout.sidebar_side == SidebarSide::Right {
        SIDEBAR_RIGHT_SINGLE_MIN_WIDTH
    } else {
        SIDEBAR_MIN_WIDTH
    }
}

/// Clamps a candidate expanded sidebar width for `layout`.
pub fn clamp_sidebar_width_for_layout(layout: &LayoutSettings, width: f32) -> f32 {
    let min_w = min_sidebar_width_for_layout(layout);
    if width.is_finite() {
        width.clamp(min_w, SIDEBAR_MAX_WIDTH)
    } else {
        SIDEBAR_DEFAULT_WIDTH.max(min_w)
    }
}

/// Returns the displayed sidebar width constrained so a narrow window always leaves usable page width.
pub fn effective_sidebar_width_for_window(layout: &LayoutSettings, window_width: f32) -> f32 {
    if !layout.sidebar_expanded {
        return SIDEBAR_COLLAPSED_WIDTH;
    }
    let desired = clamp_sidebar_width_for_layout(layout, layout.sidebar_width);
    if !window_width.is_finite() || window_width <= 0.0 {
        return desired;
    }
    let min_w = min_sidebar_width_for_layout(layout);
    let max_for_window = (window_width - NARROW_WINDOW_MIN_PAGE_WIDTH).max(min_w);
    desired.min(max_for_window)
}

/// Returns whether dragging the resize handle to raw candidate width `raw_width` should snap into compact mode.
pub fn should_drag_trigger_compact(raw_width: f32) -> bool {
    raw_width.is_finite() && raw_width < SIDEBAR_DRAG_COMPACT_THRESHOLD
}

/// Builds the context menu shown when right-clicking empty space in the sidebar or toolbar.
pub fn build_sidebar_context_menu(model: &BrowserModel, key_table: Option<&KeyTable>) -> Vec<ResolvedMenuItem> {
    let layout = &model.layout;
    let is_single = !layout.has_top_toolbar();
    let illegal_toolbar_only = is_illegal_compact_toolbar_only(layout);

    let shortcut_for = |cmd: &str| -> Option<String> {
        key_table.and_then(|table| crate::menus::shortcut_hint_from_table(table, cmd, &[]))
    };

    let mut items = Vec::new();

    if is_single {
        let mut toggle = ResolvedMenuItem::action(
            "zendia.sidebar_ctx.compact_toggle",
            "Compact Mode",
            "layout.toggle_compact_mode",
        );
        toggle.kind = MenuEntryKind::Toggle {
            checked: layout.compact_mode,
        };
        toggle.shortcut_hint = shortcut_for("layout.toggle_compact_mode");
        items.push(toggle);
    } else {
        let mut enable = ResolvedMenuItem::action(
            "zendia.sidebar_ctx.compact_enable",
            "Enable Compact Mode",
            "layout.toggle_compact_mode",
        );
        enable.kind = MenuEntryKind::Toggle {
            checked: layout.compact_mode,
        };
        enable.shortcut_hint = shortcut_for("layout.toggle_compact_mode");

        let mut hide_sidebar = ResolvedMenuItem::action(
            "zendia.sidebar_ctx.compact_hide_sidebar",
            "Hide Tab Bar",
            "layout.set_compact_hide",
        );
        hide_sidebar.command_args = vec![("hide".to_string(), "sidebar".to_string())];
        hide_sidebar.kind = MenuEntryKind::Toggle {
            checked: layout.compact_hide == CompactHide::Sidebar,
        };

        let mut hide_toolbar = ResolvedMenuItem::action(
            "zendia.sidebar_ctx.compact_hide_toolbar",
            "Hide Top Bar",
            "layout.set_compact_hide",
        );
        hide_toolbar.command_args = vec![("hide".to_string(), "toolbar".to_string())];
        hide_toolbar.kind = MenuEntryKind::Toggle {
            checked: layout.compact_hide == CompactHide::Toolbar,
        };
        hide_toolbar.enabled = !illegal_toolbar_only;

        let mut hide_both = ResolvedMenuItem::action(
            "zendia.sidebar_ctx.compact_hide_both",
            "Hide Both",
            "layout.set_compact_hide",
        );
        hide_both.command_args = vec![("hide".to_string(), "both".to_string())];
        hide_both.kind = MenuEntryKind::Toggle {
            checked: layout.compact_hide == CompactHide::Both,
        };

        items.push(ResolvedMenuItem {
            id: ContributionId::new("zendia.sidebar_ctx.compact_menu"),
            label: "Compact Mode".to_string(),
            icon: Some("sidebar".to_string()),
            command: String::new(),
            command_args: Vec::new(),
            shortcut_hint: None,
            badge: None,
            kind: MenuEntryKind::Submenu,
            enabled: true,
            keeps_menu_open: false,
            children: vec![
                enable,
                ResolvedMenuItem::separator("zendia.sidebar_ctx.compact_sep"),
                hide_sidebar,
                hide_toolbar,
                hide_both,
            ],
        });
    }

    let mut right_side = ResolvedMenuItem::action(
        "zendia.sidebar_ctx.tabs_on_right",
        "Tabs on Right",
        "sidebar.toggle_side",
    );
    right_side.kind = MenuEntryKind::Toggle {
        checked: layout.sidebar_side == SidebarSide::Right,
    };
    items.push(right_side);

    items.push(ResolvedMenuItem::separator("zendia.sidebar_ctx.sep_actions"));

    let mut new_tab = ResolvedMenuItem::action("zendia.sidebar_ctx.new_tab", "New Tab", "tab.new");
    new_tab.icon = Some("plus".to_string());
    new_tab.shortcut_hint = shortcut_for("address_bar.open");
    items.push(new_tab);

    let mut new_folder = ResolvedMenuItem::action("zendia.sidebar_ctx.new_folder", "New Folder", "folder.create");
    new_folder.icon = Some("folder".to_string());
    items.push(new_folder);

    let mut new_space =
        ResolvedMenuItem::action("zendia.sidebar_ctx.new_space", "New Space…", "space.create_form.open");
    new_space.icon = Some("sparkles".to_string());
    items.push(new_space);

    items.push(ResolvedMenuItem::separator("zendia.sidebar_ctx.sep_settings"));

    let mut settings = ResolvedMenuItem::action("zendia.sidebar_ctx.settings", "Settings…", "settings.open");
    settings.icon = Some("settings".to_string());
    settings.shortcut_hint = shortcut_for("settings.open");
    items.push(settings);

    items
}

/// Persisted snapshot of layout and side panel settings stored in the user's profile config folder.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShellLayoutRecord {
    pub layout: LayoutSettings,
    pub panel_prefs: BTreeMap<SidePanelId, SidePanelPrefs>,
}

/// Returns the default config file path (`$XDG_CONFIG_HOME/zendia/layout.conf`).
pub fn shell_layout_profile_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("zendia").join("layout.conf"));
    }
    std::env::var("HOME")
        .ok()
        .filter(|h| !h.is_empty())
        .map(|home| PathBuf::from(home).join(".config").join("zendia").join("layout.conf"))
}

fn parse_panel_layout(raw: &str) -> Option<SidePanelLayout> {
    match raw {
        "float" => Some(SidePanelLayout::Float),
        "slide" => Some(SidePanelLayout::Slide),
        "split" => Some(SidePanelLayout::Split),
        _ => None,
    }
}

fn panel_layout_str(layout: SidePanelLayout) -> &'static str {
    match layout {
        SidePanelLayout::Float => "float",
        SidePanelLayout::Slide => "slide",
        SidePanelLayout::Split => "split",
    }
}

fn parse_panel_side(raw: &str) -> Option<SidePanelSide> {
    match raw {
        "left" => Some(SidePanelSide::Left),
        "right" => Some(SidePanelSide::Right),
        _ => None,
    }
}

fn panel_side_str(side: SidePanelSide) -> &'static str {
    match side {
        SidePanelSide::Left => "left",
        SidePanelSide::Right => "right",
    }
}

/// Loads persisted shell layout & panel preferences from `path`.
pub fn load_shell_layout_from(path: &Path) -> Option<ShellLayoutRecord> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut record = ShellLayoutRecord::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "toolbar_layout" => {
                record.layout.toolbar_layout = match value {
                    "single" => ToolbarLayout::Single,
                    "multiple" => ToolbarLayout::Multiple,
                    "collapsed" => ToolbarLayout::Collapsed,
                    _ => record.layout.toolbar_layout,
                };
            }
            "sidebar_side" => {
                record.layout.sidebar_side = match value {
                    "left" => SidebarSide::Left,
                    "right" => SidebarSide::Right,
                    _ => record.layout.sidebar_side,
                };
            }
            "sidebar_expanded" => {
                if let Ok(b) = value.parse::<bool>() {
                    record.layout.sidebar_expanded = b;
                }
            }
            "sidebar_width" => {
                if let Ok(w) = value.parse::<f32>() {
                    record.layout.sidebar_width = LayoutSettings::clamp_sidebar_width(w);
                }
            }
            "compact_mode" => {
                if let Ok(b) = value.parse::<bool>() {
                    record.layout.compact_mode = b;
                }
            }
            "compact_hide" => {
                if let Some(hide) = CompactHide::parse(value) {
                    record.layout.compact_hide = hide;
                }
            }
            "tab_switch_motion" => {
                if let Ok(b) = value.parse::<bool>() {
                    record.layout.tab_switch_motion = b;
                }
            }
            other if other.starts_with("panel.") => {
                let rest = &other["panel.".len()..];
                let Some((id_str, field)) = rest.rsplit_once('.') else {
                    continue;
                };
                let panel_id = SidePanelId::new(id_str);
                let entry = record
                    .panel_prefs
                    .entry(panel_id.clone())
                    .or_insert_with(|| SidePanelPrefs::default_for(id_str));
                match field {
                    "layout" => {
                        if let Some(l) = parse_panel_layout(value) {
                            entry.layout = l;
                        }
                    }
                    "side" => {
                        if let Some(s) = parse_panel_side(value) {
                            entry.side = s;
                        }
                    }
                    "width" => {
                        if let Ok(w) = value.parse::<f32>() {
                            entry.width = SidePanelPrefs::clamp_width_for(id_str, w);
                        }
                    }
                    "split_fallback" => {
                        if let Some(f) = parse_panel_layout(value) {
                            entry.split_fallback = f;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    record.layout.sidebar_expanded = !matches!(record.layout.toolbar_layout, ToolbarLayout::Collapsed);
    normalize_compact_layout(&mut record.layout);
    Some(record)
}

/// Saves `record` to `path`, creating parent directories when needed.
pub fn save_shell_layout_to(path: &Path, record: &ShellLayoutRecord) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toolbar_str = match record.layout.toolbar_layout {
        ToolbarLayout::Single => "single",
        ToolbarLayout::Multiple => "multiple",
        ToolbarLayout::Collapsed => "collapsed",
    };
    let side_str = match record.layout.sidebar_side {
        SidebarSide::Left => "left",
        SidebarSide::Right => "right",
    };
    let mut out = String::new();
    out.push_str(&format!("toolbar_layout={toolbar_str}\n"));
    out.push_str(&format!("sidebar_side={side_str}\n"));
    out.push_str(&format!("sidebar_expanded={}\n", record.layout.sidebar_expanded));
    out.push_str(&format!("sidebar_width={:.1}\n", record.layout.sidebar_width));
    out.push_str(&format!("compact_mode={}\n", record.layout.compact_mode));
    out.push_str(&format!("compact_hide={}\n", record.layout.compact_hide.as_str()));
    out.push_str(&format!("tab_switch_motion={}\n", record.layout.tab_switch_motion));
    for (panel_id, prefs) in &record.panel_prefs {
        let id = panel_id.as_str();
        out.push_str(&format!("panel.{id}.layout={}\n", panel_layout_str(prefs.layout)));
        out.push_str(&format!("panel.{id}.side={}\n", panel_side_str(prefs.side)));
        out.push_str(&format!("panel.{id}.width={:.1}\n", prefs.width));
        out.push_str(&format!(
            "panel.{id}.split_fallback={}\n",
            panel_layout_str(prefs.split_fallback)
        ));
    }
    std::fs::write(path, out)
}

/// Returns the default width for any built-in panel ID (matching fork defaults).
pub fn builtin_panel_default_width(panel_id: &str) -> f32 {
    match panel_id {
        AGENTS_PANEL_ID => crate::agent::AGENTS_PANEL_DEFAULT_WIDTH,
        WEBCHAT_PANEL_ID => WEBCHAT_PANEL_DEFAULT_WIDTH,
        _ => crate::agent::PANEL_DEFAULT_WIDTH,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::sample_model;

    #[test]
    fn single_toolbar_compact_always_hides_sidebar_never_toolbar() {
        let layout = LayoutSettings {
            toolbar_layout: ToolbarLayout::Single,
            compact_mode: true,
            compact_hide: CompactHide::Toolbar,
            ..LayoutSettings::default()
        };
        assert!(effective_compact_hides_sidebar(&layout));
        assert!(!effective_compact_hides_toolbar(&layout));
        assert!(effective_has_compact_top_bar(&layout));

        let single_normal = LayoutSettings {
            toolbar_layout: ToolbarLayout::Single,
            compact_mode: false,
            ..LayoutSettings::default()
        };
        assert!(!effective_has_compact_top_bar(&single_normal));

        let multiple_compact = LayoutSettings {
            toolbar_layout: ToolbarLayout::Multiple,
            compact_mode: true,
            ..LayoutSettings::default()
        };
        assert!(!effective_has_compact_top_bar(&multiple_compact));
    }

    #[test]
    fn illegal_collapsed_right_toolbar_only_normalizes_to_sidebar() {
        let mut layout = LayoutSettings {
            toolbar_layout: ToolbarLayout::Collapsed,
            sidebar_expanded: false,
            sidebar_side: SidebarSide::Right,
            compact_mode: true,
            compact_hide: CompactHide::Toolbar,
            ..LayoutSettings::default()
        };
        assert!(is_illegal_compact_toolbar_only(&layout));
        normalize_compact_layout(&mut layout);
        assert_eq!(layout.compact_hide, CompactHide::Sidebar);
    }

    #[test]
    fn sidebar_min_width_widens_for_single_right_and_caps_for_narrow_windows() {
        let single_right = LayoutSettings {
            toolbar_layout: ToolbarLayout::Single,
            sidebar_side: SidebarSide::Right,
            sidebar_expanded: true,
            sidebar_width: 186.0,
            ..LayoutSettings::default()
        };
        assert_eq!(
            min_sidebar_width_for_layout(&single_right),
            SIDEBAR_RIGHT_SINGLE_MIN_WIDTH
        );
        assert_eq!(
            clamp_sidebar_width_for_layout(&single_right, 170.0),
            SIDEBAR_RIGHT_SINGLE_MIN_WIDTH
        );

        let wide_left = LayoutSettings {
            toolbar_layout: ToolbarLayout::Multiple,
            sidebar_side: SidebarSide::Left,
            sidebar_expanded: true,
            sidebar_width: 500.0,
            ..LayoutSettings::default()
        };
        // In a 700px window, 700 - 360 = 340px max so page keeps 360px.
        assert_eq!(effective_sidebar_width_for_window(&wide_left, 700.0), 340.0);
        assert_eq!(effective_sidebar_width_for_window(&wide_left, 1600.0), 500.0);
    }

    #[test]
    fn sidebar_context_menu_adapts_to_single_vs_multiple_and_illegal_state() {
        let mut model = sample_model(true);
        model.layout.toolbar_layout = ToolbarLayout::Single;
        let single_menu = build_sidebar_context_menu(&model, None);
        assert!(single_menu.iter().any(|i| i.command == "layout.toggle_compact_mode"));
        assert!(single_menu.iter().any(|i| i.command == "sidebar.toggle_side"));

        model.layout.toolbar_layout = ToolbarLayout::Collapsed;
        model.layout.sidebar_expanded = false;
        model.layout.sidebar_side = SidebarSide::Right;
        let collapsed_right = build_sidebar_context_menu(&model, None);
        let compact_sub = collapsed_right
            .iter()
            .find(|i| i.id.as_str() == "zendia.sidebar_ctx.compact_menu")
            .expect("submenu present");
        let hide_toolbar = compact_sub
            .children
            .iter()
            .find(|c| c.id.as_str() == "zendia.sidebar_ctx.compact_hide_toolbar")
            .expect("hide toolbar option");
        assert!(!hide_toolbar.enabled, "illegal toolbar-only option must be disabled");
    }

    #[test]
    fn shell_layout_persistence_round_trips() {
        let dir = std::env::temp_dir().join(format!("zendia-shell-test-{}", std::process::id()));
        let file = dir.join("layout.conf");
        let mut record = ShellLayoutRecord {
            layout: LayoutSettings {
                toolbar_layout: ToolbarLayout::Multiple,
                sidebar_side: SidebarSide::Right,
                sidebar_expanded: true,
                sidebar_width: 248.0,
                compact_mode: true,
                compact_hide: CompactHide::Sidebar,
                tab_switch_motion: false,
            },
            panel_prefs: BTreeMap::new(),
        };
        record.panel_prefs.insert(
            SidePanelId::new(WEBCHAT_PANEL_ID),
            SidePanelPrefs {
                layout: SidePanelLayout::Split,
                side: SidePanelSide::Left,
                width: 500.0,
                split_fallback: SidePanelLayout::Slide,
            },
        );
        save_shell_layout_to(&file, &record).unwrap();
        let loaded = load_shell_layout_from(&file).unwrap();
        assert_eq!(loaded, record);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
