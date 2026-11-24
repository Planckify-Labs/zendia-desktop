/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Menu contribution builders, contribution merging/ordering, Ladybird-compatible page context menus, select dropdown
//! data, and About Zendia metadata.
//!
//! All types and functions here are pure data and independent of any UI toolkit.

use crate::contributions::{ContributionId, ContributionSource, MenuEntry, MenuEntryKind, MenuLocation};
use crate::model::{BrowserModel, FolderId, SpaceId, TabId, TabKind};
use std::collections::BTreeSet;
use zendia_keymap::{KeyModifiers, KeyTable};

/// Built-in widget identifier for the App Menu's zoom controls row (`- 100% + | fullscreen`).
pub const ZOOM_CONTROLS_WIDGET_ID: &str = "zendia.zoom_controls";

/// Built-in contribution ID for the App Menu's Vim mode switch row.
pub const APP_MENU_VIM_MODE_ID: &str = "zendia.app.vim_mode";

/// Preferred top-level group order for the application menu.
pub const APP_MENU_GROUPS: &[&str] = &["new", "library", "page", "zoom", "tools", "quit"];

/// Preferred top-level group order for the tab context menu.
pub const TAB_CONTEXT_GROUPS: &[&str] = &["zendia_ask", "organize", "tab_state", "manage", "close", "routing"];

/// Preferred top-level group order for the folder context menu.
pub const FOLDER_CONTEXT_GROUPS: &[&str] = &["edit", "organize", "danger"];

/// Preferred top-level group order for the space context menu.
pub const SPACE_CONTEXT_GROUPS: &[&str] = &["switch", "tabs", "manage"];

/// Preferred group order for contributed page context menu items.
pub const PAGE_CONTEXT_GROUPS: &[&str] = &["zendia"];

/// A single link shown in the About Zendia dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AboutLink {
    pub label: &'static str,
    pub url: &'static str,
}

/// Core-owned metadata rendered by the About Zendia dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AboutInfo {
    pub app_name: &'static str,
    pub version: &'static str,
    pub architecture: &'static str,
    pub engine_line: &'static str,
    pub community_prefix: &'static str,
    pub community_link_label: &'static str,
    pub community_link_url: &'static str,
    pub community_suffix: &'static str,
    pub primary_links: &'static [AboutLink],
    pub legal_links: &'static [AboutLink],
}

impl AboutInfo {
    /// Formatted version line shown below the application name (e.g. `Version 0.1.0 (64-bit)`).
    pub fn version_line(&self) -> String {
        format!("Version {} ({})", self.version, self.architecture)
    }

    /// Plain text copied to the clipboard when the user clicks the version badge.
    pub fn clipboard_version(&self) -> String {
        format!(
            "{} {} ({}; {})",
            self.app_name, self.version, self.architecture, self.engine_line
        )
    }
}

const ABOUT_PRIMARY_LINKS: &[AboutLink] = &[
    AboutLink {
        label: "What's new",
        url: "https://github.com/Planckify-Labs/zendia/releases",
    },
    AboutLink {
        label: "Donate",
        url: "https://github.com/sponsors/Planckify-Labs",
    },
    AboutLink {
        label: "Report an issue",
        url: "https://github.com/Planckify-Labs/zendia/issues",
    },
];

const ABOUT_LEGAL_LINKS: &[AboutLink] = &[
    AboutLink {
        label: "License",
        url: "https://github.com/Planckify-Labs/zendia/blob/master/Zendia/LICENSE",
    },
    AboutLink {
        label: "Your rights",
        url: "about:version",
    },
    AboutLink {
        label: "Privacy",
        url: "https://github.com/Planckify-Labs/zendia",
    },
];

/// Returns the metadata displayed by the About Zendia dialog.
pub fn about_info() -> AboutInfo {
    let architecture = if cfg!(target_pointer_width = "64") {
        "64-bit"
    } else {
        "32-bit"
    };
    AboutInfo {
        app_name: "Zendia",
        version: env!("CARGO_PKG_VERSION"),
        architecture,
        engine_line: "Based on Ladybird",
        community_prefix: "Zendia is free and open source, built on Ladybird by a ",
        community_link_label: "community of contributors",
        community_link_url: "https://github.com/Planckify-Labs/zendia",
        community_suffix: ".",
        primary_links: ABOUT_PRIMARY_LINKS,
        legal_links: ABOUT_LEGAL_LINKS,
    }
}

/// A resolved menu entry ready for a UI view to draw (with separators inserted and submenus nested).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedMenuItem {
    pub id: ContributionId,
    pub label: String,
    pub icon: Option<String>,
    pub command: String,
    pub command_args: Vec<(String, String)>,
    pub shortcut_hint: Option<String>,
    pub badge: Option<String>,
    pub kind: MenuEntryKind,
    pub enabled: bool,
    pub keeps_menu_open: bool,
    pub children: Vec<ResolvedMenuItem>,
}

impl ResolvedMenuItem {
    pub fn separator(id: impl Into<String>) -> Self {
        Self {
            id: ContributionId::new(id),
            label: String::new(),
            icon: None,
            command: String::new(),
            command_args: Vec::new(),
            shortcut_hint: None,
            badge: None,
            kind: MenuEntryKind::Separator,
            enabled: false,
            keeps_menu_open: false,
            children: Vec::new(),
        }
    }

    pub fn action(id: impl Into<String>, label: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            id: ContributionId::new(id),
            label: label.into(),
            icon: None,
            command: command.into(),
            command_args: Vec::new(),
            shortcut_hint: None,
            badge: None,
            kind: MenuEntryKind::Action,
            enabled: true,
            keeps_menu_open: false,
            children: Vec::new(),
        }
    }

    pub fn with_arg(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.command_args.push((key.into(), value.into()));
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_kind(mut self, kind: MenuEntryKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn with_shortcut(mut self, shortcut: Option<String>) -> Self {
        self.shortcut_hint = shortcut;
        self
    }

    pub fn is_separator(&self) -> bool {
        matches!(self.kind, MenuEntryKind::Separator)
    }

    pub fn is_submenu(&self) -> bool {
        matches!(self.kind, MenuEntryKind::Submenu)
    }
}

/// Looks up the best human-readable shortcut hint for `(command, args)` in `key_table`.
///
/// Prefers bindings whose arguments match `command_args` exactly (for commands like `library.open` with
/// `section=downloads`) and prefers Ctrl/Alt shortcuts on Linux/Windows (Super on macOS).
pub fn shortcut_hint_from_table(
    key_table: &KeyTable,
    command: &str,
    command_args: &[(String, String)],
) -> Option<String> {
    if command.is_empty() {
        return None;
    }

    let prefer_super = cfg!(target_os = "macos");
    let mut best_exact: Option<(bool, String)> = None;
    let mut best_fallback: Option<(bool, String)> = None;

    for binding in key_table.bindings() {
        if binding.invocation.command != command {
            continue;
        }
        let is_super = binding
            .keys
            .strokes()
            .first()
            .is_some_and(|stroke| stroke.modifiers.contains(KeyModifiers::SUPER));
        let preferred_platform = is_super == prefer_super;
        let display = binding.keys.display_human();

        let args_match = binding.invocation.args.len() == command_args.len()
            && command_args.iter().all(|(key, value)| {
                binding
                    .invocation
                    .args
                    .get(key)
                    .is_some_and(|bound| bound.to_arg_string() == *value)
            });

        if args_match {
            if preferred_platform {
                return Some(display);
            }
            if best_exact.is_none() {
                best_exact = Some((preferred_platform, display));
            }
        } else if command_args.is_empty()
            && best_fallback
                .as_ref()
                .is_none_or(|(pref, _)| !*pref && preferred_platform)
        {
            best_fallback = Some((preferred_platform, display));
        }
    }

    best_exact.or(best_fallback).map(|(_, hint)| hint)
}

/// Resolves all contributed [`MenuEntry`] items for `location` into a grouped, ordered, nested menu tree.
///
/// Groups listed in `group_order` appear first in that order; any additional groups contributed by plugins or
/// extensions follow in deterministic alphabetical order. Separators are inserted between non-empty groups and
/// collapsed so no leading, trailing, or consecutive separators are ever emitted.
pub fn resolve_contributed_menu(
    entries: &[MenuEntry],
    location: MenuLocation,
    key_table: Option<&KeyTable>,
    group_order: &[&str],
) -> Vec<ResolvedMenuItem> {
    let scoped: Vec<&MenuEntry> = entries.iter().filter(|entry| entry.location == location).collect();
    resolve_menu_level(&scoped, None, key_table, group_order, 0)
}

fn resolve_menu_level(
    scoped: &[&MenuEntry],
    parent: Option<&ContributionId>,
    key_table: Option<&KeyTable>,
    group_order: &[&str],
    depth: usize,
) -> Vec<ResolvedMenuItem> {
    if depth > 4 {
        return Vec::new();
    }

    let level_entries: Vec<&MenuEntry> = scoped
        .iter()
        .copied()
        .filter(|entry| entry.parent.as_ref() == parent)
        .collect();
    if level_entries.is_empty() {
        return Vec::new();
    }

    let mut ordered_groups: Vec<String> = Vec::new();
    let mut seen_groups: BTreeSet<String> = BTreeSet::new();

    for &group in group_order {
        if level_entries.iter().any(|entry| entry.group == group) {
            ordered_groups.push(group.to_string());
            seen_groups.insert(group.to_string());
        }
    }
    let mut extra_groups: BTreeSet<&str> = BTreeSet::new();
    for entry in &level_entries {
        if !seen_groups.contains(&entry.group) {
            extra_groups.insert(entry.group.as_str());
        }
    }
    for group in extra_groups {
        ordered_groups.push(group.to_string());
    }

    let mut items = Vec::new();
    for (group_index, group_name) in ordered_groups.iter().enumerate() {
        let mut group_entries: Vec<&MenuEntry> = level_entries
            .iter()
            .copied()
            .filter(|entry| entry.group == *group_name)
            .collect();
        group_entries.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));

        if group_entries.is_empty() {
            continue;
        }
        if !items.is_empty() {
            items.push(ResolvedMenuItem::separator(format!(
                "sep.{}.{group_index}.{group_name}",
                parent.map_or("root", ContributionId::as_str)
            )));
        }

        for entry in group_entries {
            let children = if matches!(entry.kind, MenuEntryKind::Submenu) {
                resolve_menu_level(scoped, Some(&entry.id), key_table, &[], depth + 1)
            } else {
                Vec::new()
            };
            let shortcut_hint = key_table
                .and_then(|table| shortcut_hint_from_table(table, &entry.command, &entry.command_args))
                .or_else(|| entry.shortcut_hint.clone());
            items.push(ResolvedMenuItem {
                id: entry.id.clone(),
                label: entry.label.clone(),
                icon: entry.icon.clone(),
                command: entry.command.clone(),
                command_args: entry.command_args.clone(),
                shortcut_hint,
                badge: entry.badge.clone(),
                kind: entry.kind.clone(),
                enabled: entry.enabled,
                keeps_menu_open: entry.keeps_menu_open,
                children,
            });
        }
    }

    collapse_separators(items)
}

