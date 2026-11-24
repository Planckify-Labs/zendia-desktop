/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The data types that make up the browser core's model snapshot.
//!
//! Views read this model to draw windows, tabs, spaces, panels, and service state. None of these types depend on any UI
//! toolkit so headless runners and background services can share the crate.

use crate::contributions::{Contributions, ToolbarArea, ToolbarPlacementRules};
use crate::events::Event;
use std::collections::BTreeMap;

/// Stable identifier for a browser window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(pub u64);

/// Stable identifier for a workspace (space).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpaceId(pub u64);

/// Stable identifier for a tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);

/// Stable identifier for a tab folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FolderId(pub u64);

/// Stable identifier for a split view group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SplitGroupId(pub u64);

/// Stable identifier for a download item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DownloadId(pub u64);

/// Stable identifier for a browsing history entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HistoryEntryId(pub u64);

/// Stable identifier for an active media playback session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MediaSessionId(pub u64);

/// Stable identifier for a sidebar notification card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NotificationId(pub u64);

/// One clickable link row inside a [`SidebarNotification`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarNotificationLink {
    pub label: String,
    pub icon: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub special: bool,
}

/// Core-published notification card displayed at the foot of the expanded sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarNotification {
    pub id: NotificationId,
    pub heading: String,
    pub links: Vec<SidebarNotificationLink>,
    pub auto_hide_ms: Option<u64>,
}

/// Destination section and insertion index for a `tab.move` reorder command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabMoveTarget {
    Normal { space_id: Option<SpaceId>, index: usize },
    Pinned { space_id: Option<SpaceId>, index: usize },
    Folder { folder_id: FolderId, index: usize },
    Essential { index: usize },
}

/// Stable identifier for an installed AI agent.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// Stable identifier for an agent conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConversationId(pub u64);

/// Stable identifier for a single step within an agent conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StepId(pub u64);

/// Stable identifier for an LLM model offered by an agent.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelId(pub String);

impl ModelId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// Stable identifier for a configured web chat site.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChatSiteId(pub String);

impl ChatSiteId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// Identifies one of the browser's child processes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ServiceId {
    Hub,
    InputServer,
    AgentServer,
    PluginHost,
    ExtensionServer,
}

/// A top-level browser window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub id: WindowId,
    pub title: String,
    pub active_space: Option<SpaceId>,
    pub active_tab: Option<TabId>,
}

/// Background gradient configuration for a space.
#[derive(Clone, Debug, PartialEq)]
pub struct SpaceGradient {
    pub stops: Vec<(u8, u8, u8)>,
    pub angle_deg: f32,
    pub opacity: f32,
}

/// A workspace grouping tabs, pinned tabs, and folders with its own theme tint.
#[derive(Clone, Debug, PartialEq)]
pub struct Space {
    pub id: SpaceId,
    pub name: String,
    pub icon: String,
    pub gradient: SpaceGradient,
    /// Last tab that was active while viewing this space, restored when switching back.
    pub last_active_tab: Option<TabId>,
}

/// Where a tab lives in the sidebar hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind {
    Normal,
    Pinned,
    Essential,
}

/// A single browser tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub id: TabId,
    /// The Hub view backing this tab, when attached to a live engine.
    pub view_id: Option<u64>,
    pub space_id: Option<SpaceId>,
    pub folder_id: Option<FolderId>,
    pub split_group_id: Option<SplitGroupId>,
    pub kind: TabKind,
    pub title: String,
    /// User-assigned custom title from `tab.rename`, preserved across page title updates.
    pub custom_title: Option<String>,
    pub url: String,
    pub favicon: Option<String>,
    pub is_loading: bool,
    pub can_navigate_back: bool,
    pub can_navigate_forward: bool,
    pub is_audible: bool,
    pub is_muted: bool,
    pub closing: bool,
}

impl Tab {
    /// Human-readable label shown in tab strips and pickers.
    pub fn display_title(&self) -> String {
        if let Some(custom) = &self.custom_title
            && !custom.is_empty()
        {
            custom.clone()
        } else if !self.title.is_empty() {
            self.title.clone()
        } else if !self.url.is_empty() && self.url != "about:blank" {
            self.url.clone()
        } else {
            "New Tab".to_string()
        }
    }
}

