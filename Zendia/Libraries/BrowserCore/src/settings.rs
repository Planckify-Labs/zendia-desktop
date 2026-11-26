/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Native settings catalog, profile settings, command bindings, and live search.
//!
//! Each setting entry binds to an existing profile setting and command on [`BrowserModel`] where one already exists
//! (layout, compact mode, sidebar side, theme, vim mode, statusline, side panels, agents, web chat). Profile settings
//! introduced by Phase 13 (Glance triggers, pinned tab close behavior, vertical tab options, workspace behaviors,
//! agent automation toggles) live in [`BrowserSettings`].

use crate::agent::{AGENT_PANEL_ID, AGENTS_PANEL_ID};
use crate::commands::Command;
use crate::contributions::{SidePanelId, SidePanelLayout, SidePanelSide};
use crate::input_state::StatuslineShow;
use crate::model::{AgentId, BrowserModel, CompactHide, SidebarSide, ThemeScheme, ToolbarLayout};
use crate::webchat::WEBCHAT_PANEL_ID;

/// Top-level navigation groups in Zendia's native settings window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SettingsGroupId {
    #[default]
    LookAndFeel,
    TabManagement,
    Spaces,
    Glance,
    Vim,
    Agents,
    WebChat,
    Plugins,
    Extensions,
}

impl SettingsGroupId {
    /// All groups in sidebar display order.
    pub const ALL: &'static [Self] = &[
        Self::LookAndFeel,
        Self::TabManagement,
        Self::Spaces,
        Self::Glance,
        Self::Vim,
        Self::Agents,
        Self::WebChat,
        Self::Plugins,
        Self::Extensions,
    ];

    /// Groups matching Zen's settings sidebar (shown above the Zendia divider).
    pub const ZEN_GROUPS: &'static [Self] = &[Self::LookAndFeel, Self::TabManagement, Self::Spaces, Self::Glance];

    /// Zendia-specific settings groups.
    pub const ZENDIA_GROUPS: &'static [Self] = &[Self::Vim, Self::Agents, Self::WebChat];

    /// Module and service groups (plugins, extensions, background services).
    pub const MODULE_GROUPS: &'static [Self] = &[Self::Plugins, Self::Extensions];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::LookAndFeel => "look-and-feel",
            Self::TabManagement => "tab-management",
            Self::Spaces => "spaces",
            Self::Glance => "glance",
            Self::Vim => "vim",
            Self::Agents => "agents",
            Self::WebChat => "webchat",
            Self::Plugins => "plugins",
            Self::Extensions => "extensions",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "look-and-feel" | "look_and_feel" | "looks" | "appearance" | "theme" => Some(Self::LookAndFeel),
            "tab-management" | "tab_management" | "tabs" | "pinned" => Some(Self::TabManagement),
            "spaces" | "workspaces" => Some(Self::Spaces),
            "glance" => Some(Self::Glance),
            "vim" | "vim-mode" | "keys" | "shortcuts" => Some(Self::Vim),
            "agents" | "agent" | "ai" => Some(Self::Agents),
            "webchat" | "web-chat" | "chat" => Some(Self::WebChat),
            "plugins" | "mods" => Some(Self::Plugins),
            "extensions" | "services" | "extensions-services" => Some(Self::Extensions),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::LookAndFeel => "Look and Feel",
            Self::TabManagement => "Tab Management",
            Self::Spaces => "Spaces",
            Self::Glance => "Glance",
            Self::Vim => "Vim Mode",
            Self::Agents => "Agents",
            Self::WebChat => "Web Chat",
            Self::Plugins => "Plugins",
            Self::Extensions => "Extensions & Services",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::LookAndFeel => "paintbrush",
            Self::TabManagement => "tab",
            Self::Spaces => "library-spaces",
            Self::Glance => "eye",
            Self::Vim => "vim",
            Self::Agents => "chat",
            Self::WebChat => "webchat",
            Self::Plugins => "sparkles",
            Self::Extensions => "extension",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::LookAndFeel => "Browser layout, vertical tabs, compact mode, window dragging, and theme",
            Self::TabManagement => "Tab cycling, pinned tab behavior, and essentials",
            Self::Spaces => "Workspaces, containers, and space routing",
            Self::Glance => "Quick link previews without leaving the current tab",
            Self::Vim => "Optional keyboard-driven browsing and statusline behavior",
            Self::Agents => "Default AI agent, dock layout, instructions, and automation rules",
            Self::WebChat => "Side-panel chat sites and dock layout",
            Self::Plugins => "Sandboxed plugins, declared UI panels, and capabilities",
            Self::Extensions => "WebExtensions toolbar items and background service health",
        }
    }
}

/// Modifier key that opens a link in Glance (`zen.glance.activation-method`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GlanceTrigger {
    #[default]
    Ctrl,
    Alt,
    Shift,
    Meta,
}

impl GlanceTrigger {
    pub const ALL: &'static [Self] = &[Self::Ctrl, Self::Alt, Self::Shift, Self::Meta];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ctrl => "ctrl",
            Self::Alt => "alt",
            Self::Shift => "shift",
            Self::Meta => "meta",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some(Self::Ctrl),
            "alt" | "option" => Some(Self::Alt),
            "shift" => Some(Self::Shift),
            "meta" | "cmd" | "command" | "super" => Some(Self::Meta),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ctrl => "Ctrl + Click",
            Self::Alt => "Alt + Click",
            Self::Shift => "Shift + Click",
            Self::Meta => "Meta + Click",
        }
    }
}

/// Action performed when closing a pinned tab with the close-tab shortcut (`zen.pinned-tab-manager.close-shortcut-behavior`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PinnedTabCloseBehavior {
    ResetUnloadSwitch,
    UnloadSwitch,
    ResetSwitch,
    #[default]
    Switch,
    Reset,
    Close,
}