fn collapse_separators(items: Vec<ResolvedMenuItem>) -> Vec<ResolvedMenuItem> {
    let mut out: Vec<ResolvedMenuItem> = Vec::with_capacity(items.len());
    for item in items {
        if item.is_separator() && (out.is_empty() || out.last().is_some_and(ResolvedMenuItem::is_separator)) {
            continue;
        }
        out.push(item);
    }
    while out.last().is_some_and(ResolvedMenuItem::is_separator) {
        out.pop();
    }
    out
}

fn menu_entry(
    id: &str,
    location: MenuLocation,
    label: &str,
    icon: Option<&str>,
    command: &str,
    group: &str,
    order: i32,
) -> MenuEntry {
    MenuEntry {
        id: ContributionId::new(id),
        location,
        label: label.to_string(),
        icon: icon.map(str::to_string),
        command: command.to_string(),
        command_args: Vec::new(),
        shortcut_hint: None,
        group: group.to_string(),
        order,
        enabled: true,
        source: ContributionSource::BuiltIn,
        parent: None,
        kind: MenuEntryKind::Action,
        badge: None,
        keeps_menu_open: false,
    }
}

fn submenu_entry(
    id: &str,
    location: MenuLocation,
    label: &str,
    icon: Option<&str>,
    group: &str,
    order: i32,
) -> MenuEntry {
    MenuEntry {
        kind: MenuEntryKind::Submenu,
        keeps_menu_open: true,
        ..menu_entry(id, location, label, icon, "", group, order)
    }
}

#[allow(clippy::too_many_arguments)]
fn child_entry(
    id: &str,
    parent: &str,
    location: MenuLocation,
    label: &str,
    icon: Option<&str>,
    command: &str,
    group: &str,
    order: i32,
) -> MenuEntry {
    MenuEntry {
        parent: Some(ContributionId::new(parent)),
        ..menu_entry(id, location, label, icon, command, group, order)
    }
}

/// Built-in application menu contributions matching Zendia's menu layout (without Firefox account sync).
pub fn builtin_app_menu_entries(vim_mode_enabled: bool) -> Vec<MenuEntry> {
    let loc = MenuLocation::AppMenu;
    let mut entries = vec![
        // Group: new
        menu_entry("zendia.app.new_tab", loc, "New Tab", Some("tab"), "tab.new", "new", 10),
        menu_entry(
            "zendia.app.new_window",
            loc,
            "New Window",
            Some("window"),
            "window.new",
            "new",
            20,
        ),
        menu_entry(
            "zendia.app.new_blank_window",
            loc,
            "New Blank Window",
            Some("window"),
            "window.new_blank",
            "new",
            30,
        ),
        menu_entry(
            "zendia.app.new_private_window",
            loc,
            "New Private Window",
            Some("private-window"),
            "window.new_private",
            "new",
            40,
        ),
        // Group: library
        submenu_entry("zendia.app.history", loc, "History", Some("history"), "library", 10),
        submenu_entry(
            "zendia.app.bookmarks",
            loc,
            "Bookmarks",
            Some("bookmark-hollow"),
            "library",
            20,
        ),
        MenuEntry {
            command_args: vec![("section".to_string(), "downloads".to_string())],
            ..menu_entry(
                "zendia.app.downloads",
                loc,
                "Downloads",
                Some("downloads"),
                "library.open",
                "library",
                30,
            )
        },
        menu_entry(
            "zendia.app.passwords",
            loc,
            "Passwords",
            Some("passwords"),
            "passwords.open",
            "library",
            40,
        ),
        menu_entry(
            "zendia.app.extensions",
            loc,
            "Extensions and Themes",
            Some("extension"),
            "extensions.open",
            "library",
            50,
        ),
        // Group: page
        menu_entry(
            "zendia.app.print",
            loc,
            "Print...",
            Some("print"),
            "page.print",
            "page",
            10,
        ),
        menu_entry(
            "zendia.app.save_page",
            loc,
            "Save Page As...",
            Some("save"),
            "page.save",
            "page",
            20,
        ),
        menu_entry(
            "zendia.app.find",
            loc,
            "Find in Page...",
            Some("search-page"),
            "page.find",
            "page",
            30,
        ),
        // Group: zoom
        MenuEntry {
            kind: MenuEntryKind::Widget {
                widget_id: ZOOM_CONTROLS_WIDGET_ID.to_string(),
            },
            keeps_menu_open: true,
            ..menu_entry("zendia.app.zoom", loc, "Zoom", None, "", "zoom", 10)
        },
        // Group: tools
        MenuEntry {
            command_args: vec![("enabled".to_string(), (!vim_mode_enabled).to_string())],
            kind: MenuEntryKind::Toggle {
                checked: vim_mode_enabled,
            },
            keeps_menu_open: true,
            ..menu_entry(
                APP_MENU_VIM_MODE_ID,
                loc,
                "Vim mode",
                Some("vim"),
                "vim.set_enabled",
                "tools",
                10,
            )
        },
        menu_entry(
            "zendia.app.settings",
            loc,
            "Settings",
            Some("settings"),
            "settings.open",
            "tools",
            20,
        ),
        submenu_entry("zendia.app.more_tools", loc, "More Tools", Some("menu"), "tools", 30),
        submenu_entry("zendia.app.help", loc, "Help and Report", Some("help"), "tools", 40),
        // Group: quit
        menu_entry("zendia.app.quit", loc, "Quit", Some("close"), "app.quit", "quit", 10),
    ];

    // History submenu sample items
    entries.extend([
        child_entry(
            "zendia.app.history.recently_closed",
            "zendia.app.history",
            loc,
            "Reopen Closed Tab",
            Some("u-turn-to-left"),
            "tab.reopen_closed",
            "actions",
            10,
        ),
        MenuEntry {
            command_args: vec![("section".to_string(), "history".to_string())],
            ..child_entry(
                "zendia.app.history.manage",
                "zendia.app.history",
                loc,
                "Show All History",
                Some("history"),
                "library.open",
                "actions",
                20,
            )
        },
        child_entry(
            "zendia.app.history.clear",
            "zendia.app.history",
            loc,
            "Clear Recent History...",
            Some("trash"),
            "history.clear",
            "danger",
            30,
        ),
    ]);

    // Bookmarks submenu sample items
    entries.extend([
        child_entry(
            "zendia.app.bookmarks.add",
            "zendia.app.bookmarks",
            loc,
            "Bookmark Current Tab",
            Some("bookmark"),
            "bookmark.toggle",
            "actions",
            10,
        ),
        MenuEntry {
            command_args: vec![("section".to_string(), "bookmarks".to_string())],
            ..child_entry(
                "zendia.app.bookmarks.manage",
                "zendia.app.bookmarks",
                loc,
                "Manage Bookmarks",
                Some("bookmark-star-on-tray"),
                "library.open",
                "actions",
                20,
            )
        },
    ]);

    // More Tools submenu sample items
    entries.extend([
        child_entry(
            "zendia.app.more_tools.devtools",
            "zendia.app.more_tools",
            loc,
            "Developer Tools",
            Some("code"),
            "devtools.toggle",
            "dev",
            10,
        ),
        child_entry(
            "zendia.app.more_tools.view_source",
            "zendia.app.more_tools",
            loc,
            "View Page Source",
            Some("brackets-curly"),
            "page.view_source",
            "dev",
            20,
        ),
        child_entry(
            "zendia.app.more_tools.task_manager",
            "zendia.app.more_tools",
            loc,
            "Task Manager",
            Some("sliders"),
            "task_manager.open",
            "dev",
            30,
        ),
        child_entry(
            "zendia.app.more_tools.customize",
            "zendia.app.more_tools",
            loc,
            "Customize Toolbar...",
            Some("paintbrush"),
            "toolbar.customize",
            "ui",
            40,
        ),
    ]);

    // Help and Report submenu items
    entries.extend([
        child_entry(
            "zendia.app.help.about",
            "zendia.app.help",
            loc,
            "About Zendia",
            Some("info"),
            "app.about",
            "about",
            10,
        ),
        child_entry(
            "zendia.app.help.report_issue",
            "zendia.app.help",
            loc,
            "Report Website Issue...",
            Some("warning"),
            "page.report_issue",
            "support",
            20,
        ),
        child_entry(
            "zendia.app.help.shortcuts",
            "zendia.app.help",
            loc,
            "Keyboard Shortcuts",
            Some("vim"),
            "help.shortcuts",
            "support",
            30,
        ),
        child_entry(
            "zendia.app.help.release_notes",
            "zendia.app.help",
            loc,
            "Release Notes",
            Some("sparkles"),
            "help.release_notes",
            "support",
            40,
        ),
    ]);

    entries
}

