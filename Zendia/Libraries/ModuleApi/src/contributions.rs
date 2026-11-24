/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Shared UI contribution shapes used by built-in browser features, plugins, and extensions.

/// Stable identifier for a contributed toolbar item, menu entry, or palette entry.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContributionId(pub String);

impl ContributionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable identifier for a side panel hosted by the window's dock frame.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SidePanelId(pub String);

impl SidePanelId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Where a UI contribution originates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContributionSource {
    BuiltIn,
    Plugin { plugin_id: String },
    Extension { extension_id: String },
}

/// Placement slot for a toolbar item across toolbar layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolbarPlacement {
    NavigationStart,
    NavigationEnd,
    SidebarTop,
    SidebarBottom,
    AddressBarEnd,
}

/// Visual tone for a badge shown on a toolbar item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeTone {
    Neutral,
    Accent,
    Warning,
    Error,
}

/// Small counter or status badge attached to a toolbar item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolbarBadge {
    pub text: String,
    pub tone: BadgeTone,
}

/// A button or widget placed on the browser's toolbar model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolbarItem {
    pub id: ContributionId,
    pub title: String,
    pub tooltip: Option<String>,
    pub icon: String,
    pub command: String,
    pub command_args: Vec<(String, String)>,
    pub placement: ToolbarPlacement,
    pub order: i32,
    pub badge: Option<ToolbarBadge>,
    pub visible: bool,
    pub enabled: bool,
    pub source: ContributionSource,
}

/// Which menu a contributed menu entry appears in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MenuLocation {
    AppMenu,
    TabContext,
    PageContext,
    FolderContext,
    SidebarContext,
}

/// A contributed entry inside an application or context menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuEntry {
    pub id: ContributionId,
    pub location: MenuLocation,
    pub label: String,
    pub icon: Option<String>,
    pub command: String,
    pub command_args: Vec<(String, String)>,
    pub shortcut_hint: Option<String>,
    pub group: String,
    pub order: i32,
    pub enabled: bool,
    pub source: ContributionSource,
}

/// An actionable entry listed in the command palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteEntry {
    pub id: ContributionId,
    pub title: String,
    pub detail: Option<String>,
    pub category: String,
    pub icon: Option<String>,
    pub command: String,
    pub command_args: Vec<(String, String)>,
    pub shortcut_hint: Option<String>,
    pub source: ContributionSource,
}

/// How a side panel arranges itself relative to the page area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidePanelLayout {
    Float,
    Slide,
    Split,
}

/// Which edge of the window a side panel attaches to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidePanelSide {
    Left,
    Right,
}

/// A side panel registered with the browser's shared dock host.
#[derive(Clone, Debug, PartialEq)]
pub struct SidePanelEntry {
    pub id: SidePanelId,
    pub title: String,
    pub icon: String,
    pub default_layout: SidePanelLayout,
    pub default_side: SidePanelSide,
    pub default_width: f32,
    pub source: ContributionSource,
}

/// All active UI contributions published by the browser core.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Contributions {
    pub toolbar_items: Vec<ToolbarItem>,
    pub menu_entries: Vec<MenuEntry>,
    pub palette_entries: Vec<PaletteEntry>,
    pub side_panels: Vec<SidePanelEntry>,
}