impl PinnedTabCloseBehavior {
    pub const ALL: &'static [Self] = &[
        Self::ResetUnloadSwitch,
        Self::UnloadSwitch,
        Self::ResetSwitch,
        Self::Switch,
        Self::Reset,
        Self::Close,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ResetUnloadSwitch => "reset-unload-switch",
            Self::UnloadSwitch => "unload-switch",
            Self::ResetSwitch => "reset-switch",
            Self::Switch => "switch",
            Self::Reset => "reset",
            Self::Close => "close",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "reset-unload-switch" => Some(Self::ResetUnloadSwitch),
            "unload-switch" => Some(Self::UnloadSwitch),
            "reset-switch" => Some(Self::ResetSwitch),
            "switch" => Some(Self::Switch),
            "reset" => Some(Self::Reset),
            "close" => Some(Self::Close),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ResetUnloadSwitch => "Reset URL, unload and switch to next tab",
            Self::UnloadSwitch => "Unload and switch to next tab",
            Self::ResetSwitch => "Reset URL and switch to next tab",
            Self::Switch => "Switch to next tab",
            Self::Reset => "Reset URL",
            Self::Close => "Close tab",
        }
    }
}

/// Additional profile settings not already represented on another specialized field of [`BrowserModel`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserSettings {
    // Look and Feel
    pub show_new_tab_on_tab_list: bool,
    pub new_tab_button_top: bool,
    pub compact_toolbar_flash_popup: bool,
    pub compact_show_on_hover: bool,
    pub compact_animate_sidebar: bool,
    pub compact_show_background_tab_toast: bool,
    pub window_drag_from_content: bool,
    pub grey_out_inactive_windows: bool,

    // Tab Management & Spaces
    pub sync_only_pinned_tabs: bool,
    pub hide_default_container_indicator: bool,
    pub force_container_workspace: bool,
    pub close_on_back_with_no_history: bool,
    pub ctrl_tab_ignore_pending_tabs: bool,
    pub ctrl_tab_cycle_by_attribute: bool,
    pub select_recently_used_on_close: bool,
    pub continue_where_left_off: bool,
    pub restore_pinned_tabs_on_startup: bool,
    pub separate_essentials_per_container: bool,
    pub pinned_tab_close_behavior: PinnedTabCloseBehavior,

    // Glance
    pub glance_enabled: bool,
    pub glance_trigger: GlanceTrigger,

    // Agents
    pub agent_steal_focus: bool,
    pub agent_loop_enabled: bool,

    // Plugins
    pub plugins_auto_update: bool,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            show_new_tab_on_tab_list: true,
            new_tab_button_top: true,
            compact_toolbar_flash_popup: false,
            compact_show_on_hover: true,
            compact_animate_sidebar: true,
            compact_show_background_tab_toast: true,
            window_drag_from_content: true,
            grey_out_inactive_windows: true,

            sync_only_pinned_tabs: false,
            hide_default_container_indicator: true,
            force_container_workspace: false,
            close_on_back_with_no_history: true,
            ctrl_tab_ignore_pending_tabs: false,
            ctrl_tab_cycle_by_attribute: false,
            select_recently_used_on_close: true,
            continue_where_left_off: false,
            restore_pinned_tabs_on_startup: false,
            separate_essentials_per_container: true,
            pinned_tab_close_behavior: PinnedTabCloseBehavior::ResetUnloadSwitch,

            glance_enabled: true,
            glance_trigger: GlanceTrigger::Ctrl,

            agent_steal_focus: false,
            agent_loop_enabled: true,

            plugins_auto_update: true,
        }
    }
}

/// One selectable option for a choice setting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingOption {
    pub value: String,
    pub label: String,
}

impl SettingOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// Control presentation for a setting in the catalog and search results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingControl {
    Toggle,
    Choice(Vec<SettingOption>),
    Action { button_label: String },
}

/// Descriptor for one setting in Zendia's native settings catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingEntry {
    pub key: String,
    pub group: SettingsGroupId,
    pub section: String,
    pub title: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub control: SettingControl,
    /// Primary browser core command invoked when this setting changes.
    pub command_name: String,
}

fn opt(value: &str, label: &str) -> SettingOption {
    SettingOption::new(value, label)
}

impl SettingEntry {
    fn new(
        key: &str,
        group: SettingsGroupId,
        section: &str,
        title: &str,
        description: &str,
        control: SettingControl,
    ) -> Self {
        Self {
            key: key.to_string(),
            group,
            section: section.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            keywords: Vec::new(),
            control,
            command_name: "settings.set".to_string(),
        }
    }

    fn keywords(mut self, keywords: &[&str]) -> Self {
        self.keywords = keywords.iter().map(|kw| (*kw).to_string()).collect();
        self
    }

    fn command(mut self, command_name: &str) -> Self {
        self.command_name = command_name.to_string();
        self
    }
}