/// Built-in tab context menu contributions matching Zendia's tab menu.
pub fn builtin_tab_context_entries(chat_site_name: &str, essentials_count: usize) -> Vec<MenuEntry> {
    let loc = MenuLocation::TabContext;
    let ask_label = format!("Ask {chat_site_name} About This Tab");

    vec![
        // Group: zendia_ask
        submenu_entry("zendia.tab.ask", loc, &ask_label, Some("sparkles"), "zendia_ask", 10),
        MenuEntry {
            command_args: vec![
                ("site_id".to_string(), "chatgpt".to_string()),
                ("prompt".to_string(), "Summarize this page in English".to_string()),
            ],
            ..child_entry(
                "zendia.tab.ask.summarize",
                "zendia.tab.ask",
                loc,
                "Summarize Page in English",
                None,
                "webchat.ask",
                "prompts",
                10,
            )
        },
        MenuEntry {
            command_args: vec![
                ("site_id".to_string(), "chatgpt".to_string()),
                (
                    "prompt".to_string(),
                    "List the key points and action items in English".to_string(),
                ),
            ],
            ..child_entry(
                "zendia.tab.ask.key_points",
                "zendia.tab.ask",
                loc,
                "Key Points and Action Items in English",
                None,
                "webchat.ask",
                "prompts",
                20,
            )
        },
        MenuEntry {
            command_args: vec![
                ("site_id".to_string(), "chatgpt".to_string()),
                ("prompt".to_string(), "Translate this page to English".to_string()),
            ],
            ..child_entry(
                "zendia.tab.ask.translate",
                "zendia.tab.ask",
                loc,
                "Translate Page to English",
                None,
                "webchat.ask",
                "prompts",
                30,
            )
        },
        MenuEntry {
            command_args: vec![
                ("site_id".to_string(), "chatgpt".to_string()),
                ("prompt".to_string(), "Ask about this tab".to_string()),
            ],
            ..child_entry(
                "zendia.tab.ask.custom",
                "zendia.tab.ask",
                loc,
                "Ask…",
                None,
                "webchat.ask",
                "prompts",
                40,
            )
        },
        MenuEntry {
            command_args: vec![
                ("site_id".to_string(), "chatgpt".to_string()),
                ("prompt".to_string(), "Summarize this page on every site".to_string()),
            ],
            ..child_entry(
                "zendia.tab.ask.every_site",
                "zendia.tab.ask",
                loc,
                "Summarize Page in English: Every Site",
                None,
                "webchat.ask",
                "all_sites",
                10,
            )
        },
        // Group: organize
        submenu_entry("zendia.tab.move_to_folder", loc, "Move to Folder", None, "organize", 10),
        child_entry(
            "zendia.tab.move_to_folder.new",
            "zendia.tab.move_to_folder",
            loc,
            "New Folder…",
            None,
            "folder.create",
            "create",
            10,
        ),
        submenu_entry("zendia.tab.move_to_space", loc, "Move to Space", None, "organize", 15),
        menu_entry(
            "zendia.tab.add_split",
            loc,
            "Add Split View...",
            None,
            "tab.add_split_view",
            "organize",
            20,
        ),
        menu_entry("zendia.tab.new", loc, "New Tab", None, "tab.new", "organize", 30),
        // Group: tab_state
        menu_entry(
            "zendia.tab.reload",
            loc,
            "Reload Tab",
            None,
            "navigation.reload",
            "tab_state",
            10,
        ),
        menu_entry(
            "zendia.tab.mute",
            loc,
            "Mute Tab",
            None,
            "tab.toggle_mute",
            "tab_state",
            20,
        ),
        menu_entry("zendia.tab.pin", loc, "Pin Tab", None, "tab.pin", "tab_state", 30),
        MenuEntry {
            badge: Some(format!("{essentials_count} / 12")),
            ..menu_entry(
                "zendia.tab.essentials",
                loc,
                "Add to Essentials",
                None,
                "tab.add_to_essentials",
                "tab_state",
                40,
            )
        },
        menu_entry(
            "zendia.tab.rename",
            loc,
            "Rename Tab",
            None,
            "tab.start_rename",
            "tab_state",
            45,
        ),
        menu_entry(
            "zendia.tab.duplicate",
            loc,
            "Duplicate Tab",
            None,
            "tab.duplicate",
            "tab_state",
            50,
        ),
        menu_entry(
            "zendia.tab.unload",
            loc,
            "Unload Tab",
            None,
            "tab.unload",
            "tab_state",
            60,
        ),
        // Group: manage
        menu_entry(
            "zendia.tab.bookmark",
            loc,
            "Bookmark Tab…",
            None,
            "tab.bookmark",
            "manage",
            10,
        ),
        submenu_entry("zendia.tab.move", loc, "Move Tab", None, "manage", 20),
        child_entry(
            "zendia.tab.move.start",
            "zendia.tab.move",
            loc,
            "Move to Start",
            None,
            "tab.move_to_start",
            "move",
            10,
        ),
        child_entry(
            "zendia.tab.move.end",
            "zendia.tab.move",
            loc,
            "Move to End",
            None,
            "tab.move_to_end",
            "move",
            20,
        ),
        child_entry(
            "zendia.tab.move.new_window",
            "zendia.tab.move",
            loc,
            "Move to New Window",
            None,
            "tab.move_to_new_window",
            "move",
            30,
        ),
        menu_entry(
            "zendia.tab.select_all",
            loc,
            "Select All Tabs",
            None,
            "tab.select_all",
            "manage",
            30,
        ),
        // Group: close
        menu_entry("zendia.tab.close", loc, "Close Tab", None, "tab.close", "close", 10),
        submenu_entry(
            "zendia.tab.close_multiple",
            loc,
            "Close Multiple Tabs",
            None,
            "close",
            20,
        ),
        child_entry(
            "zendia.tab.close_multiple.start",
            "zendia.tab.close_multiple",
            loc,
            "Close Tabs to Start",
            None,
            "tab.close_to_start",
            "close",
            10,
        ),
        child_entry(
            "zendia.tab.close_multiple.end",
            "zendia.tab.close_multiple",
            loc,
            "Close Tabs to End",
            None,
            "tab.close_to_end",
            "close",
            20,
        ),
        child_entry(
            "zendia.tab.close_multiple.others",
            "zendia.tab.close_multiple",
            loc,
            "Close Other Tabs",
            None,
            "tab.close_others",
            "close",
            30,
        ),
        menu_entry(
            "zendia.tab.reopen_closed",
            loc,
            "Reopen Closed Tab",
            None,
            "tab.reopen_closed",
            "close",
            30,
        ),
        // Group: routing
        menu_entry(
            "zendia.tab.route_domain",
            loc,
            "Add Route for Domain",
            None,
            "space_routing.add_domain",
            "routing",
            10,
        ),
    ]
}

/// Built-in Zendia contributions injected into Ladybird's page context menus (Ask, Split View, Glance).
pub fn builtin_page_context_entries(chat_site_name: &str) -> Vec<MenuEntry> {
    let loc = MenuLocation::PageContext;
    let ask_label = format!("Ask {chat_site_name}");

    vec![
        submenu_entry("zendia.page.ask", loc, &ask_label, Some("sparkles"), "zendia", 10),
        MenuEntry {
            command_args: vec![
                ("site_id".to_string(), "chatgpt".to_string()),
                ("prompt".to_string(), "Summarize this page".to_string()),
            ],
            ..child_entry(
                "zendia.page.ask.summarize",
                "zendia.page.ask",
                loc,
                "Summarize Page",
                None,
                "webchat.ask",
                "prompts",
                10,
            )
        },
        MenuEntry {
            command_args: vec![
                ("site_id".to_string(), "chatgpt".to_string()),
                ("prompt".to_string(), "Explain this".to_string()),
            ],
            ..child_entry(
                "zendia.page.ask.explain",
                "zendia.page.ask",
                loc,
                "Explain",
                None,
                "webchat.ask",
                "prompts",
                20,
            )
        },
        MenuEntry {
            command_args: vec![
                ("site_id".to_string(), "chatgpt".to_string()),
                ("prompt".to_string(), "Translate to English".to_string()),
            ],
            ..child_entry(
                "zendia.page.ask.translate",
                "zendia.page.ask",
                loc,
                "Translate to English",
                None,
                "webchat.ask",
                "prompts",
                30,
            )
        },
        MenuEntry {
            command_args: vec![
                ("site_id".to_string(), "chatgpt".to_string()),
                ("prompt".to_string(), "Ask about this page".to_string()),
            ],
            ..child_entry(
                "zendia.page.ask.custom",
                "zendia.page.ask",
                loc,
                "Ask…",
                None,
                "webchat.ask",
                "prompts",
                40,
            )
        },
        menu_entry(
            "zendia.page.split_view",
            loc,
            "Open in Split View",
            None,
            "page.open_in_split",
            "zendia",
            20,
        ),
        menu_entry(
            "zendia.page.glance",
            loc,
            "Preview in Glance",
            None,
            "glance.open",
            "zendia",
            30,
        ),
    ]
}

