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

/// Visual presentation and role of a contributed toolbar item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToolbarItemKind {
    /// Standard icon button.
    #[default]
    Button,
    /// Pill-shaped button showing both its icon and title (for example, Chat).
    PillButton,
    /// The address bar slot rendered by the window chrome.
    UrlBar,
    /// Application menu trigger button that toggles view-local app menu state.
    AppMenu,
}

/// Concrete toolbar area where a toolbar item renders for a resolved window layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolbarArea {
    /// Leading group of the top bar (sidebar toggle, navigation buttons).
    TopBarStart,
    /// Flexible center slot of the top bar (address bar in multiple and collapsed layouts).
    TopBarCenter,
    /// Trailing group of the top bar before window controls (Zendia items, extensions, app menu).
    TopBarEnd,
    /// Leading group of the sidebar top row (app menu and sidebar toggle in single-toolbar layout).
    SidebarTopStart,
    /// Trailing group of the sidebar top row (navigation buttons in single-toolbar layout).
    SidebarTopEnd,
    /// Dedicated address bar row inside the sidebar (single-toolbar layout).
    SidebarUrlBar,
    /// Bottom foot row of the sidebar (library, Zendia items in single-toolbar layout, new tab).
    SidebarBottom,
}

/// Per-layout placement rules mapping one toolbar item across all toolbar layouts.
///
/// `None` hides the item from toolbars in that layout while keeping it registered in the toolbar model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ToolbarPlacementRules {
    pub single_toolbar: Option<ToolbarArea>,
    pub multiple_toolbars: Option<ToolbarArea>,
    pub collapsed_toolbar: Option<ToolbarArea>,
}

impl ToolbarPlacementRules {
    /// Default placement rules derived from a high-level [`ToolbarPlacement`] slot.
    pub const fn from_placement(placement: ToolbarPlacement) -> Self {
        match placement {
            ToolbarPlacement::NavigationStart => Self {
                single_toolbar: Some(ToolbarArea::SidebarTopEnd),
                multiple_toolbars: Some(ToolbarArea::TopBarStart),
                collapsed_toolbar: Some(ToolbarArea::TopBarStart),
            },
            ToolbarPlacement::NavigationEnd => Self {
                single_toolbar: Some(ToolbarArea::SidebarBottom),
                multiple_toolbars: Some(ToolbarArea::TopBarEnd),
                collapsed_toolbar: Some(ToolbarArea::TopBarEnd),
            },
            ToolbarPlacement::SidebarTop => Self {
                single_toolbar: Some(ToolbarArea::SidebarTopStart),
                multiple_toolbars: Some(ToolbarArea::TopBarStart),
                collapsed_toolbar: Some(ToolbarArea::TopBarStart),
            },
            ToolbarPlacement::SidebarBottom => Self {
                single_toolbar: Some(ToolbarArea::SidebarBottom),
                multiple_toolbars: Some(ToolbarArea::SidebarBottom),
                collapsed_toolbar: Some(ToolbarArea::SidebarBottom),
            },
            ToolbarPlacement::AddressBarEnd => Self {
                single_toolbar: Some(ToolbarArea::SidebarUrlBar),
                multiple_toolbars: Some(ToolbarArea::TopBarCenter),
                collapsed_toolbar: Some(ToolbarArea::TopBarCenter),
            },
        }
    }
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
    pub kind: ToolbarItemKind,
    pub placement: ToolbarPlacement,
    pub placement_rules: ToolbarPlacementRules,
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