/// Returns the complete ordered catalog of settings across all groups.
pub fn settings_catalog() -> Vec<SettingEntry> {
    vec![
        // --- Look and Feel ---
        SettingEntry::new(
            "layout.toolbar_layout",
            SettingsGroupId::LookAndFeel,
            "Sidebar and tabs layout",
            "Browser Layout",
            "Choose between Only Sidebar, Sidebar and Top Toolbar, or Collapsed Sidebar.",
            SettingControl::Choice(vec![
                opt("single", "Only Sidebar"),
                opt("multiple", "Sidebar and Top Toolbar"),
                opt("collapsed", "Collapsed Sidebar"),
            ]),
        )
        .keywords(&["single", "multiple", "collapsed", "toolbar", "sidebar", "layout"])
        .command("layout.set_toolbar_layout"),
        SettingEntry::new(
            "layout.sidebar_side",
            SettingsGroupId::LookAndFeel,
            "Sidebar and tabs layout",
            "Sidebar position",
            "Place the vertical tab sidebar on the left or right edge of the window.",
            SettingControl::Choice(vec![opt("left", "Left"), opt("right", "Right")]),
        )
        .keywords(&["left", "right", "vertical", "edge"])
        .command("sidebar.set_side"),
        SettingEntry::new(
            "tabs.show_newtab_vertical",
            SettingsGroupId::LookAndFeel,
            "Vertical Tabs",
            "Show New Tab Button on Tab List",
            "Display a quick New Tab row directly inside the vertical tab list.",
            SettingControl::Toggle,
        )
        .keywords(&["new tab", "vertical tabs", "button"]),
        SettingEntry::new(
            "tabs.newtab_button_top",
            SettingsGroupId::LookAndFeel,
            "Vertical Tabs",
            "Move the new tab button to the top",
            "Keep the New Tab button pinned above normal tabs instead of below them.",
            SettingControl::Toggle,
        )
        .keywords(&["new tab", "top", "vertical tabs"]),
        SettingEntry::new(
            "layout.compact_mode",
            SettingsGroupId::LookAndFeel,
            "Compact mode",
            "Enable compact mode",
            "Hide chrome around the page and reveal the sidebar or toolbar on edge hover.",
            SettingControl::Toggle,
        )
        .keywords(&["compact", "hide", "fullscreen", "minimal"])
        .command("layout.set_compact_mode"),
        SettingEntry::new(
            "layout.compact_hide",
            SettingsGroupId::LookAndFeel,
            "Compact mode",
            "Hide in compact mode",
            "Choose whether compact mode hides the sidebar, the top toolbar, or both.",
            SettingControl::Choice(vec![
                opt("both", "Hide Both"),
                opt("sidebar", "Hide Tab Bar"),
                opt("toolbar", "Hide Top Bar"),
            ]),
        )
        .keywords(&["compact", "hide", "sidebar", "toolbar"])
        .command("layout.set_compact_hide"),
        SettingEntry::new(
            "compact.toolbar_flash_popup",
            SettingsGroupId::LookAndFeel,
            "Compact mode",
            "Briefly make the toolbar popup when switching or opening new tabs in compact mode",
            "Flash the hidden toolbar briefly on tab switch so you can see where you landed.",
            SettingControl::Toggle,
        )
        .keywords(&["compact", "flash", "popup", "toolbar"]),
        SettingEntry::new(
            "compact.show_on_hover",
            SettingsGroupId::LookAndFeel,
            "Compact mode",
            "Show hidden toolbars when hovering near the window edge",
            "Reveal the sidebar or top toolbar automatically when moving the pointer to the edge in compact mode.",
            SettingControl::Toggle,
        )
        .keywords(&["compact", "hover", "reveal", "sidebar", "toolbar"]),
        SettingEntry::new(
            "compact.animate_sidebar",
            SettingsGroupId::LookAndFeel,
            "Compact mode",
            "Animate sidebar reveal in compact mode",
            "Use Zendia's smooth slide animation when revealing and hiding the compact sidebar.",
            SettingControl::Toggle,
        )
        .keywords(&["compact", "animate", "sidebar", "motion"]),
        SettingEntry::new(
            "compact.show_background_tab_toast",
            SettingsGroupId::LookAndFeel,
            "Compact mode",
            "Show notification toast when opening a tab in the background",
            "Display a brief toast confirming that a link opened in a background tab while the tab bar is hidden.",
            SettingControl::Toggle,
        )
        .keywords(&["compact", "toast", "background tab", "notification"]),
        SettingEntry::new(
            "window.drag_from_content",
            SettingsGroupId::LookAndFeel,
            "Window dragging",
            "Allow dragging the window from web pages",
            "Move the window by dragging empty space at the top of websites, just like the titlebar.",
            SettingControl::Toggle,
        )
        .keywords(&["drag", "titlebar", "window"]),
        SettingEntry::new(
            "layout.tab_switch_motion",
            SettingsGroupId::LookAndFeel,
            "Motion",
            "Animate page transition when switching tabs",
            "Slide the page gently in the direction of the selected tab in the sidebar.",
            SettingControl::Toggle,
        )
        .keywords(&["motion", "animation", "slide", "tab switch"])
        .command("layout.set_tab_switch_motion"),
        SettingEntry::new(
            "theme.scheme",
            SettingsGroupId::LookAndFeel,
            "Theme Settings",
            "Color scheme",
            "Follow your system appearance or lock Zendia's Tokyonight theme to Dark or Light.",
            SettingControl::Choice(vec![
                opt("system", "System"),
                opt("dark", "Dark (Tokyonight Moon)"),
                opt("light", "Light (Tokyonight Day)"),
            ]),
        )
        .keywords(&["dark", "light", "system", "tokyonight", "appearance", "theme"])
        .command("theme.set"),
        // --- Tab Management ---
        SettingEntry::new(
            "workspaces.sync_only_pinned_tabs",
            SettingsGroupId::TabManagement,
            "Workspaces",
            "Sync only pinned tabs in workspaces",
            "Keep regular unpinned tabs local to each window while syncing pinned tabs across windows.",
            SettingControl::Toggle,
        )
        .keywords(&["sync", "pinned", "workspaces"]),
        SettingEntry::new(
            "workspaces.hide_default_container_indicator",
            SettingsGroupId::TabManagement,
            "Workspaces",
            "Hide the default container indicator in the tab bar",
            "Avoid showing a container badge on tabs using the space's default container.",
            SettingControl::Toggle,
        )
        .keywords(&["container", "indicator", "tab bar"]),
        SettingEntry::new(
            "workspaces.force_container_workspace",
            SettingsGroupId::TabManagement,
            "Workspaces",
            "Switch to workspace where container is set as default when opening container tabs",
            "Automatically move focus to the matching workspace when opening a containerized tab.",
            SettingControl::Toggle,
        )
        .keywords(&["container", "workspace", "switch"]),
        SettingEntry::new(
            "tabs.close_on_back_with_no_history",
            SettingsGroupId::TabManagement,
            "Workspaces",
            "Close tab and switch to its owner tab when going back with no history",
            "Pressing Back on the first page of a spawned tab closes it and returns to its opener.",
            SettingControl::Toggle,
        )
        .keywords(&["back", "history", "close", "owner"]),
        SettingEntry::new(
            "tabs.ctrl_tab_ignore_pending",
            SettingsGroupId::TabManagement,
            "Workspaces",
            "Ignore Pending tabs when cycling with Ctrl+Tab",
            "Skip unloaded background tabs when cycling through tabs from the keyboard.",
            SettingControl::Toggle,
        )
        .keywords(&["ctrl+tab", "pending", "cycle", "unloaded"]),
        SettingEntry::new(
            "tabs.ctrl_tab_cycle_by_attribute",
            SettingsGroupId::TabManagement,
            "Workspaces",
            "Ctrl+Tab cycles within Essential or Workspace tabs only",
            "Keep Ctrl+Tab cycling scoped to the current group of essentials or workspace tabs.",
            SettingControl::Toggle,
        )
        .keywords(&["ctrl+tab", "essential", "workspace", "cycle"]),
        SettingEntry::new(
            "tabs.select_recently_used_on_close",
            SettingsGroupId::TabManagement,
            "Workspaces",
            "When closing a tab, switch to the most recently used tab instead of the next tab",
            "Return to the tab you were looking at before rather than the neighbor in the list.",
            SettingControl::Toggle,
        )
        .keywords(&["close", "mru", "recently used", "focus"]),
        SettingEntry::new(
            "pinned.restore_url_on_startup",
            SettingsGroupId::TabManagement,
            "Pinned Tabs",
            "Restore pinned tabs to their originally pinned URL on startup",
            "Reset pinned tabs back to their saved home address each time Zendia starts.",
            SettingControl::Toggle,
        )
        .keywords(&["pinned", "startup", "restore", "url"]),
        SettingEntry::new(
            "pinned.separate_essentials",
            SettingsGroupId::TabManagement,
            "Pinned Tabs",
            "Enable container-specific essentials",
            "Keep separate sets of essential icon tiles per container profile.",
            SettingControl::Toggle,
        )
        .keywords(&["essentials", "container", "pinned"]),
        SettingEntry::new(
            "pinned.close_shortcut_behavior",
            SettingsGroupId::TabManagement,
            "Pinned Tabs",
            "Close Tab Shortcut Behavior",
            "Choose what Ctrl+W does when pressed on a pinned tab.",
            SettingControl::Choice(
                PinnedTabCloseBehavior::ALL
                    .iter()
                    .map(|b| opt(b.as_str(), b.label()))
                    .collect(),
            ),
        )
        .keywords(&["pinned", "close", "shortcut", "unload", "reset"]),
        // --- Spaces ---
        SettingEntry::new(
            "spaces.continue_where_left_off",
            SettingsGroupId::Spaces,
            "Spaces",
            "Continue where you left off",
            "Restore your spaces and open tabs from your last browsing session on startup.",
            SettingControl::Toggle,
        )
        .keywords(&["spaces", "workspaces", "session", "restore", "startup"]),
        SettingEntry::new(
            "spaces.create",
            SettingsGroupId::Spaces,
            "Spaces",
            "Create a new Space",
            "Open the space creation form to configure a name, icon, and backdrop theme.",
            SettingControl::Action {
                button_label: "New Space…".to_string(),
            },
        )
        .keywords(&["space", "workspace", "create", "new"])
        .command("space.create_form.open"),
        // --- Glance ---
        SettingEntry::new(
            "glance.enabled",
            SettingsGroupId::Glance,
            "Glance",
            "Enable Glance",
            "Preview links in a floating card over the current page without opening a full tab.",
            SettingControl::Toggle,
        )
        .keywords(&["glance", "preview", "link", "popup"]),
        SettingEntry::new(
            "glance.trigger",
            SettingsGroupId::Glance,
            "Glance",
            "Trigger method",
            "Modifier key held while clicking a link to open it in Glance.",
            SettingControl::Choice(GlanceTrigger::ALL.iter().map(|t| opt(t.as_str(), t.label())).collect()),
        )
        .keywords(&["glance", "trigger", "click", "modifier", "ctrl", "alt", "shift"]),
        // --- Vim Mode ---
        SettingEntry::new(
            "vim.enabled",
            SettingsGroupId::Vim,
            "Vim Mode",
            "Enable Vim mode",
            "Turn on modal keyboard navigation, explorer and toolbar focus, hints, and which-key.",
            SettingControl::Toggle,
        )
        .keywords(&["vim", "neovim", "keyboard", "modal", "keymaps", "shortcuts"])
        .command("vim.set_enabled"),
        SettingEntry::new(
            "vim.statusline_show",
            SettingsGroupId::Vim,
            "Statusline",
            "Mode indicator resting style",
            "Keep the mode indicator full size, shrink it to a corner badge when idle, or hide it.",
            SettingControl::Choice(vec![
                opt("shrink", "Shrink to badge when quiet"),
                opt("always", "Always visible"),
                opt("never", "Hide when quiet"),
            ]),
        )
        .keywords(&["statusline", "mascot", "indicator", "badge", "shrink", "always"])
        .command("statusline.set_show"),
        // --- Agents ---
        SettingEntry::new(
            "agent.default",
            SettingsGroupId::Agents,
            "Default Agent",
            "Default AI agent",
            "Choose which agent opens when starting a new conversation.",
            SettingControl::Choice(vec![opt("claude-code", "Claude"), opt("codex", "Codex")]),
        )
        .keywords(&["agent", "claude", "codex", "ai", "default"])
        .command("agent.select"),
        SettingEntry::new(
            "agent.panel_layout",
            SettingsGroupId::Agents,
            "Agent Side Panel",
            "Agent panel layout",
            "Float the agent panel over the page, slide it from the edge, or split beside the page.",
            SettingControl::Choice(vec![
                opt("float", "Float"),
                opt("slide", "Slide"),
                opt("split", "Split"),
            ]),
        )
        .keywords(&["agent", "dock", "float", "slide", "split", "panel"])
        .command("panel.set_layout"),
        SettingEntry::new(
            "agent.panel_side",
            SettingsGroupId::Agents,
            "Agent Side Panel",
            "Agent panel side",
            "Attach the agent side panel to the left or right edge of the page stage.",
            SettingControl::Choice(vec![opt("right", "Right"), opt("left", "Left")]),
        )
        .keywords(&["agent", "dock", "left", "right", "side"])
        .command("panel.set_side"),
        SettingEntry::new(
            "agents.panel_layout",
            SettingsGroupId::Agents,
            "Agents Roster Panel",
            "Agents roster layout",
            "Choose how the cross-window Agents roster panel docks against the page.",
            SettingControl::Choice(vec![
                opt("float", "Float"),
                opt("slide", "Slide"),
                opt("split", "Split"),
            ]),
        )
        .keywords(&["agents", "roster", "dock", "float", "slide", "split"])
        .command("panel.set_layout"),
        SettingEntry::new(
            "agent.steal_focus",
            SettingsGroupId::Agents,
            "Automation",
            "Allow agents to focus tabs they drive",
            "Let an active agent bring the tab it is inspecting to the front automatically.",
            SettingControl::Toggle,
        )
        .keywords(&["agent", "focus", "automation", "steal"]),
        SettingEntry::new(
            "agent.loop_enabled",
            SettingsGroupId::Agents,
            "Automation",
            "Keep agents on their work list",
            "Automatically prompt an agent to continue until every item on its work list finishes.",
            SettingControl::Toggle,
        )
        .keywords(&["agent", "loop", "work list", "tasks"]),
        // --- Web Chat ---
        SettingEntry::new(
            "webchat.panel_layout",
            SettingsGroupId::WebChat,
            "Web Chat Side Panel",
            "Web chat panel layout",
            "Float the web chat panel over the page, slide it from the edge, or split beside the page.",
            SettingControl::Choice(vec![
                opt("float", "Float"),
                opt("slide", "Slide"),
                opt("split", "Split"),
            ]),
        )
        .keywords(&["webchat", "chat", "dock", "float", "slide", "split"])
        .command("panel.set_layout"),
        SettingEntry::new(
            "webchat.panel_side",
            SettingsGroupId::WebChat,
            "Web Chat Side Panel",
            "Web chat panel side",
            "Attach the web chat side panel to the left or right edge of the page stage.",
            SettingControl::Choice(vec![opt("right", "Right"), opt("left", "Left")]),
        )
        .keywords(&["webchat", "chat", "left", "right", "side"])
        .command("panel.set_side"),
        // --- Plugins ---
        SettingEntry::new(
            "plugins.enabled",
            SettingsGroupId::Plugins,
            "Plugins",
            "Enable installed plugins",
            "Master switch controlling whether sandboxed PluginHost plugins are active.",
            SettingControl::Toggle,
        )
        .keywords(&["plugins", "mods", "pluginhost", "enable", "disable"])
        .command("plugin.set_all_enabled"),
        SettingEntry::new(
            "plugins.auto_update",
            SettingsGroupId::Plugins,
            "Plugins",
            "Automatically check installed plugins for updates on startup",
            "Check plugin packages for newer releases when Zendia starts.",
            SettingControl::Toggle,
        )
        .keywords(&["plugins", "mods", "update", "startup"]),
    ]
}