/// Built-in folder context menu contributions matching Zen's folder actions menu.
pub fn builtin_folder_context_entries() -> Vec<MenuEntry> {
    let loc = MenuLocation::FolderContext;
    vec![
        menu_entry(
            "zendia.folder.rename",
            loc,
            "Rename Folder",
            None,
            "folder.start_rename",
            "edit",
            10,
        ),
        menu_entry(
            "zendia.folder.toggle_collapsed",
            loc,
            "Collapse Folder",
            None,
            "folder.toggle_collapsed",
            "edit",
            20,
        ),
        menu_entry("zendia.folder.new_tab", loc, "New Tab", None, "tab.new", "organize", 10),
        menu_entry(
            "zendia.folder.unpack",
            loc,
            "Unpack Folder",
            None,
            "folder.unpack",
            "danger",
            10,
        ),
        menu_entry(
            "zendia.folder.delete",
            loc,
            "Delete Folder",
            None,
            "folder.delete",
            "danger",
            20,
        ),
    ]
}

/// Built-in space context menu contributions for space headers and switcher icons.
pub fn builtin_space_context_entries() -> Vec<MenuEntry> {
    let loc = MenuLocation::SpaceContext;
    vec![
        menu_entry(
            "zendia.space.switch",
            loc,
            "Switch to Space",
            None,
            "space.switch",
            "switch",
            10,
        ),
        menu_entry(
            "zendia.space.previous",
            loc,
            "Previous Space",
            None,
            "space.previous",
            "switch",
            20,
        ),
        menu_entry("zendia.space.next", loc, "Next Space", None, "space.next", "switch", 30),
        menu_entry(
            "zendia.space.new_folder",
            loc,
            "New Folder",
            None,
            "folder.create",
            "tabs",
            10,
        ),
        menu_entry(
            "zendia.space.clear_tabs",
            loc,
            "Clear Unpinned Tabs",
            None,
            "space.clear_tabs",
            "tabs",
            20,
        ),
    ]
}

/// All built-in menu contributions registered by the browser core at startup.
pub fn builtin_menu_entries(vim_mode_enabled: bool, chat_site_name: &str, essentials_count: usize) -> Vec<MenuEntry> {
    let mut entries = builtin_app_menu_entries(vim_mode_enabled);
    entries.extend(builtin_tab_context_entries(chat_site_name, essentials_count));
    entries.extend(builtin_folder_context_entries());
    entries.extend(builtin_space_context_entries());
    entries.extend(builtin_page_context_entries(chat_site_name));
    entries
}

/// Resolves the tab context menu for `tab_id` from the core's contributions, adapting dynamic labels (Pin/Unpin,
/// Add/Remove Essentials, Mute/Unmute) and populating folder/space destination submenus from `model`.
pub fn build_tab_context_menu(
    model: &BrowserModel,
    tab_id: TabId,
    key_table: Option<&KeyTable>,
) -> Vec<ResolvedMenuItem> {
    let mut items = resolve_contributed_menu(
        &model.contributions.menu_entries,
        MenuLocation::TabContext,
        key_table,
        TAB_CONTEXT_GROUPS,
    );
    let tab = model.tab(tab_id);
    let essentials_count = model.essential_tabs().len();
    let tab_space = tab.and_then(|t| t.space_id).or(model.active_space);

    for item in &mut items {
        match item.id.as_str() {
            "zendia.tab.pin" => {
                if let Some(tab) = tab {
                    match tab.kind {
                        TabKind::Pinned => {
                            item.label = "Unpin Tab".to_string();
                            item.command = "tab.unpin".to_string();
                        }
                        TabKind::Normal => {
                            item.label = "Pin Tab".to_string();
                            item.command = "tab.pin".to_string();
                        }
                        TabKind::Essential => {
                            item.label = "Pin Tab".to_string();
                            item.command = "tab.pin".to_string();
                            item.enabled = false;
                        }
                    }
                }
            }
            "zendia.tab.essentials" => {
                if let Some(tab) = tab
                    && tab.kind == TabKind::Essential
                {
                    item.label = "Remove from Essentials".to_string();
                    item.command = "tab.remove_from_essentials".to_string();
                    item.badge = Some(format!("{essentials_count} / 12"));
                } else {
                    item.label = "Add to Essentials".to_string();
                    item.command = "tab.add_to_essentials".to_string();
                    item.badge = Some(format!("{essentials_count} / 12"));
                    item.enabled = essentials_count < 12;
                }
            }
            "zendia.tab.mute" => {
                if tab.is_some_and(|t| t.is_muted) {
                    item.label = "Unmute Tab".to_string();
                } else {
                    item.label = "Mute Tab".to_string();
                }
            }
            "zendia.tab.rename" => {
                if tab.is_some_and(|t| t.kind == TabKind::Essential) {
                    item.enabled = false;
                }
            }
            "zendia.tab.move_to_folder" => {
                let mut children = vec![
                    ResolvedMenuItem::action("zendia.tab.move_to_folder.new", "New Folder…", "folder.create")
                        .with_arg("tab_id", tab_id.0.to_string()),
                ];
                if let Some(space_id) = tab_space {
                    let folders = model.folders_in_space(space_id);
                    if !folders.is_empty() {
                        children.push(ResolvedMenuItem::separator("zendia.tab.move_to_folder.sep"));
                        for folder in folders {
                            let already_in = tab.and_then(|t| t.folder_id) == Some(folder.id);
                            children.push(
                                ResolvedMenuItem::action(
                                    format!("zendia.tab.move_to_folder.{}", folder.id.0),
                                    folder.name.clone(),
                                    "tab.move_to_folder",
                                )
                                .with_arg("tab_id", tab_id.0.to_string())
                                .with_arg("folder_id", folder.id.0.to_string())
                                .with_enabled(!already_in),
                            );
                        }
                    }
                }
                if tab.is_some_and(|t| t.folder_id.is_some()) {
                    children.push(ResolvedMenuItem::separator("zendia.tab.move_to_folder.remove_sep"));
                    children.push(
                        ResolvedMenuItem::action(
                            "zendia.tab.move_to_folder.remove",
                            "Remove from Folder",
                            "tab.move_to_folder",
                        )
                        .with_arg("tab_id", tab_id.0.to_string()),
                    );
                }
                item.children = children;
            }
            "zendia.tab.move_to_space" => {
                let mut children = Vec::new();
                for space in &model.spaces {
                    let is_current = tab_space == Some(space.id) && tab.is_some_and(|t| t.kind != TabKind::Essential);
                    children.push(
                        ResolvedMenuItem::action(
                            format!("zendia.tab.move_to_space.{}", space.id.0),
                            space.name.clone(),
                            "tab.move_to_space",
                        )
                        .with_arg("tab_id", tab_id.0.to_string())
                        .with_arg("space_id", space.id.0.to_string())
                        .with_enabled(!is_current),
                    );
                }
                item.enabled = !children.is_empty();
                item.children = children;
            }
            _ => {}
        }
    }

    inject_arg_recursive(&mut items, "tab_id", &tab_id.0.to_string());
    items
}

/// Resolves the folder context menu for `folder_id` from the core's `MenuLocation::FolderContext` contributions.
pub fn build_folder_context_menu(
    model: &BrowserModel,
    folder_id: FolderId,
    key_table: Option<&KeyTable>,
) -> Vec<ResolvedMenuItem> {
    let mut items = resolve_contributed_menu(
        &model.contributions.menu_entries,
        MenuLocation::FolderContext,
        key_table,
        FOLDER_CONTEXT_GROUPS,
    );
    let collapsed = model.folder(folder_id).is_some_and(|folder| folder.collapsed);
    for item in &mut items {
        if item.id.as_str() == "zendia.folder.toggle_collapsed" {
            item.label = if collapsed {
                "Expand Folder".to_string()
            } else {
                "Collapse Folder".to_string()
            };
        }
    }
    inject_arg_recursive(&mut items, "folder_id", &folder_id.0.to_string());
    items
}

/// Resolves the space context menu for `space_id` from the core's `MenuLocation::SpaceContext` contributions.
pub fn build_space_context_menu(
    model: &BrowserModel,
    space_id: SpaceId,
    key_table: Option<&KeyTable>,
) -> Vec<ResolvedMenuItem> {
    let mut items = resolve_contributed_menu(
        &model.contributions.menu_entries,
        MenuLocation::SpaceContext,
        key_table,
        SPACE_CONTEXT_GROUPS,
    );
    let is_active = model.active_space == Some(space_id);
    let has_normal_tabs = !model.normal_tabs_in_space(space_id).is_empty();
    let multi_space = model.spaces.len() > 1;
    for item in &mut items {
        match item.id.as_str() {
            "zendia.space.switch" => {
                item.enabled = !is_active;
            }
            "zendia.space.next" | "zendia.space.previous" => {
                item.enabled = multi_space;
            }
            "zendia.space.clear_tabs" => {
                item.enabled = has_normal_tabs;
            }
            _ => {}
        }
    }
    inject_arg_recursive(&mut items, "space_id", &space_id.0.to_string());
    items
}