/// An essential tab tile pinned across spaces at the top of the sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Essential {
    pub tab_id: TabId,
    pub order: usize,
}

/// A tab pinned to a specific space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinnedTab {
    pub tab_id: TabId,
    pub space_id: SpaceId,
    pub order: usize,
}

/// A collapsible folder of tabs inside a space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    pub id: FolderId,
    pub space_id: SpaceId,
    pub name: String,
    pub icon: String,
    pub collapsed: bool,
    pub tab_ids: Vec<TabId>,
}

/// How panes in a split group are arranged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
    Grid,
}

/// A set of tabs displayed side by side in the page area.
#[derive(Clone, Debug, PartialEq)]
pub struct SplitGroup {
    pub id: SplitGroupId,
    pub space_id: SpaceId,
    pub direction: SplitDirection,
    pub tab_ids: Vec<TabId>,
    pub ratios: Vec<f32>,
}

/// Lifecycle state of a download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadStatus {
    InProgress,
    Completed,
    Paused,
    Failed { reason: String },
    Cancelled,
}

/// A file download shown in the library and download indicators.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Download {
    pub id: DownloadId,
    pub filename: String,
    pub url: String,
    pub bytes_received: u64,
    pub total_bytes: Option<u64>,
    pub status: DownloadStatus,
}

/// A visited page recorded in browsing history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: HistoryEntryId,
    pub url: String,
    pub title: String,
    pub visited_at_unix: u64,
    pub visit_count: u32,
}

/// Media currently playing in a tab, surfaced in the sidebar media controls.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaSession {
    pub id: MediaSessionId,
    pub tab_id: TabId,
    pub title: String,
    pub artist: String,
    pub is_playing: bool,
    pub is_muted: bool,
    pub position_secs: f32,
    pub duration_secs: Option<f32>,
}

/// An available AI agent that can run sessions in Zendia.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agent {
    pub id: AgentId,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub accent_rgb: (u8, u8, u8),
    pub available: bool,
}

/// What kind of work a conversation step represents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepKind {
    UserPrompt,
    Thinking,
    ToolCall { tool: String },
    Response,
    Confirmation { prompt: String, resolved: Option<bool> },
}

/// Execution state of a conversation step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    WaitingForUser,
}

/// One entry in an agent conversation's trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationStep {
    pub id: StepId,
    pub kind: StepKind,
    pub summary: String,
    pub detail: Option<String>,
    pub status: StepStatus,
}

/// Aggregate state of an agent conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversationStatus {
    Idle,
    Working,
    WaitingForUser,
    Finished,
    Stopped,
}

/// A single conversation with an AI agent, optionally scoped to a tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversation {
    pub id: ConversationId,
    pub agent_id: AgentId,
    pub tab_id: Option<TabId>,
    pub title: String,
    pub model_id: ModelId,
    pub status: ConversationStatus,
    pub steps: Vec<ConversationStep>,
}

/// Reasoning effort setting exposed by the model card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelEffort {
    Low,
    Medium,
    High,
}

/// Public metadata for a selectable language model (API keys never leave AgentServer).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: ModelId,
    pub name: String,
    pub provider: String,
    pub context_window: u32,
    pub effort: ModelEffort,
}

/// A web chat website loaded inside the web chat side panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatSite {
    pub id: ChatSiteId,
    pub name: String,
    pub url: String,
    pub icon: String,
}

/// Health state of one of Zendia's background service processes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceStatus {
    Running,
    Starting,
    Restarting,
    Stopped { reason: Option<String> },
}

/// Current status of a child service, shown by service-backed surfaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceState {
    pub service: ServiceId,
    pub status: ServiceStatus,
}

/// Keyboard input mode published by InputServer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputMode {
    #[default]
    Normal,
    Insert,
    Visual,
    Explorer,
    Toolbar,
    Library,
    Window,
}

/// One next-key suggestion shown in the which-key popup, published by InputServer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhichKeyEntry {
    pub key: String,
    pub description: String,
    pub is_group: bool,
}

/// Active which-key popup state published by InputServer when waiting on a multi-key chord.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WhichKeyState {
    pub title: String,
    pub entries: Vec<WhichKeyEntry>,
}

/// User-selected color scheme preference stored in profile settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeScheme {
    #[default]
    System,
    Light,
    Dark,
}