/// Searches the settings catalog by title, description, section, group name, and keywords.
///
/// Every whitespace-separated query token must match at least one field (case-insensitive). Results are ordered with
/// title matches ahead of description/keyword matches.
pub fn search_settings(query: &str) -> Vec<SettingEntry> {
    let trimmed = query.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return settings_catalog();
    }
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let mut scored: Vec<(u8, usize, SettingEntry)> = Vec::new();

    for (index, item) in settings_catalog().into_iter().enumerate() {
        let title_l = item.title.to_ascii_lowercase();
        let desc_l = item.description.to_ascii_lowercase();
        let section_l = item.section.to_ascii_lowercase();
        let group_l = item.group.label().to_ascii_lowercase();
        let kw_l: Vec<String> = item.keywords.iter().map(|k| k.to_ascii_lowercase()).collect();

        let all_match = tokens.iter().all(|tok| {
            title_l.contains(tok)
                || desc_l.contains(tok)
                || section_l.contains(tok)
                || group_l.contains(tok)
                || kw_l.iter().any(|k| k.contains(tok))
        });
        if !all_match {
            continue;
        }

        let rank = if title_l.starts_with(&trimmed) {
            0
        } else if tokens.iter().all(|tok| title_l.contains(tok)) {
            1
        } else if tokens
            .iter()
            .all(|tok| section_l.contains(tok) || group_l.contains(tok))
        {
            2
        } else if tokens.iter().all(|tok| desc_l.contains(tok)) {
            3
        } else {
            4
        };
        scored.push((rank, index, item));
    }

    scored.sort_by_key(|(rank, index, _)| (*rank, *index));
    scored.into_iter().map(|(_, _, item)| item).collect()
}