fn inject_arg_recursive(items: &mut [ResolvedMenuItem], arg_key: &str, arg_value: &str) {
    for item in items {
        let should_inject = match arg_key {
            "tab_id" => {
                !item.command.is_empty()
                    && !matches!(
                        item.command.as_str(),
                        "tab.new" | "space.next" | "space.previous" | "space.switch"
                    )
            }
            "folder_id" => item.command.starts_with("folder."),
            "space_id" => matches!(
                item.command.as_str(),
                "space.switch" | "space.clear_tabs" | "folder.create"
            ),
            _ => false,
        };
        if should_inject && !item.command_args.iter().any(|(name, _)| name == arg_key) {
            item.command_args.push((arg_key.to_string(), arg_value.to_string()));
        }
        inject_arg_recursive(&mut item.children, arg_key, arg_value);
    }
}

/// Kind of element that triggered a page context menu (mirrors Ladybird's `WebView::ContextMenuKind`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageContextMenuKind {
    #[default]
    Page,
    Link,
    Image,
    Media,
}

/// Media state for a media context menu (mirrors Ladybird's `Web::MediaContextMenu`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaMenuContext {
    pub media_url: Option<String>,
    pub is_video: bool,
    pub is_playing: bool,
    pub is_muted: bool,
    pub has_user_agent_controls: bool,
    pub is_looping: bool,
    pub is_fullscreen: bool,
    pub is_picture_in_picture: bool,
    pub can_enter_picture_in_picture: bool,
}

/// Core model representation of a page context menu request (mirrors `WebView::ContextMenuRequest` without Hub types).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageContextMenuRequest {
    pub tab_id: Option<TabId>,
    pub kind: PageContextMenuKind,
    pub position: (i32, i32),
    pub is_editable: bool,
    pub selected_text: Option<String>,
    pub has_dictionary_lookup: bool,
    pub dictionary_lookup_text: Option<String>,
    pub misspelled_word: Option<String>,
    pub spelling_guesses: Vec<String>,
    pub makes_link_to_highlight: bool,
    pub can_inspect_element: bool,
    pub link_url: Option<String>,
    pub has_link_navigation: bool,
    pub navigation_url: Option<String>,
    pub has_navigation: bool,
    pub has_image_bitmap: bool,
    pub image_mime_type: Option<String>,
    pub media: Option<MediaMenuContext>,
    pub can_navigate_back: bool,
    pub can_navigate_forward: bool,
    pub can_view_source: bool,
    pub can_report_website_issue: bool,
    pub is_private: bool,
    pub supports_private_browsing_windows: bool,
    pub search_engine_name: Option<String>,
}

fn with_optional_tab(mut item: ResolvedMenuItem, tab_id: Option<TabId>) -> ResolvedMenuItem {
    if let Some(tab_id) = tab_id {
        item.command_args.push(("tab_id".to_string(), tab_id.0.to_string()));
    }
    item
}