/// Color override for either both schemes or a light/dark pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeColorOverride {
    Single(String),
    Pair { light: String, dark: String },
}

/// Role override pointing at a palette entry, another role, a `#rrggbb` hex color, or a light/dark pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeRoleOverride {
    Single(String),
    Pair { light: String, dark: String },
}

/// Glass surface tuning overrides (matching `zendia.theme`'s `glass` block).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeGlassOverrides {
    /// Surface opacity in `0.0..=1.0`.
    pub opacity: Option<f32>,
    /// Backdrop blur radius in logical pixels (`>= 0.0`).
    pub blur: Option<f32>,
}

/// User overrides layered over Zendia's default Tokyonight theme.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeOverrides {
    pub palette: BTreeMap<String, ThemeColorOverride>,
    pub roles: BTreeMap<String, ThemeRoleOverride>,
    pub glass: ThemeGlassOverrides,
    pub agents: BTreeMap<String, String>,
}

/// Profile theme settings owned by the browser core and resolved by views.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeSettings {
    pub scheme: ThemeScheme,
    pub overrides: ThemeOverrides,
}

/// Default expanded sidebar width in logical pixels (matching Zen's Linux default).
pub const SIDEBAR_DEFAULT_WIDTH: f32 = 186.0;
/// Minimum expanded sidebar width in logical pixels (matching `--zen-toolbox-min-width`).
pub const SIDEBAR_MIN_WIDTH: f32 = 163.0;
/// Maximum expanded sidebar width in logical pixels (matching `zen.view.sidebar-expanded.max-width`).
pub const SIDEBAR_MAX_WIDTH: f32 = 500.0;
/// Fixed width of the collapsed icon-only sidebar in logical pixels (`48px` tab column + `6px` padding each side).
pub const SIDEBAR_COLLAPSED_WIDTH: f32 = 60.0;

/// Toolbar layout mode, corresponding to Zen's `zen.view.use-single-toolbar` and `zen.view.sidebar-expanded`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToolbarLayout {
    /// Buttons and address bar sit inside the expanded sidebar with no top toolbar.
    #[default]
    Single,
    /// Full-width top toolbar paired with an expanded vertical tab sidebar.
    Multiple,
    /// Full-width top toolbar paired with a collapsed icon-only sidebar.
    Collapsed,
}

/// Horizontal edge of the window where the vertical sidebar sits (`zen.tabs.vertical.right-side`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SidebarSide {
    #[default]
    Left,
    Right,
}

/// Profile layout settings owned by the browser core and read by views.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutSettings {
    pub toolbar_layout: ToolbarLayout,
    pub sidebar_side: SidebarSide,
    pub sidebar_expanded: bool,
    pub sidebar_width: f32,
    /// Compact mode profile setting (auto-hiding chrome is implemented in phase 4).
    pub compact_mode: bool,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            toolbar_layout: ToolbarLayout::Single,
            sidebar_side: SidebarSide::Left,
            sidebar_expanded: true,
            sidebar_width: SIDEBAR_DEFAULT_WIDTH,
            compact_mode: false,
        }
    }
}

impl LayoutSettings {
    /// Clamps a candidate sidebar width into `[SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH]`.
    pub fn clamp_sidebar_width(width: f32) -> f32 {
        if width.is_finite() {
            width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
        } else {
            SIDEBAR_DEFAULT_WIDTH
        }
    }

    /// Logical width the sidebar occupies in its current expanded/collapsed state.
    pub fn effective_sidebar_width(&self) -> f32 {
        if self.sidebar_expanded {
            Self::clamp_sidebar_width(self.sidebar_width)
        } else {
            SIDEBAR_COLLAPSED_WIDTH
        }
    }

    /// Whether the active toolbar layout renders a full-width top toolbar above the sidebar and page.
    pub fn has_top_toolbar(&self) -> bool {
        !matches!(self.toolbar_layout, ToolbarLayout::Single)
    }

    /// Resolves which [`ToolbarArea`] an item occupies under this layout's toolbar mode.
    pub fn resolve_toolbar_area(&self, rules: ToolbarPlacementRules) -> Option<ToolbarArea> {
        match self.toolbar_layout {
            ToolbarLayout::Single => rules.single_toolbar,
            ToolbarLayout::Multiple => rules.multiple_toolbars,
            ToolbarLayout::Collapsed => rules.collapsed_toolbar,
        }
    }
}