/// Reads the current boolean value of a toggle setting from `model`.
pub fn setting_bool_value(model: &BrowserModel, key: &str) -> Option<bool> {
    let s = &model.settings;
    match key {
        "tabs.show_newtab_vertical" => Some(s.show_new_tab_on_tab_list),
        "tabs.newtab_button_top" => Some(s.new_tab_button_top),
        "layout.compact_mode" => Some(model.layout.compact_mode),
        "compact.toolbar_flash_popup" => Some(s.compact_toolbar_flash_popup),
        "compact.show_on_hover" => Some(s.compact_show_on_hover),
        "compact.animate_sidebar" => Some(s.compact_animate_sidebar),
        "compact.show_background_tab_toast" => Some(s.compact_show_background_tab_toast),
        "window.drag_from_content" => Some(s.window_drag_from_content),
        "layout.tab_switch_motion" => Some(model.layout.tab_switch_motion),
        "workspaces.sync_only_pinned_tabs" => Some(s.sync_only_pinned_tabs),
        "workspaces.hide_default_container_indicator" => Some(s.hide_default_container_indicator),
        "workspaces.force_container_workspace" => Some(s.force_container_workspace),
        "tabs.close_on_back_with_no_history" => Some(s.close_on_back_with_no_history),
        "tabs.ctrl_tab_ignore_pending" => Some(s.ctrl_tab_ignore_pending_tabs),
        "tabs.ctrl_tab_cycle_by_attribute" => Some(s.ctrl_tab_cycle_by_attribute),
        "tabs.select_recently_used_on_close" => Some(s.select_recently_used_on_close),
        "pinned.restore_url_on_startup" => Some(s.restore_pinned_tabs_on_startup),
        "pinned.separate_essentials" => Some(s.separate_essentials_per_container),
        "spaces.continue_where_left_off" => Some(s.continue_where_left_off),
        "glance.enabled" => Some(s.glance_enabled),
        "vim.enabled" => Some(model.vim_mode_enabled),
        "agent.steal_focus" => Some(s.agent_steal_focus),
        "agent.loop_enabled" => Some(s.agent_loop_enabled),
        "plugins.enabled" => Some(model.plugins_enabled),
        "plugins.auto_update" => Some(s.plugins_auto_update),
        _ => None,
    }
}