/// Checks if selected text looks like a direct HTTP(S) URL, mirroring Ladybird's `WebView::url_from_text`.
pub fn url_from_selected_text(selected_text: &str) -> Option<String> {
    let trimmed = selected_text.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let rest = trimmed.split_once("://")?.1;
        if !rest.is_empty() && !rest.starts_with('/') {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn copy_link_label_for_url(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("mailto:") {
        "Copy Email Address"
    } else if lower.starts_with("tel:") {
        "Copy Phone Number"
    } else {
        "Copy Link"
    }
}

fn url_text_to_copy(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("mailto:").or_else(|| url.strip_prefix("MAILTO:")) {
        rest.to_string()
    } else if let Some(rest) = url.strip_prefix("tel:").or_else(|| url.strip_prefix("TEL:")) {
        rest.to_string()
    } else {
        url.to_string()
    }
}

fn format_search_label(engine_name: &str, selected_text: &str) -> String {
    const MAX_LEN: usize = 32;
    let collapsed: String = selected_text.split_whitespace().collect::<Vec<_>>().join(" ");
    let truncated = if collapsed.chars().count() > MAX_LEN {
        let prefix: String = collapsed.chars().take(MAX_LEN).collect();
        format!("{prefix}…")
    } else {
        collapsed
    };
    format!("Search {engine_name} for \"{truncated}\"")
}

fn add_open_url_actions(items: &mut Vec<ResolvedMenuItem>, request: &PageContextMenuRequest, url: &str, prefix: &str) {
    items.push(with_optional_tab(
        ResolvedMenuItem::action(
            format!("{prefix}.open_new_tab"),
            "Open Link in New Tab",
            "page.open_link_in_new_tab",
        )
        .with_arg("url", url)
        .with_arg("activate", "false"),
        request.tab_id,
    ));
    if !request.is_private {
        items.push(with_optional_tab(
            ResolvedMenuItem::action(
                format!("{prefix}.open_new_window"),
                "Open Link in New Window",
                "page.open_link_in_new_window",
            )
            .with_arg("url", url),
            request.tab_id,
        ));
    }
    if request.supports_private_browsing_windows {
        items.push(with_optional_tab(
            ResolvedMenuItem::action(
                format!("{prefix}.open_private_window"),
                "Open Link in Private Window",
                "page.open_link_in_private_window",
            )
            .with_arg("url", url),
            request.tab_id,
        ));
    }
}

fn add_link_actions(items: &mut Vec<ResolvedMenuItem>, request: &PageContextMenuRequest, url: &str) {
    add_open_url_actions(items, request, url, "ctx.link");
    items.push(ResolvedMenuItem::separator("ctx.link.sep"));
    items.push(
        ResolvedMenuItem::action("ctx.link.copy", copy_link_label_for_url(url), "page.copy_link")
            .with_arg("url", url_text_to_copy(url)),
    );
    items.push(with_optional_tab(
        ResolvedMenuItem::action("ctx.link.download", "Download Linked File", "page.download_link")
            .with_arg("url", url),
        request.tab_id,
    ));
    items.push(with_optional_tab(
        ResolvedMenuItem::action("ctx.link.save_as", "Save Link As…", "page.save_link_as").with_arg("url", url),
        request.tab_id,
    ));
}

fn add_text_edit_actions(items: &mut Vec<ResolvedMenuItem>, request: &PageContextMenuRequest, has_selected_text: bool) {
    if request.is_editable {
        items.push(
            with_optional_tab(
                ResolvedMenuItem::action("ctx.edit.cut", "Cut", "page.cut"),
                request.tab_id,
            )
            .with_shortcut(Some("Ctrl+X".to_string())),
        );
    }
    if request.is_editable || has_selected_text {
        items.push(
            with_optional_tab(
                ResolvedMenuItem::action("ctx.edit.copy", "Copy", "page.copy"),
                request.tab_id,
            )
            .with_shortcut(Some("Ctrl+C".to_string())),
        );
    }
    if request.is_editable {
        items.push(
            with_optional_tab(
                ResolvedMenuItem::action("ctx.edit.paste", "Paste", "page.paste").with_arg("match_style", "false"),
                request.tab_id,
            )
            .with_shortcut(Some("Ctrl+V".to_string())),
        );
        items.push(
            with_optional_tab(
                ResolvedMenuItem::action(
                    "ctx.edit.paste_match",
                    "Paste and Match Style",
                    "page.paste_and_match_style",
                )
                .with_arg("match_style", "true"),
                request.tab_id,
            )
            .with_shortcut(Some("Ctrl+Shift+V".to_string())),
        );
        items.push(
            with_optional_tab(
                ResolvedMenuItem::action("ctx.edit.select_all", "Select All", "page.select_all"),
                request.tab_id,
            )
            .with_shortcut(Some("Ctrl+A".to_string())),
        );
    }
}

fn add_look_up_action(items: &mut Vec<ResolvedMenuItem>, request: &PageContextMenuRequest) {
    if !request.has_dictionary_lookup {
        return;
    }
    let mut item = with_optional_tab(
        ResolvedMenuItem::action("ctx.lookup", "Look Up", "page.look_up"),
        request.tab_id,
    );
    if let Some(text) = &request.dictionary_lookup_text {
        item = item.with_arg("text", text);
    }
    items.push(item);
}

fn add_search_action(items: &mut Vec<ResolvedMenuItem>, request: &PageContextMenuRequest) {
    let (Some(engine), Some(selected)) = (&request.search_engine_name, &request.selected_text) else {
        return;
    };
    if selected.trim().is_empty() {
        return;
    }
    items.push(with_optional_tab(
        ResolvedMenuItem::action(
            "ctx.search_selected",
            format_search_label(engine, selected),
            "page.search_selected_text",
        )
        .with_arg("query", selected),
        request.tab_id,
    ));
}

fn add_spelling_actions(items: &mut Vec<ResolvedMenuItem>, request: &PageContextMenuRequest) {
    let Some(word) = &request.misspelled_word else {
        return;
    };
    if request.spelling_guesses.is_empty() {
        items.push(
            ResolvedMenuItem::action("ctx.spelling.none", "No Guesses Found", "page.no_spelling_guesses")
                .with_enabled(false),
        );
    } else {
        for (idx, guess) in request.spelling_guesses.iter().enumerate() {
            items.push(with_optional_tab(
                ResolvedMenuItem::action(
                    format!("ctx.spelling.guess.{idx}"),
                    guess.clone(),
                    "page.replace_misspelling",
                )
                .with_arg("replacement", guess),
                request.tab_id,
            ));
        }
    }
    items.push(ResolvedMenuItem::separator("ctx.spelling.sep"));
    items.push(with_optional_tab(
        ResolvedMenuItem::action("ctx.spelling.ignore", "Ignore Spelling", "page.ignore_misspelling")
            .with_arg("word", word),
        request.tab_id,
    ));
    items.push(with_optional_tab(
        ResolvedMenuItem::action("ctx.spelling.learn", "Learn Spelling", "page.learn_misspelling")
            .with_arg("word", word),
        request.tab_id,
    ));
}

fn add_copy_link_to_highlight_action(items: &mut Vec<ResolvedMenuItem>, request: &PageContextMenuRequest) {
    if request.makes_link_to_highlight {
        items.push(with_optional_tab(
            ResolvedMenuItem::action(
                "ctx.copy_link_to_highlight",
                "Copy Link to Highlight",
                "page.copy_link_to_highlight",
            ),
            request.tab_id,
        ));
    }
}

fn add_contributed_page_entries(
    items: &mut Vec<ResolvedMenuItem>,
    request: &PageContextMenuRequest,
    contributions: &[MenuEntry],
    key_table: Option<&KeyTable>,
) {
    let mut contributed =
        resolve_contributed_menu(contributions, MenuLocation::PageContext, key_table, PAGE_CONTEXT_GROUPS);
    if contributed.is_empty() {
        return;
    }
    // Contextualize Zendia's split/Glance contributions when right-clicking a link.
    let link_url = request.link_url.as_deref();
    for item in &mut contributed {
        if (item.command == "page.open_in_split" || item.command == "glance.open")
            && let Some(url) = link_url
        {
            item.command_args.push(("url".to_string(), url.to_string()));
            if item.command == "page.open_in_split" {
                item.label = "Open Link in Split View".to_string();
            } else {
                item.label = "Preview Link in Glance".to_string();
            }
        }
        if let Some(tab_id) = request.tab_id
            && !item.command.is_empty()
            && !item.command_args.iter().any(|(k, _)| k == "tab_id")
        {
            item.command_args.push(("tab_id".to_string(), tab_id.0.to_string()));
        }
    }
    items.push(ResolvedMenuItem::separator("ctx.contributions.sep"));
    items.extend(contributed);
}

fn add_inspect_element_action(items: &mut Vec<ResolvedMenuItem>, request: &PageContextMenuRequest) {
    if !request.can_inspect_element {
        return;
    }
    items.push(ResolvedMenuItem::separator("ctx.inspect.sep"));
    items.push(with_optional_tab(
        ResolvedMenuItem::action("ctx.inspect", "Inspect Element", "page.inspect_element"),
        request.tab_id,
    ));
}

/// Builds a page, link, image, media, or selected-text context menu following Ladybird's `UI/Qt/ContextMenus.cpp`
/// strictly in entries, order, separators, conditions, and enabled states, then injects contributed
/// [`MenuLocation::PageContext`] entries (Zendia's Ask, Split View, Glance, and any plugin/extension items).
pub fn build_page_context_menu(
    request: &PageContextMenuRequest,
    contributions: &[MenuEntry],
    key_table: Option<&KeyTable>,
) -> Vec<ResolvedMenuItem> {
    let mut items = Vec::new();

    match request.kind {
        PageContextMenuKind::Page => {
            if let Some(selected) = &request.selected_text
                && let Some(url) = url_from_selected_text(selected)
            {
                add_look_up_action(&mut items, request);
                add_open_url_actions(&mut items, request, &url, "ctx.selected_link");
                items.push(ResolvedMenuItem::separator("ctx.selected_link.sep1"));
                add_text_edit_actions(&mut items, request, true);
                items.push(ResolvedMenuItem::separator("ctx.selected_link.sep2"));
                add_search_action(&mut items, request);
                add_copy_link_to_highlight_action(&mut items, request);
                add_contributed_page_entries(&mut items, request, contributions, key_table);
                add_inspect_element_action(&mut items, request);
                return collapse_separators(items);
            }
            build_plain_page_items(&mut items, request, contributions, key_table);
        }
        PageContextMenuKind::Link => {
            if let Some(link_url) = &request.link_url {
                add_link_actions(&mut items, request, link_url);
                add_copy_link_to_highlight_action(&mut items, request);
                add_contributed_page_entries(&mut items, request, contributions, key_table);
                add_inspect_element_action(&mut items, request);
            } else {
                build_plain_page_items(&mut items, request, contributions, key_table);
            }
        }
        PageContextMenuKind::Image => {
            if let Some(image_url) = &request.navigation_url {
                if let Some(link_url) = &request.link_url {
                    add_link_actions(&mut items, request, link_url);
                    items.push(ResolvedMenuItem::separator("ctx.image.link_sep"));
                }
                items.push(with_optional_tab(
                    ResolvedMenuItem::action(
                        "ctx.image.open_new_tab",
                        "Open Image in New Tab",
                        "page.open_image_in_new_tab",
                    )
                    .with_arg("url", image_url)
                    .with_arg("activate", "false"),
                    request.tab_id,
                ));
                items.push(with_optional_tab(
                    ResolvedMenuItem::action("ctx.image.open", "Open Image", "navigation.load_url")
                        .with_arg("input", image_url),
                    request.tab_id,
                ));
                items.push(ResolvedMenuItem::separator("ctx.image.copy_sep"));
                items.push(with_optional_tab(
                    ResolvedMenuItem::action("ctx.image.copy", "Copy Image", "page.copy_image")
                        .with_enabled(request.has_image_bitmap),
                    request.tab_id,
                ));
                items.push(
                    ResolvedMenuItem::action(
                        "ctx.image.copy_address",
                        "Copy Image Address",
                        "page.copy_image_address",
                    )
                    .with_arg("url", url_text_to_copy(image_url)),
                );
                let mut save = with_optional_tab(
                    ResolvedMenuItem::action("ctx.image.save", "Save Image As…", "page.save_image")
                        .with_arg("url", image_url),
                    request.tab_id,
                );
                if let Some(mime) = &request.image_mime_type {
                    save = save.with_arg("mime_type", mime);
                }
                items.push(save);
                add_contributed_page_entries(&mut items, request, contributions, key_table);
                add_inspect_element_action(&mut items, request);
            } else {
                build_plain_page_items(&mut items, request, contributions, key_table);
            }
        }
        PageContextMenuKind::Media => {
            if let Some(media) = &request.media {
                items.push(with_optional_tab(
                    ResolvedMenuItem::action(
                        "ctx.media.play",
                        if media.is_playing { "Pause" } else { "Play" },
                        "media.toggle_play",
                    ),
                    request.tab_id,
                ));
                items.push(with_optional_tab(
                    ResolvedMenuItem::action(
                        "ctx.media.mute",
                        if media.is_muted { "Unmute" } else { "Mute" },
                        "media.toggle_mute",
                    ),
                    request.tab_id,
                ));
                items.push(with_optional_tab(
                    ResolvedMenuItem::action(
                        "ctx.media.controls",
                        if media.has_user_agent_controls {
                            "Hide Controls"
                        } else {
                            "Show Controls"
                        },
                        "media.toggle_controls",
                    ),
                    request.tab_id,
                ));
                items.push(with_optional_tab(
                    ResolvedMenuItem::action("ctx.media.loop", "Loop", "media.toggle_loop").with_kind(
                        MenuEntryKind::Checkable {
                            checked: media.is_looping,
                        },
                    ),
                    request.tab_id,
                ));
                items.push(ResolvedMenuItem::separator("ctx.media.sep1"));
                if media.is_picture_in_picture {
                    items.push(with_optional_tab(
                        ResolvedMenuItem::action("ctx.media.pip", "Exit Picture in Picture", "media.toggle_pip"),
                        request.tab_id,
                    ));
                } else if media.can_enter_picture_in_picture {
                    items.push(with_optional_tab(
                        ResolvedMenuItem::action("ctx.media.pip", "Picture in Picture", "media.toggle_pip"),
                        request.tab_id,
                    ));
                }
                if media.is_video && media.is_fullscreen {
                    items.push(with_optional_tab(
                        ResolvedMenuItem::action("ctx.media.fullscreen", "Exit Full Screen", "media.toggle_fullscreen"),
                        request.tab_id,
                    ));
                } else if media.is_video {
                    items.push(with_optional_tab(
                        ResolvedMenuItem::action("ctx.media.fullscreen", "Full Screen", "media.toggle_fullscreen"),
                        request.tab_id,
                    ));
                }
                items.push(ResolvedMenuItem::separator("ctx.media.sep2"));
                if let Some(url) = &media.media_url {
                    if request.has_navigation {
                        items.push(with_optional_tab(
                            ResolvedMenuItem::action(
                                "ctx.media.open_new_tab",
                                if media.is_video {
                                    "Open Video in New Tab"
                                } else {
                                    "Open Audio in New Tab"
                                },
                                "page.open_media_in_new_tab",
                            )
                            .with_arg("url", url)
                            .with_arg("activate", "false"),
                            request.tab_id,
                        ));
                        items.push(with_optional_tab(
                            ResolvedMenuItem::action(
                                "ctx.media.open",
                                if media.is_video { "Open Video" } else { "Open Audio" },
                                "navigation.load_url",
                            )
                            .with_arg("input", url),
                            request.tab_id,
                        ));
                    }
                    items.push(
                        ResolvedMenuItem::action(
                            "ctx.media.copy_address",
                            if media.is_video {
                                "Copy Video Address"
                            } else {
                                "Copy Audio Address"
                            },
                            "page.copy_media_address",
                        )
                        .with_arg("url", url_text_to_copy(url)),
                    );
                }
                add_contributed_page_entries(&mut items, request, contributions, key_table);
                add_inspect_element_action(&mut items, request);
            } else {
                build_plain_page_items(&mut items, request, contributions, key_table);
            }
        }
    }

    collapse_separators(items)
}

fn build_plain_page_items(
    items: &mut Vec<ResolvedMenuItem>,
    request: &PageContextMenuRequest,
    contributions: &[MenuEntry],
    key_table: Option<&KeyTable>,
) {
    let is_about_page = !request.is_editable && request.selected_text.is_none();

    add_spelling_actions(items, request);
    items.push(ResolvedMenuItem::separator("ctx.page.sep1"));
    add_look_up_action(items, request);
    if is_about_page {
        let back_shortcut = key_table.and_then(|t| shortcut_hint_from_table(t, "navigation.back", &[]));
        let fwd_shortcut = key_table.and_then(|t| shortcut_hint_from_table(t, "navigation.forward", &[]));
        let reload_shortcut = key_table
            .and_then(|t| shortcut_hint_from_table(t, "navigation.reload", &[("bypass_cache".into(), "false".into())]))
            .or_else(|| key_table.and_then(|t| shortcut_hint_from_table(t, "navigation.reload", &[])));

        items.push(with_optional_tab(
            ResolvedMenuItem::action("ctx.page.back", "Back", "navigation.back")
                .with_enabled(request.can_navigate_back)
                .with_shortcut(back_shortcut),
            request.tab_id,
        ));
        items.push(with_optional_tab(
            ResolvedMenuItem::action("ctx.page.forward", "Forward", "navigation.forward")
                .with_enabled(request.can_navigate_forward)
                .with_shortcut(fwd_shortcut),
            request.tab_id,
        ));
        items.push(with_optional_tab(
            ResolvedMenuItem::action("ctx.page.reload", "Reload", "navigation.reload")
                .with_arg("bypass_cache", "false")
                .with_shortcut(reload_shortcut),
            request.tab_id,
        ));
    }
    items.push(ResolvedMenuItem::separator("ctx.page.sep2"));
    add_text_edit_actions(items, request, request.selected_text.is_some());
    items.push(ResolvedMenuItem::separator("ctx.page.sep3"));
    add_search_action(items, request);
    add_copy_link_to_highlight_action(items, request);
    items.push(ResolvedMenuItem::separator("ctx.page.sep4"));
    if is_about_page {
        items.push(with_optional_tab(
            ResolvedMenuItem::action("ctx.page.view_source", "View Page Source", "page.view_source")
                .with_enabled(request.can_view_source),
            request.tab_id,
        ));
    }
    items.push(with_optional_tab(
        ResolvedMenuItem::action("ctx.page.report_issue", "Report Website Issue…", "page.report_issue")
            .with_enabled(request.can_report_website_issue),
        request.tab_id,
    ));
    items.push(ResolvedMenuItem::separator("ctx.page.sep5"));
    if is_about_page {
        items.push(with_optional_tab(
            ResolvedMenuItem::action("ctx.page.screenshot", "Take Screenshot...", "page.take_screenshot"),
            request.tab_id,
        ));
    }
    add_contributed_page_entries(items, request, contributions, key_table);
    add_inspect_element_action(items, request);
}

/// One selectable `<option>` entry inside a `<select>` dropdown popup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectOption {
    pub id: u32,
    pub label: String,
    pub value: String,
    pub selected: bool,
    pub disabled: bool,
}

/// One item inside a page `<select>` dropdown popup (mirrors `Web::HTML::SelectItem`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectDropdownItem {
    Option(SelectOption),
    Group { label: String, items: Vec<SelectOption> },
    Separator,
}