/// Complete snapshot of browser state owned by the core and read by views.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BrowserModel {
    pub windows: Vec<Window>,
    pub spaces: Vec<Space>,
    pub active_space: Option<SpaceId>,
    pub tabs: Vec<Tab>,
    pub active_tab: Option<TabId>,
    pub essentials: Vec<Essential>,
    pub pinned_tabs: Vec<PinnedTab>,
    pub folders: Vec<Folder>,
    pub split_groups: Vec<SplitGroup>,
    pub downloads: Vec<Download>,
    pub history: Vec<HistoryEntry>,
    pub media_sessions: Vec<MediaSession>,
    pub sidebar_notifications: Vec<SidebarNotification>,
    pub show_essentials_promo: bool,
    pub contributions: Contributions,
    pub agents: Vec<Agent>,
    pub conversations: Vec<Conversation>,
    pub models: Vec<ModelInfo>,
    pub chat_sites: Vec<ChatSite>,
    pub services: Vec<ServiceState>,
    pub input_mode: InputMode,
    pub pending_keys: Vec<String>,
    pub which_key: Option<WhichKeyState>,
    pub theme: ThemeSettings,
    pub layout: LayoutSettings,
    pub vim_mode_enabled: bool,
    pub zoom_level: f32,
}

impl BrowserModel {
    pub fn tab(&self, tab_id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id == tab_id)
    }

    pub fn tab_by_view_id(&self, view_id: u64) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.view_id == Some(view_id))
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.active_tab.and_then(|id| self.tab(id))
    }

    pub fn space(&self, space_id: SpaceId) -> Option<&Space> {
        self.spaces.iter().find(|space| space.id == space_id)
    }

    pub fn folder(&self, folder_id: FolderId) -> Option<&Folder> {
        self.folders.iter().find(|folder| folder.id == folder_id)
    }

    pub fn active_space(&self) -> Option<&Space> {
        self.active_space.and_then(|id| self.space(id))
    }

    /// Essential tabs shared across all spaces, in their configured tile order.
    pub fn essential_tabs(&self) -> Vec<&Tab> {
        let mut ordered = self.essentials.clone();
        ordered.sort_by_key(|item| item.order);
        let mut out = Vec::new();
        for item in &ordered {
            if let Some(tab) = self.tab(item.tab_id)
                && tab.kind == TabKind::Essential
            {
                out.push(tab);
            }
        }
        for tab in &self.tabs {
            if tab.kind == TabKind::Essential && !out.iter().any(|existing| existing.id == tab.id) {
                out.push(tab);
            }
        }
        out
    }

    /// Top-level pinned tabs belonging to `space_id` (excluding tabs nested inside folders), in pinned order.
    pub fn pinned_tabs_in_space(&self, space_id: SpaceId) -> Vec<&Tab> {
        let mut ordered: Vec<&PinnedTab> = self
            .pinned_tabs
            .iter()
            .filter(|item| item.space_id == space_id)
            .collect();
        ordered.sort_by_key(|item| item.order);
        let mut out = Vec::new();
        for item in ordered {
            if let Some(tab) = self.tab(item.tab_id)
                && tab.kind == TabKind::Pinned
                && tab.space_id == Some(space_id)
                && tab.folder_id.is_none()
            {
                out.push(tab);
            }
        }
        for tab in &self.tabs {
            if tab.kind == TabKind::Pinned
                && tab.space_id == Some(space_id)
                && tab.folder_id.is_none()
                && !out.iter().any(|existing| existing.id == tab.id)
            {
                out.push(tab);
            }
        }
        out
    }

    /// Folders belonging to `space_id`, in sidebar order.
    pub fn folders_in_space(&self, space_id: SpaceId) -> Vec<&Folder> {
        self.folders
            .iter()
            .filter(|folder| folder.space_id == space_id)
            .collect()
    }

    /// All tabs contained in `folder_id`, in folder order.
    pub fn folder_tabs(&self, folder_id: FolderId) -> Vec<&Tab> {
        let Some(folder) = self.folder(folder_id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for tab_id in &folder.tab_ids {
            if let Some(tab) = self.tab(*tab_id)
                && tab.folder_id == Some(folder_id)
            {
                out.push(tab);
            }
        }
        for tab in &self.tabs {
            if tab.folder_id == Some(folder_id) && !out.iter().any(|existing| existing.id == tab.id) {
                out.push(tab);
            }
        }
        out
    }

    /// Tabs currently visible under `folder_id`: all tabs when expanded, or only the active tab when collapsed.
    pub fn visible_folder_tabs(&self, folder_id: FolderId) -> Vec<&Tab> {
        let Some(folder) = self.folder(folder_id) else {
            return Vec::new();
        };
        let all = self.folder_tabs(folder_id);
        if !folder.collapsed {
            return all;
        }
        all.into_iter().filter(|tab| self.active_tab == Some(tab.id)).collect()
    }

    /// Normal (unpinned, non-essential, non-folder) tabs belonging to `space_id`, in sidebar order.
    pub fn normal_tabs_in_space(&self, space_id: SpaceId) -> Vec<&Tab> {
        self.tabs
            .iter()
            .filter(|tab| tab.kind == TabKind::Normal && tab.space_id == Some(space_id) && tab.folder_id.is_none())
            .collect()
    }

    /// Ordered list of tabs reachable in the active space (essentials, pinned, folders, then normal tabs).
    pub fn tabs_in_active_space_order(&self) -> Vec<TabId> {
        let mut out: Vec<TabId> = self.essential_tabs().into_iter().map(|tab| tab.id).collect();
        if let Some(space_id) = self.active_space {
            for tab in self.pinned_tabs_in_space(space_id) {
                out.push(tab.id);
            }
            for folder in self.folders_in_space(space_id) {
                for tab in self.folder_tabs(folder.id) {
                    out.push(tab.id);
                }
            }
            for tab in self.normal_tabs_in_space(space_id) {
                out.push(tab.id);
            }
        } else {
            for tab in &self.tabs {
                if !out.contains(&tab.id) {
                    out.push(tab.id);
                }
            }
        }
        out
    }

    pub fn service_state(&self, service: ServiceId) -> Option<&ServiceState> {
        self.services.iter().find(|state| state.service == service)
    }

    /// Applies an event published by the core to keep a view's cached snapshot in sync.
    ///
    /// Returns `true` if the snapshot changed, or `false` when the event is view-local (such as `PanelToggled`) or
    /// requires a full snapshot refresh (`ModelReplaced`).
    pub fn apply_event(&mut self, event: &Event) -> bool {
        match event {
            Event::TabOpened { tab, index } => {
                if self.tabs.iter().any(|existing| existing.id == tab.id) {
                    return false;
                }
                let insert_at = (*index).min(self.tabs.len());
                self.tabs.insert(insert_at, tab.clone());
                true
            }
            Event::TabUpdated { tab } => {
                if let Some(existing) = self.tabs.iter_mut().find(|existing| existing.id == tab.id) {
                    *existing = tab.clone();
                    true
                } else {
                    false
                }
            }
            Event::TabClosed { tab_id } => {
                let before = self.tabs.len();
                self.tabs.retain(|tab| tab.id != *tab_id);
                if self.active_tab == Some(*tab_id) {
                    self.active_tab = None;
                }
                self.essentials.retain(|item| item.tab_id != *tab_id);
                self.pinned_tabs.retain(|item| item.tab_id != *tab_id);
                for folder in &mut self.folders {
                    folder.tab_ids.retain(|id| id != tab_id);
                }
                for space in &mut self.spaces {
                    if space.last_active_tab == Some(*tab_id) {
                        space.last_active_tab = None;
                    }
                }
                for group in &mut self.split_groups {
                    group.tab_ids.retain(|id| id != tab_id);
                }
                self.split_groups.retain(|group| group.tab_ids.len() > 1);
                for window in &mut self.windows {
                    if window.active_tab == Some(*tab_id) {
                        window.active_tab = None;
                    }
                }
                self.tabs.len() != before
            }
            Event::TabActivated { tab_id } => {
                self.active_tab = *tab_id;
                if let Some(window) = self.windows.first_mut() {
                    window.active_tab = *tab_id;
                }
                if let Some(id) = *tab_id
                    && let Some(space_id) = self.tab(id).and_then(|tab| tab.space_id)
                    && let Some(space) = self.spaces.iter_mut().find(|s| s.id == space_id)
                {
                    space.last_active_tab = Some(id);
                }
                true
            }
            Event::SpaceSwitched { space_id } => {
                self.active_space = Some(*space_id);
                if let Some(window) = self.windows.first_mut() {
                    window.active_space = Some(*space_id);
                }
                true
            }
            Event::SpacesUpdated { spaces } => {
                self.spaces = spaces.clone();
                true
            }
            Event::TabsReordered {
                tabs,
                essentials,
                pinned_tabs,
                folders,
            } => {
                self.tabs = tabs.clone();
                self.essentials = essentials.clone();
                self.pinned_tabs = pinned_tabs.clone();
                self.folders = folders.clone();
                true
            }
            Event::EssentialsChanged { essentials } => {
                self.essentials = essentials.clone();
                true
            }
            Event::PinnedTabsChanged { pinned_tabs } => {
                self.pinned_tabs = pinned_tabs.clone();
                true
            }
            Event::FolderCreated { folder } => {
                if self.folders.iter().any(|existing| existing.id == folder.id) {
                    return false;
                }
                self.folders.push(folder.clone());
                true
            }
            Event::FolderUpdated { folder } => {
                if let Some(existing) = self.folders.iter_mut().find(|existing| existing.id == folder.id) {
                    *existing = folder.clone();
                    true
                } else {
                    false
                }
            }
            Event::FolderDeleted { folder_id } => {
                let before = self.folders.len();
                self.folders.retain(|folder| folder.id != *folder_id);
                self.folders.len() != before
            }
            Event::SidebarNotificationAdded { notification } => {
                if let Some(existing) = self
                    .sidebar_notifications
                    .iter_mut()
                    .find(|item| item.id == notification.id)
                {
                    *existing = notification.clone();
                } else {
                    self.sidebar_notifications.push(notification.clone());
                }
                true
            }
            Event::SidebarNotificationDismissed { notification_id } => {
                let before = self.sidebar_notifications.len();
                self.sidebar_notifications.retain(|item| item.id != *notification_id);
                self.sidebar_notifications.len() != before
            }
            Event::EssentialsPromoChanged { visible } => {
                self.show_essentials_promo = *visible;
                true
            }
            Event::SplitGroupUpdated { group } => {
                if let Some(existing) = self.split_groups.iter_mut().find(|item| item.id == group.id) {
                    *existing = group.clone();
                } else {
                    self.split_groups.push(group.clone());
                }
                true
            }
            Event::ModeChanged { mode } => {
                self.input_mode = *mode;
                true
            }
            Event::PendingKeysChanged { keys } => {
                self.pending_keys = keys.clone();
                true
            }
            Event::WhichKeyChanged { state } => {
                self.which_key = state.clone();
                true
            }
            Event::ConversationUpdated { conversation } => {
                if let Some(existing) = self.conversations.iter_mut().find(|item| item.id == conversation.id) {
                    *existing = conversation.clone();
                } else {
                    self.conversations.push(conversation.clone());
                }
                true
            }
            Event::ServiceStateChanged { state } => {
                if let Some(existing) = self.services.iter_mut().find(|item| item.service == state.service) {
                    *existing = state.clone();
                } else {
                    self.services.push(state.clone());
                }
                true
            }
            Event::ContributionsChanged { contributions } => {
                self.contributions = contributions.clone();
                true
            }
            Event::SettingsChanged { theme } => {
                self.theme = theme.clone();
                true
            }
            Event::LayoutSettingsChanged { layout } => {
                self.layout = layout.clone();
                true
            }
            Event::VimModeChanged { enabled } => {
                self.vim_mode_enabled = *enabled;
                true
            }
            Event::ZoomChanged { zoom_level } => {
                self.zoom_level = *zoom_level;
                true
            }
            Event::PanelToggled { .. }
            | Event::LocationFocusRequested
            | Event::AppMenuToggleRequested
            | Event::AboutDialogRequested
            | Event::QuitRequested
            | Event::ClipboardCopyRequested { .. }
            | Event::TabRenameRequested { .. }
            | Event::FolderRenameRequested { .. }
            | Event::ModelReplaced => false,
        }
    }
}