/// Reads the current string value of a choice setting from `model`.
pub fn setting_choice_value(model: &BrowserModel, key: &str) -> Option<String> {
    match key {
        "layout.toolbar_layout" => Some(
            match model.layout.toolbar_layout {
                ToolbarLayout::Single => "single",
                ToolbarLayout::Multiple => "multiple",
                ToolbarLayout::Collapsed => "collapsed",
            }
            .to_string(),
        ),
        "layout.sidebar_side" => Some(
            match model.layout.sidebar_side {
                SidebarSide::Left => "left",
                SidebarSide::Right => "right",
            }
            .to_string(),
        ),
        "layout.compact_hide" => Some(model.layout.compact_hide.as_str().to_string()),
        "theme.scheme" => Some(
            match model.theme.scheme {
                ThemeScheme::System => "system",
                ThemeScheme::Light => "light",
                ThemeScheme::Dark => "dark",
            }
            .to_string(),
        ),
        "pinned.close_shortcut_behavior" => Some(model.settings.pinned_tab_close_behavior.as_str().to_string()),
        "glance.trigger" => Some(model.settings.glance_trigger.as_str().to_string()),
        "vim.statusline_show" => Some(model.statusline.show.as_str().to_string()),
        "agent.default" => model
            .default_agent_id
            .as_ref()
            .map(|id| id.0.clone())
            .or_else(|| model.agents.first().map(|a| a.id.0.clone())),
        "agent.panel_layout" => Some(panel_layout_str(
            model.panel_prefs(&SidePanelId::new(AGENT_PANEL_ID)).layout,
        )),
        "agent.panel_side" => Some(panel_side_str(
            model.panel_prefs(&SidePanelId::new(AGENT_PANEL_ID)).side,
        )),
        "agents.panel_layout" => Some(panel_layout_str(
            model.panel_prefs(&SidePanelId::new(AGENTS_PANEL_ID)).layout,
        )),
        "webchat.panel_layout" => Some(panel_layout_str(
            model.panel_prefs(&SidePanelId::new(WEBCHAT_PANEL_ID)).layout,
        )),
        "webchat.panel_side" => Some(panel_side_str(
            model.panel_prefs(&SidePanelId::new(WEBCHAT_PANEL_ID)).side,
        )),
        _ => None,
    }
}

fn panel_layout_str(layout: SidePanelLayout) -> String {
    match layout {
        SidePanelLayout::Float => "float",
        SidePanelLayout::Slide => "slide",
        SidePanelLayout::Split => "split",
    }
    .to_string()
}

fn parse_panel_layout(raw: &str) -> Option<SidePanelLayout> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "float" => Some(SidePanelLayout::Float),
        "slide" => Some(SidePanelLayout::Slide),
        "split" => Some(SidePanelLayout::Split),
        _ => None,
    }
}

fn panel_side_str(side: SidePanelSide) -> String {
    match side {
        SidePanelSide::Left => "left",
        SidePanelSide::Right => "right",
    }
    .to_string()
}

fn parse_panel_side(raw: &str) -> Option<SidePanelSide> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "left" => Some(SidePanelSide::Left),
        "right" => Some(SidePanelSide::Right),
        _ => None,
    }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Builds the typed [`Command`] that updates setting `key` to `value`, binding to existing browser commands whenever one