/// Core model representation of a `<select>` dropdown request (no Hub types).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectDropdownRequest {
    pub tab_id: Option<TabId>,
    pub position: (i32, i32),
    pub minimum_width: i32,
    pub items: Vec<SelectDropdownItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(items: &[ResolvedMenuItem]) -> Vec<&str> {
        items
            .iter()
            .map(|item| {
                if item.is_separator() {
                    "---"
                } else {
                    item.label.as_str()
                }
            })
            .collect()
    }

    #[test]
    fn app_menu_order_groups_shortcuts_and_submenus() {
        let table = KeyTable::standard();
        let entries = builtin_app_menu_entries(false);
        let resolved = resolve_contributed_menu(&entries, MenuLocation::AppMenu, Some(&table), APP_MENU_GROUPS);

        assert_eq!(
            labels(&resolved),
            vec![
                "New Tab",
                "New Window",
                "New Blank Window",
                "New Private Window",
                "---",
                "History",
                "Bookmarks",
                "Downloads",
                "Passwords",
                "Extensions and Themes",
                "---",
                "Print...",
                "Save Page As...",
                "Find in Page...",
                "---",
                "Zoom",
                "---",
                "Vim mode",
                "Settings",
                "More Tools",
                "Help and Report",
                "---",
                "Quit",
            ]
        );

        let new_tab = resolved.iter().find(|i| i.label == "New Tab").unwrap();
        assert_eq!(new_tab.command, "tab.new");
        assert!(new_tab.shortcut_hint.as_deref().is_some_and(|s| s.ends_with("+T")));

        let downloads = resolved.iter().find(|i| i.label == "Downloads").unwrap();
        assert!(
            downloads
                .shortcut_hint
                .as_deref()
                .is_some_and(|s| s.contains("Shift+Y"))
        );

        let zoom = resolved.iter().find(|i| i.label == "Zoom").unwrap();
        assert_eq!(
            zoom.kind,
            MenuEntryKind::Widget {
                widget_id: ZOOM_CONTROLS_WIDGET_ID.to_string()
            }
        );

        let vim = resolved.iter().find(|i| i.label == "Vim mode").unwrap();
        assert_eq!(vim.kind, MenuEntryKind::Toggle { checked: false });
        assert!(vim.keeps_menu_open);

        let help = resolved.iter().find(|i| i.label == "Help and Report").unwrap();
        assert!(help.is_submenu());
        assert!(
            help.children
                .iter()
                .any(|c| c.label == "About Zendia" && c.command == "app.about")
        );
    }

    #[test]
    fn merges_plugin_and_extension_contributions_with_deterministic_ordering() {
        let mut entries = builtin_app_menu_entries(true);
        // Plugin injects a new group and an extra item inside an existing submenu.
        entries.push(MenuEntry {
            id: ContributionId::new("plugin.demo.item"),
            location: MenuLocation::AppMenu,
            label: "Plugin Action".to_string(),
            icon: Some("sparkles".to_string()),
            command: "plugin.demo.run".to_string(),
            command_args: Vec::new(),
            shortcut_hint: None,
            group: "zz_plugin".to_string(),
            order: 5,
            enabled: true,
            source: ContributionSource::Plugin {
                plugin_id: "demo".to_string(),
            },
            parent: None,
            kind: MenuEntryKind::Action,
            badge: None,
            keeps_menu_open: false,
        });
        entries.push(MenuEntry {
            id: ContributionId::new("ext.tools.inspect_json"),
            location: MenuLocation::AppMenu,
            label: "JSON Inspector".to_string(),
            icon: None,
            command: "ext.json.open".to_string(),
            command_args: Vec::new(),
            shortcut_hint: None,
            group: "dev".to_string(),
            order: 15,
            enabled: true,
            source: ContributionSource::Extension {
                extension_id: "json-tools".to_string(),
            },
            parent: Some(ContributionId::new("zendia.app.more_tools")),
            kind: MenuEntryKind::Action,
            badge: None,
            keeps_menu_open: false,
        });

        let resolved = resolve_contributed_menu(&entries, MenuLocation::AppMenu, None, APP_MENU_GROUPS);
        assert_eq!(resolved.last().unwrap().label, "Plugin Action");
        assert!(resolved[resolved.len() - 2].is_separator());

        let more_tools = resolved.iter().find(|i| i.label == "More Tools").unwrap();
        let child_labels = labels(&more_tools.children);
        assert_eq!(child_labels[0], "Developer Tools");
        assert_eq!(child_labels[1], "JSON Inspector");
        assert_eq!(child_labels[2], "View Page Source");

        // Ensure no duplicate consecutive separators exist anywhere.
        for pair in resolved.windows(2) {
            assert!(!(pair[0].is_separator() && pair[1].is_separator()));
        }
    }

    #[test]
    fn builds_plain_page_context_menu_matching_qt_order() {
        let req = PageContextMenuRequest {
            tab_id: Some(TabId(4)),
            kind: PageContextMenuKind::Page,
            can_navigate_back: true,
            can_navigate_forward: false,
            can_view_source: true,
            can_report_website_issue: true,
            can_inspect_element: true,
            ..PageContextMenuRequest::default()
        };
        let contribs = builtin_page_context_entries("ChatGPT");
        let items = build_page_context_menu(&req, &contribs, Some(&KeyTable::standard()));

        assert_eq!(
            labels(&items),
            vec![
                "Back",
                "Forward",
                "Reload",
                "---",
                "View Page Source",
                "Report Website Issue…",
                "---",
                "Take Screenshot...",
                "---",
                "Ask ChatGPT",
                "Open in Split View",
                "Preview in Glance",
                "---",
                "Inspect Element",
            ]
        );
        assert!(items[0].enabled);
        assert!(!items[1].enabled);
        assert_eq!(items.last().unwrap().command, "page.inspect_element");
    }

    #[test]
    fn builds_editable_menu_with_spelling_guesses_and_no_page_nav() {
        let req = PageContextMenuRequest {
            tab_id: Some(TabId(2)),
            kind: PageContextMenuKind::Page,
            is_editable: true,
            misspelled_word: Some("browsr".to_string()),
            spelling_guesses: vec!["browser".to_string(), "browsed".to_string()],
            can_report_website_issue: false,
            can_inspect_element: true,
            ..PageContextMenuRequest::default()
        };
        let items = build_page_context_menu(&req, &[], None);
        assert_eq!(
            labels(&items),
            vec![
                "browser",
                "browsed",
                "---",
                "Ignore Spelling",
                "Learn Spelling",
                "---",
                "Cut",
                "Copy",
                "Paste",
                "Paste and Match Style",
                "Select All",
                "---",
                "Report Website Issue…",
                "---",
                "Inspect Element",
            ]
        );
        assert!(!items.iter().any(|i| i.label == "Back" || i.label == "Reload"));
        assert!(
            !items
                .iter()
                .find(|i| i.label == "Report Website Issue…")
                .unwrap()
                .enabled
        );
    }

    #[test]
    fn builds_editable_menu_with_disabled_no_guesses_placeholder() {
        let req = PageContextMenuRequest {
            kind: PageContextMenuKind::Page,
            is_editable: true,
            misspelled_word: Some("zzzzzz".to_string()),
            spelling_guesses: Vec::new(),
            ..PageContextMenuRequest::default()
        };
        let items = build_page_context_menu(&req, &[], None);
        assert_eq!(items[0].label, "No Guesses Found");
        assert!(!items[0].enabled);
    }

    #[test]
    fn builds_selected_text_and_selected_url_menus() {
        let text_req = PageContextMenuRequest {
            kind: PageContextMenuKind::Page,
            selected_text: Some("Ladybird engine".to_string()),
            has_dictionary_lookup: true,
            dictionary_lookup_text: Some("Ladybird".to_string()),
            search_engine_name: Some("Google".to_string()),
            makes_link_to_highlight: true,
            can_report_website_issue: true,
            can_inspect_element: true,
            ..PageContextMenuRequest::default()
        };
        let text_items = build_page_context_menu(&text_req, &[], None);
        assert_eq!(
            labels(&text_items),
            vec![
                "Look Up",
                "---",
                "Copy",
                "---",
                "Search Google for \"Ladybird engine\"",
                "Copy Link to Highlight",
                "---",
                "Report Website Issue…",
                "---",
                "Inspect Element",
            ]
        );

        let url_req = PageContextMenuRequest {
            kind: PageContextMenuKind::Page,
            selected_text: Some("https://ladybird.org".to_string()),
            supports_private_browsing_windows: true,
            search_engine_name: Some("Google".to_string()),
            can_inspect_element: true,
            ..PageContextMenuRequest::default()
        };
        let url_items = build_page_context_menu(&url_req, &[], None);
        assert_eq!(
            labels(&url_items),
            vec![
                "Open Link in New Tab",
                "Open Link in New Window",
                "Open Link in Private Window",
                "---",
                "Copy",
                "---",
                "Search Google for \"https://ladybird.org\"",
                "---",
                "Inspect Element",
            ]
        );
    }

    #[test]
    fn builds_link_image_and_media_context_menus() {
        let mail_req = PageContextMenuRequest {
            kind: PageContextMenuKind::Link,
            link_url: Some("mailto:hello@zendia.dev".to_string()),
            has_link_navigation: true,
            is_private: true,
            supports_private_browsing_windows: true,
            can_inspect_element: true,
            ..PageContextMenuRequest::default()
        };
        let mail_items = build_page_context_menu(&mail_req, &[], None);
        assert_eq!(
            labels(&mail_items),
            vec![
                "Open Link in New Tab",
                "Open Link in Private Window",
                "---",
                "Copy Email Address",
                "Download Linked File",
                "Save Link As…",
                "---",
                "Inspect Element",
            ]
        );
        let copy_email = mail_items.iter().find(|i| i.label == "Copy Email Address").unwrap();
        assert_eq!(
            copy_email.command_args,
            vec![("url".to_string(), "hello@zendia.dev".to_string())]
        );

        let img_req = PageContextMenuRequest {
            kind: PageContextMenuKind::Image,
            link_url: Some("https://example.org".to_string()),
            navigation_url: Some("https://example.org/hero.png".to_string()),
            has_navigation: true,
            has_image_bitmap: false,
            can_inspect_element: true,
            ..PageContextMenuRequest::default()
        };
        let img_items = build_page_context_menu(&img_req, &[], None);
        assert!(!img_items.iter().find(|i| i.label == "Copy Image").unwrap().enabled);
        assert!(img_items.iter().any(|i| i.label == "Open Image in New Tab"));
        assert!(img_items.iter().any(|i| i.label == "Copy Image Address"));

        let media_req = PageContextMenuRequest {
            kind: PageContextMenuKind::Media,
            has_navigation: true,
            media: Some(MediaMenuContext {
                media_url: Some("https://example.org/demo.mp4".to_string()),
                is_video: true,
                is_playing: true,
                is_muted: false,
                has_user_agent_controls: true,
                is_looping: true,
                is_fullscreen: false,
                is_picture_in_picture: false,
                can_enter_picture_in_picture: true,
            }),
            can_inspect_element: true,
            ..PageContextMenuRequest::default()
        };
        let media_items = build_page_context_menu(&media_req, &[], None);
        assert_eq!(
            labels(&media_items),
            vec![
                "Pause",
                "Mute",
                "Hide Controls",
                "Loop",
                "---",
                "Picture in Picture",
                "Full Screen",
                "---",
                "Open Video in New Tab",
                "Open Video",
                "Copy Video Address",
                "---",
                "Inspect Element",
            ]
        );
        let loop_item = media_items.iter().find(|i| i.label == "Loop").unwrap();
        assert_eq!(loop_item.kind, MenuEntryKind::Checkable { checked: true });
    }

    #[test]
    fn about_info_uses_cargo_version_and_ladybird_attribution() {
        let info = about_info();
        assert_eq!(info.app_name, "Zendia");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.engine_line, "Based on Ladybird");
        assert!(!info.primary_links.is_empty());
        assert!(!info.legal_links.is_empty());
    }

    #[test]
    fn tab_folder_and_space_context_menus_reflect_model_state() {
        use crate::BrowserCore;
        use crate::stub::StubBackend;

        let core = StubBackend::new();
        let model = core.model();
        let table = KeyTable::standard();

        // Normal tab in Space 1 shows Pin Tab, Add to Essentials, Rename Tab, Move to Folder, Move to Space.
        let normal_tab = model.normal_tabs_in_space(SpaceId(1))[0].id;
        let tab_items = build_tab_context_menu(&model, normal_tab, Some(&table));
        assert!(tab_items.iter().any(|i| i.label == "Pin Tab" && i.command == "tab.pin"));
        assert!(
            tab_items
                .iter()
                .any(|i| i.label == "Add to Essentials" && i.command == "tab.add_to_essentials")
        );
        assert!(
            tab_items
                .iter()
                .any(|i| i.label == "Rename Tab" && i.command == "tab.start_rename")
        );
        let move_folder = tab_items
            .iter()
            .find(|i| i.id.as_str() == "zendia.tab.move_to_folder")
            .unwrap();
        assert!(move_folder.children.iter().any(|c| c.label == "UI Spec"));
        assert!(move_folder.children.iter().any(|c| c.label == "Engine"));
        let move_space = tab_items
            .iter()
            .find(|i| i.id.as_str() == "zendia.tab.move_to_space")
            .unwrap();
        assert!(move_space.children.iter().any(|c| c.label == "Work" && c.enabled));

        // Pinned tab flips to Unpin Tab.
        let pinned_tab = model.pinned_tabs_in_space(SpaceId(1))[0].id;
        let pinned_items = build_tab_context_menu(&model, pinned_tab, Some(&table));
        assert!(
            pinned_items
                .iter()
                .any(|i| i.label == "Unpin Tab" && i.command == "tab.unpin")
        );

        // Essential tab flips to Remove from Essentials.
        let ess_tab = model.essential_tabs()[0].id;
        let ess_items = build_tab_context_menu(&model, ess_tab, Some(&table));
        assert!(
            ess_items
                .iter()
                .any(|i| i.label == "Remove from Essentials" && i.command == "tab.remove_from_essentials")
        );

        // Folder menu toggles Expand / Collapse label based on collapsed state.
        let open_folder = build_folder_context_menu(&model, FolderId(1), Some(&table));
        assert!(open_folder.iter().any(|i| i.label == "Collapse Folder"));
        let collapsed_folder = build_folder_context_menu(&model, FolderId(2), Some(&table));
        assert!(collapsed_folder.iter().any(|i| i.label == "Expand Folder"));

        // Space menu disables Switch to Space for the active space and enables it for inactive spaces.
        let active_space_menu = build_space_context_menu(&model, SpaceId(1), Some(&table));
        assert!(
            !active_space_menu
                .iter()
                .find(|i| i.id.as_str() == "zendia.space.switch")
                .unwrap()
                .enabled
        );
        let inactive_space_menu = build_space_context_menu(&model, SpaceId(2), Some(&table));
        assert!(
            inactive_space_menu
                .iter()
                .find(|i| i.id.as_str() == "zendia.space.switch")
                .unwrap()
                .enabled
        );
    }
}