/// exists.
pub fn command_for_setting_change(key: &str, value: &str) -> Option<Command> {
    match key {
        "layout.toolbar_layout" => {
            let layout = match value.trim().to_ascii_lowercase().as_str() {
                "single" => ToolbarLayout::Single,
                "multiple" => ToolbarLayout::Multiple,
                "collapsed" => ToolbarLayout::Collapsed,
                _ => return None,
            };
            Some(Command::LayoutSetToolbarLayout { layout })
        }
        "layout.sidebar_side" => {
            let side = match value.trim().to_ascii_lowercase().as_str() {
                "left" => SidebarSide::Left,
                "right" => SidebarSide::Right,
                _ => return None,
            };
            Some(Command::SidebarSetSide { side })
        }
        "layout.compact_mode" => Some(Command::LayoutSetCompactMode {
            enabled: parse_bool(value)?,
        }),
        "layout.compact_hide" => Some(Command::LayoutSetCompactHide {
            hide: CompactHide::parse(value)?,
        }),
        "layout.tab_switch_motion" => Some(Command::LayoutSetTabSwitchMotion {
            enabled: parse_bool(value)?,
        }),
        "theme.scheme" => {
            let scheme = match value.trim().to_ascii_lowercase().as_str() {
                "system" => ThemeScheme::System,
                "light" => ThemeScheme::Light,
                "dark" => ThemeScheme::Dark,
                _ => return None,
            };
            Some(Command::ThemeSet {
                scheme: Some(scheme),
                overrides: None,
            })
        }
        "vim.enabled" => Some(Command::VimSetEnabled {
            enabled: parse_bool(value)?,
        }),
        "vim.statusline_show" => Some(Command::StatuslineSetShow {
            show: StatuslineShow::parse(value)?,
        }),
        "agent.default" => Some(Command::AgentSelect {
            agent_id: Some(AgentId::new(value.trim())),
        }),
        "agent.panel_layout" => Some(Command::PanelSetLayout {
            panel_id: SidePanelId::new(AGENT_PANEL_ID),
            layout: parse_panel_layout(value)?,
        }),
        "agent.panel_side" => Some(Command::PanelSetSide {
            panel_id: SidePanelId::new(AGENT_PANEL_ID),
            side: parse_panel_side(value)?,
        }),
        "agents.panel_layout" => Some(Command::PanelSetLayout {
            panel_id: SidePanelId::new(AGENTS_PANEL_ID),
            layout: parse_panel_layout(value)?,
        }),
        "webchat.panel_layout" => Some(Command::PanelSetLayout {
            panel_id: SidePanelId::new(WEBCHAT_PANEL_ID),
            layout: parse_panel_layout(value)?,
        }),
        "webchat.panel_side" => Some(Command::PanelSetSide {
            panel_id: SidePanelId::new(WEBCHAT_PANEL_ID),
            side: parse_panel_side(value)?,
        }),
        "plugins.enabled" => Some(Command::PluginSetAllEnabled {
            enabled: parse_bool(value)?,
        }),
        "spaces.create" => Some(Command::SpaceCreateFormOpen),
        _ => Some(Command::SettingsSet {
            key: key.to_string(),
            value: value.to_string(),
        }),
    }
}

/// Applies a Phase 13 setting update directly to [`BrowserSettings`].
pub fn apply_browser_setting(settings: &mut BrowserSettings, key: &str, value: &str) -> Result<(), String> {
    let bool_val = |v: &str| parse_bool(v).ok_or_else(|| format!("expected boolean value for `{key}`, got `{v}`"));
    match key {
        "tabs.show_newtab_vertical" => settings.show_new_tab_on_tab_list = bool_val(value)?,
        "tabs.newtab_button_top" => settings.new_tab_button_top = bool_val(value)?,
        "compact.toolbar_flash_popup" => settings.compact_toolbar_flash_popup = bool_val(value)?,
        "compact.show_on_hover" => settings.compact_show_on_hover = bool_val(value)?,
        "compact.animate_sidebar" => settings.compact_animate_sidebar = bool_val(value)?,
        "compact.show_background_tab_toast" => settings.compact_show_background_tab_toast = bool_val(value)?,
        "window.drag_from_content" => settings.window_drag_from_content = bool_val(value)?,
        "workspaces.sync_only_pinned_tabs" => settings.sync_only_pinned_tabs = bool_val(value)?,
        "workspaces.hide_default_container_indicator" => settings.hide_default_container_indicator = bool_val(value)?,
        "workspaces.force_container_workspace" => settings.force_container_workspace = bool_val(value)?,
        "tabs.close_on_back_with_no_history" => settings.close_on_back_with_no_history = bool_val(value)?,
        "tabs.ctrl_tab_ignore_pending" => settings.ctrl_tab_ignore_pending_tabs = bool_val(value)?,
        "tabs.ctrl_tab_cycle_by_attribute" => settings.ctrl_tab_cycle_by_attribute = bool_val(value)?,
        "tabs.select_recently_used_on_close" => settings.select_recently_used_on_close = bool_val(value)?,
        "spaces.continue_where_left_off" => settings.continue_where_left_off = bool_val(value)?,
        "pinned.restore_url_on_startup" => settings.restore_pinned_tabs_on_startup = bool_val(value)?,
        "pinned.separate_essentials" => settings.separate_essentials_per_container = bool_val(value)?,
        "pinned.close_shortcut_behavior" => {
            settings.pinned_tab_close_behavior = PinnedTabCloseBehavior::parse(value)
                .ok_or_else(|| format!("unknown pinned tab close behavior `{value}`"))?;
        }
        "glance.enabled" => settings.glance_enabled = bool_val(value)?,
        "glance.trigger" => {
            settings.glance_trigger =
                GlanceTrigger::parse(value).ok_or_else(|| format!("unknown Glance trigger `{value}`"))?;
        }
        "agent.steal_focus" => settings.agent_steal_focus = bool_val(value)?,
        "agent.loop_enabled" => settings.agent_loop_enabled = bool_val(value)?,
        "plugins.auto_update" => settings.plugins_auto_update = bool_val(value)?,
        _ => return Err(format!("unknown setting key `{key}`")),
    }
    Ok(())
}

/// Resets the settings belonging to `group` inside [`BrowserSettings`] back to defaults.
pub fn reset_browser_settings_group(settings: &mut BrowserSettings, group: SettingsGroupId) {
    let defaults = BrowserSettings::default();
    match group {
        SettingsGroupId::LookAndFeel => {
            settings.show_new_tab_on_tab_list = defaults.show_new_tab_on_tab_list;
            settings.new_tab_button_top = defaults.new_tab_button_top;
            settings.compact_toolbar_flash_popup = defaults.compact_toolbar_flash_popup;
            settings.compact_show_on_hover = defaults.compact_show_on_hover;
            settings.compact_animate_sidebar = defaults.compact_animate_sidebar;
            settings.compact_show_background_tab_toast = defaults.compact_show_background_tab_toast;
            settings.window_drag_from_content = defaults.window_drag_from_content;
        }
        SettingsGroupId::TabManagement => {
            settings.sync_only_pinned_tabs = defaults.sync_only_pinned_tabs;
            settings.hide_default_container_indicator = defaults.hide_default_container_indicator;
            settings.force_container_workspace = defaults.force_container_workspace;
            settings.close_on_back_with_no_history = defaults.close_on_back_with_no_history;
            settings.ctrl_tab_ignore_pending_tabs = defaults.ctrl_tab_ignore_pending_tabs;
            settings.ctrl_tab_cycle_by_attribute = defaults.ctrl_tab_cycle_by_attribute;
            settings.select_recently_used_on_close = defaults.select_recently_used_on_close;
            settings.restore_pinned_tabs_on_startup = defaults.restore_pinned_tabs_on_startup;
            settings.separate_essentials_per_container = defaults.separate_essentials_per_container;
            settings.pinned_tab_close_behavior = defaults.pinned_tab_close_behavior;
        }
        SettingsGroupId::Spaces => {
            settings.continue_where_left_off = defaults.continue_where_left_off;
        }
        SettingsGroupId::Glance => {
            settings.glance_enabled = defaults.glance_enabled;
            settings.glance_trigger = defaults.glance_trigger;
        }
        SettingsGroupId::Agents => {
            settings.agent_steal_focus = defaults.agent_steal_focus;
            settings.agent_loop_enabled = defaults.agent_loop_enabled;
        }
        SettingsGroupId::Plugins => {
            settings.plugins_auto_update = defaults.plugins_auto_update;
        }
        SettingsGroupId::Vim | SettingsGroupId::WebChat | SettingsGroupId::Extensions => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::sample_model;

    #[test]
    fn group_ids_round_trip_and_cover_all_groups() {
        for group in SettingsGroupId::ALL {
            assert_eq!(SettingsGroupId::parse(group.as_str()), Some(*group));
            assert!(!group.label().is_empty());
            assert!(!group.icon().is_empty());
        }
        assert_eq!(SettingsGroupId::parse("mods"), Some(SettingsGroupId::Plugins));
        assert_eq!(SettingsGroupId::parse("unknown"), None);
    }

    #[test]
    fn search_matches_titles_descriptions_and_handles_empty() {
        let all = search_settings("");
        assert_eq!(all.len(), settings_catalog().len());

        let glance = search_settings("glance");
        assert!(glance.iter().any(|e| e.key == "glance.enabled"));
        assert!(glance.iter().any(|e| e.key == "glance.trigger"));

        let pinned = search_settings("ctrl+w pinned");
        assert!(pinned.iter().any(|e| e.key == "pinned.close_shortcut_behavior"));

        let vim = search_settings("mascot");
        assert!(vim.iter().any(|e| e.key == "vim.statusline_show"));

        let none = search_settings("zzzz-no-such-setting");
        assert!(none.is_empty());
    }

    #[test]
    fn command_bindings_reuse_existing_commands() {
        assert_eq!(
            command_for_setting_change("layout.toolbar_layout", "multiple"),
            Some(Command::LayoutSetToolbarLayout {
                layout: ToolbarLayout::Multiple,
            })
        );
        assert_eq!(
            command_for_setting_change("layout.sidebar_side", "right"),
            Some(Command::SidebarSetSide {
                side: SidebarSide::Right,
            })
        );
        assert_eq!(
            command_for_setting_change("vim.enabled", "true"),
            Some(Command::VimSetEnabled { enabled: true })
        );
        assert_eq!(
            command_for_setting_change("theme.scheme", "light"),
            Some(Command::ThemeSet {
                scheme: Some(ThemeScheme::Light),
                overrides: None,
            })
        );
        assert_eq!(
            command_for_setting_change("spaces.create", ""),
            Some(Command::SpaceCreateFormOpen)
        );
    }

    #[test]
    fn apply_browser_setting_validates_and_updates_values() {
        let mut settings = BrowserSettings::default();
        apply_browser_setting(&mut settings, "glance.enabled", "false").unwrap();
        apply_browser_setting(&mut settings, "glance.trigger", "alt").unwrap();
        apply_browser_setting(&mut settings, "pinned.close_shortcut_behavior", "unload-switch").unwrap();
        assert!(!settings.glance_enabled);
        assert_eq!(settings.glance_trigger, GlanceTrigger::Alt);
        assert_eq!(settings.pinned_tab_close_behavior, PinnedTabCloseBehavior::UnloadSwitch);

        assert!(apply_browser_setting(&mut settings, "glance.trigger", "invalid").is_err());
        assert!(apply_browser_setting(&mut settings, "nope", "true").is_err());

        reset_browser_settings_group(&mut settings, SettingsGroupId::Glance);
        assert!(settings.glance_enabled);
        assert_eq!(settings.glance_trigger, GlanceTrigger::Ctrl);
    }

    #[test]
    fn catalog_value_readers_match_model_state() {
        let model = sample_model(true);
        assert_eq!(
            setting_choice_value(&model, "layout.toolbar_layout").as_deref(),
            Some("single")
        );
        assert_eq!(setting_bool_value(&model, "vim.enabled"), Some(false));
        assert_eq!(setting_bool_value(&model, "glance.enabled"), Some(true));
        assert_eq!(setting_choice_value(&model, "glance.trigger").as_deref(), Some("ctrl"));
    }
}
