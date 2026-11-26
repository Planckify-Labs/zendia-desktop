/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Plugins, WebExtensions, their UI contributions, and background service lifecycle helpers.
//!
//! Plugins replace Zen's CSS mods (see `FreyaUIPort.md`, "Open decisions"). Each plugin runs in its own sandboxed
//! `PluginHost` process and renders declared UI panels with the shared kit. Extensions run in `ExtensionServer` and
//! expose toolbar buttons and site-panel entries as standard [`ToolbarItem`] contributions.

use crate::contributions::{
    BadgeTone, ContributionId, ContributionSource, SidePanelEntry, SidePanelId, SidePanelLayout, SidePanelSide,
    ToolbarArea, ToolbarBadge, ToolbarItem, ToolbarItemKind, ToolbarPlacement, ToolbarPlacementRules,
};
use crate::model::{ServiceId, ServiceState, ServiceStatus};
use crate::site_info::SiteExtension;
use zendia_module_api::ui::{UiAction, UiNode, UiRole, UiTone};

/// Stable identifier for an installed Zendia plugin.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PluginId(pub String);

impl PluginId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable identifier for an installed WebExtension.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExtensionId(pub String);

impl ExtensionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Runtime state of a single sandboxed plugin process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginStatus {
    Running,
    Disabled,
    Restarting,
    Stopped { reason: Option<String> },
}

impl PluginStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Disabled => "Disabled",
            Self::Restarting => "Restarting",
            Self::Stopped { .. } => "Stopped",
        }
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self, Self::Stopped { .. })
    }
}

/// One configurable preference exposed by an installed plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginSetting {
    pub key: String,
    pub label: String,
    pub value: String,
}

/// Metadata, capabilities, and declared UI for an installed plugin.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginInfo {
    pub id: PluginId,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub icon: String,
    pub enabled: bool,
    pub status: PluginStatus,
    pub capabilities: Vec<String>,
    pub panel_id: Option<SidePanelId>,
    pub panel_ui: Option<UiNode>,
    pub settings: Vec<PluginSetting>,
}

/// Decoded or raw image payload for a WebExtension icon sent by `ExtensionServer`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtensionIcon {
    Encoded { mime: String, bytes: Vec<u8> },
    Rgba { width: u32, height: u32, bytes: Vec<u8> },
}

/// Pending modal consent prompt requested by `ExtensionServer` (install, permissions, or removal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionPrompt {
    pub token: u64,
    pub title: String,
    pub message: String,
    pub details: Vec<String>,
    pub accept_label: String,
}

/// Manifest-declared keyboard shortcut command registered by an installed WebExtension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionCommand {
    pub extension_id: ExtensionId,
    pub name: String,
    pub shortcut: String,
    pub description: String,
}

/// One page context-menu item (or submenu) registered by an installed WebExtension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionContextMenuItem {
    pub extension_id: ExtensionId,
    pub item_id: String,
    pub title: String,
    pub enabled: bool,
    pub is_separator: bool,
    pub children: Vec<ExtensionContextMenuItem>,
}

/// Metadata and toolbar contribution settings for an installed WebExtension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionInfo {
    pub id: ExtensionId,
    pub name: String,
    pub version: String,
    pub description: String,
    pub icon: String,
    pub icon_data: Option<ExtensionIcon>,
    pub enabled: bool,
    pub pinned_to_toolbar: bool,
    pub has_site_access: bool,
    pub site_access: Option<String>,
    pub permissions: Vec<String>,
    pub badge_text: Option<String>,
    pub badge_bg: Option<[u8; 4]>,
    pub badge_fg: Option<[u8; 4]>,
    pub options_url: Option<String>,
    pub popup_url: Option<String>,
    pub toolbar_item_id: ContributionId,
    pub command: String,
}

/// Services surfaced in Zendia's Settings -> Extensions & Services view and service-down indicators.
pub const MANAGED_SERVICES: &[ServiceId] = &[
    ServiceId::AgentServer,
    ServiceId::InputServer,
    ServiceId::PluginHost,
    ServiceId::ExtensionServer,
];

/// Canonical wire/command name for a [`ServiceId`].
pub fn service_id_as_str(service: ServiceId) -> &'static str {
    match service {
        ServiceId::Hub => "hub",
        ServiceId::InputServer => "input_server",
        ServiceId::AgentServer => "agent_server",
        ServiceId::PluginHost => "plugin_host",
        ServiceId::ExtensionServer => "extension_server",
    }
}

/// Parses a service name (accepting kebab-case, snake_case, and short aliases).
pub fn parse_service_id(raw: &str) -> Option<ServiceId> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "hub" => Some(ServiceId::Hub),
        "input" | "input_server" | "input-server" | "inputserver" => Some(ServiceId::InputServer),
        "agent" | "agent_server" | "agent-server" | "agentserver" => Some(ServiceId::AgentServer),
        "plugin" | "plugins" | "plugin_host" | "plugin-host" | "pluginhost" => Some(ServiceId::PluginHost),
        "extension" | "extensions" | "extension_server" | "extension-server" | "extensionserver" => {
            Some(ServiceId::ExtensionServer)
        }
        _ => None,
    }
}

/// Human-readable display name of a background service.
pub fn service_label(service: ServiceId) -> &'static str {
    match service {
        ServiceId::Hub => "Hub",
        ServiceId::InputServer => "InputServer",
        ServiceId::AgentServer => "AgentServer",
        ServiceId::PluginHost => "PluginHost",
        ServiceId::ExtensionServer => "ExtensionServer",
    }
}

/// Short description of what a background service owns.
pub fn service_description(service: ServiceId) -> &'static str {
    match service {
        ServiceId::Hub => "Engine broker hosting WebContent tabs and storage.",
        ServiceId::InputServer => "Compiles keyboard profiles and manages vim mode, counts, and macros.",
        ServiceId::AgentServer => "Runs sandboxed AI agent sessions, tool traces, and model API calls.",
        ServiceId::PluginHost => "Runs each installed plugin in an isolated sandbox with declared UI.",
        ServiceId::ExtensionServer => "Hosts Chrome and Firefox WebExtensions through cgx-webext.",
    }
}

/// Summary shown when a service is not running, matching `Architecture.md`'s crash isolation table.
pub fn service_down_summary(service: ServiceId) -> &'static str {
    match service {
        ServiceId::Hub => "Engine connection lost.",
        ServiceId::InputServer => "Falling back to standard shortcuts until InputServer restarts.",
        ServiceId::AgentServer => "Active agent runs stopped with their traces kept. Tabs stay open.",
        ServiceId::PluginHost => "Plugin panels are stopped. Browser chrome and tabs keep running.",
        ServiceId::ExtensionServer => "Extension buttons are paused and will reload when ExtensionServer restarts.",
    }
}

/// Default crash reason used when cycling a service into [`ServiceStatus::Stopped`].
pub fn default_stopped_reason(service: ServiceId) -> String {
    match service {
        ServiceId::Hub => "Hub connection closed".to_string(),
        ServiceId::InputServer => "InputServer stopped (using standard fallback)".to_string(),
        ServiceId::AgentServer => "AgentServer exited unexpectedly (code 1)".to_string(),
        ServiceId::PluginHost => "PluginHost sandbox exited unexpectedly".to_string(),
        ServiceId::ExtensionServer => "ExtensionServer paused (process exited)".to_string(),
    }
}

/// Cycles a service's status through `Running -> Restarting -> Stopped -> Running` for inspection and testing.
pub fn cycle_service_status(service: ServiceId, current: &ServiceStatus) -> ServiceStatus {
    match current {
        ServiceStatus::Running => ServiceStatus::Restarting,
        ServiceStatus::Starting | ServiceStatus::Restarting => ServiceStatus::Stopped {
            reason: Some(default_stopped_reason(service)),
        },
        ServiceStatus::Stopped { .. } => ServiceStatus::Running,
    }
}

/// Parses a status token (`running`, `starting`, `restarting`, `stopped`) into a [`ServiceStatus`].
pub fn parse_service_status(raw: &str, reason: Option<String>) -> Option<ServiceStatus> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "running" | "up" | "ok" => Some(ServiceStatus::Running),
        "starting" => Some(ServiceStatus::Starting),
        "restarting" => Some(ServiceStatus::Restarting),
        "stopped" | "down" | "crashed" | "paused" => Some(ServiceStatus::Stopped { reason }),
        _ => None,
    }
}

/// Human-readable label for a [`ServiceStatus`].
pub fn service_status_label(status: &ServiceStatus) -> &'static str {
    match status {
        ServiceStatus::Running => "Running",
        ServiceStatus::Starting => "Starting",
        ServiceStatus::Restarting => "Restarting",
        ServiceStatus::Stopped { .. } => "Stopped",
    }
}

/// Built-in panel id for the stub backend's sample plugin (`Page Notes`).
pub const SAMPLE_PLUGIN_ID: &str = "plugin.page_notes";
pub const SAMPLE_PLUGIN_PANEL_ID: &str = "plugin.page_notes.panel";

/// Built-in identifier for the stub backend's sample WebExtension (`uBlock Origin`).
pub const SAMPLE_EXTENSION_ID: &str = "ublock-origin@zendia";
pub const SAMPLE_EXTENSION_TOOLBAR_ID: &str = "ext.ublock_origin";

/// Declared UI tree rendered inside the sample `Page Notes` plugin panel using the shared kit.
pub fn sample_plugin_declared_ui() -> UiNode {
    UiNode::Column {
        spacing: 10.0,
        padding: 14.0,
        children: vec![
            UiNode::Row {
                spacing: 8.0,
                padding: 0.0,
                children: vec![
                    UiNode::Pill {
                        label: "Sandboxed · PluginHost".to_string(),
                        icon: Some("sparkles".to_string()),
                        tone: UiTone::Accent,
                        action: None,
                    },
                    UiNode::Pill {
                        label: "Declared UI".to_string(),
                        icon: None,
                        tone: UiTone::Success,
                        action: None,
                    },
                ],
            },
            UiNode::Text {
                text: "Quick per-origin notes rendered natively with Zendia's component kit.".to_string(),
                role: UiRole::TextMuted,
                size: Some(12.5),
                bold: false,
            },
            UiNode::ListRow {
                label: "Review Freya multi-window API".to_string(),
                detail: Some("zendia.local/docs · 2m ago".to_string()),
                icon: Some("book".to_string()),
                key_hint: Some("1".to_string()),
                cursor: false,
                selected: true,
                disabled: false,
                action: Some(UiAction::new("toast.show").with_arg("message", "Opened note #1")),
            },
            UiNode::ListRow {
                label: "Verify service-down crash isolation".to_string(),
                detail: Some("zendia.local/docs · 14m ago".to_string()),
                icon: Some("check".to_string()),
                key_hint: Some("2".to_string()),
                cursor: false,
                selected: false,
                disabled: false,
                action: Some(UiAction::new("toast.show").with_arg("message", "Opened note #2")),
            },
            UiNode::Chip {
                label: "Copy notes summary".to_string(),
                icon: Some("edit-copy".to_string()),
                selected: false,
                disabled: false,
                tone: UiTone::Accent,
                action: Some(UiAction::new("toast.show").with_arg("message", "Copied Page Notes summary")),
            },
        ],
    }
}

/// Returns the stub backend's sample plugin (`FreyaUIPort.md`, "Who owns what").
pub fn sample_plugins() -> Vec<PluginInfo> {
    vec![PluginInfo {
        id: PluginId::new(SAMPLE_PLUGIN_ID),
        name: "Page Notes".to_string(),
        version: "1.2.0".to_string(),
        author: "Zendia Examples".to_string(),
        description: "Keep per-origin notes in a native declared UI side panel without injecting page scripts."
            .to_string(),
        icon: "book".to_string(),
        enabled: true,
        status: PluginStatus::Running,
        capabilities: vec![
            "tabs.read".to_string(),
            "ui.panel".to_string(),
            "storage.local".to_string(),
        ],
        panel_id: Some(SidePanelId::new(SAMPLE_PLUGIN_PANEL_ID)),
        panel_ui: Some(sample_plugin_declared_ui()),
        settings: vec![
            PluginSetting {
                key: "sync_by_origin".to_string(),
                label: "Group notes by origin".to_string(),
                value: "true".to_string(),
            },
            PluginSetting {
                key: "default_side".to_string(),
                label: "Preferred dock side".to_string(),
                value: "right".to_string(),
            },
        ],
    }]
}

/// Returns a stopped sample plugin used by gallery fixtures to show a crashed plugin alongside running ones.
pub fn sample_stopped_plugin() -> PluginInfo {
    PluginInfo {
        id: PluginId::new("plugin.feed_reader"),
        name: "Feed Reader".to_string(),
        version: "0.9.4".to_string(),
        author: "Community".to_string(),
        description: "Subscribe to RSS and Atom feeds and surface unread badges in a side panel.".to_string(),
        icon: "rss".to_string(),
        enabled: true,
        status: PluginStatus::Stopped {
            reason: Some("PluginHost sandbox terminated after memory limit exceeded".to_string()),
        },
        capabilities: vec!["network.fetch".to_string(), "ui.panel".to_string()],
        panel_id: Some(SidePanelId::new("plugin.feed_reader.panel")),
        panel_ui: None,
        settings: vec![PluginSetting {
            key: "poll_minutes".to_string(),
            label: "Refresh interval".to_string(),
            value: "15 min".to_string(),
        }],
    }
}

/// Returns the stub backend's sample WebExtension (`FreyaUIPort.md`, "Who owns what").
pub fn sample_extensions() -> Vec<ExtensionInfo> {
    vec![ExtensionInfo {
        id: ExtensionId::new(SAMPLE_EXTENSION_ID),
        name: "uBlock Origin".to_string(),
        version: "1.61.2".to_string(),
        description: "Efficient wide-spectrum content blocker running through ExtensionServer (cgx-webext)."
            .to_string(),
        icon: "tracking-protection-fill".to_string(),
        icon_data: None,
        enabled: true,
        pinned_to_toolbar: false,
        has_site_access: true,
        site_access: Some("granted".to_string()),
        permissions: vec!["webRequest".to_string(), "storage".to_string(), "tabs".to_string()],
        badge_text: Some("4".to_string()),
        badge_bg: None,
        badge_fg: None,
        options_url: None,
        popup_url: None,
        toolbar_item_id: ContributionId::new(SAMPLE_EXTENSION_TOOLBAR_ID),
        command: "extension.invoke_action".to_string(),
    }]
}

/// Builds a contributed [`ToolbarItem`] for `ext`, reflecting `ExtensionServer`'s health state.
pub fn extension_to_toolbar_item(ext: &ExtensionInfo, service_status: &ServiceStatus) -> ToolbarItem {
    let paused = !matches!(service_status, ServiceStatus::Running);
    let badge = if paused {
        Some(ToolbarBadge {
            text: "II".to_string(),
            tone: BadgeTone::Warning,
        })
    } else {
        ext.badge_text.as_ref().map(|text| ToolbarBadge {
            text: text.clone(),
            tone: BadgeTone::Accent,
        })
    };
    let tooltip = if paused {
        format!("{} (paused — ExtensionServer unavailable)", ext.name)
    } else {
        ext.name.clone()
    };

    ToolbarItem {
        id: ext.toolbar_item_id.clone(),
        title: ext.name.clone(),
        tooltip: Some(tooltip),
        icon: ext.icon.clone(),
        command: ext.command.clone(),
        command_args: vec![("extension_id".to_string(), ext.id.0.clone())],
        kind: ToolbarItemKind::Button,
        placement: ToolbarPlacement::NavigationEnd,
        placement_rules: ToolbarPlacementRules {
            single_toolbar: Some(ToolbarArea::SidebarUrlBar),
            single_toolbar_compact: Some(ToolbarArea::SidebarUrlBar),
            multiple_toolbars: Some(ToolbarArea::TopBarEnd),
            collapsed_toolbar: Some(ToolbarArea::TopBarEnd),
        },
        order: -40,
        badge,
        visible: ext.enabled && ext.pinned_to_toolbar,
        enabled: ext.enabled && !paused,
        source: ContributionSource::Extension {
            extension_id: ext.id.0.clone(),
        },
    }
}

/// Converts an [`ExtensionInfo`] into the [`SiteExtension`] tile shown by the Site Data Panel.
pub fn extension_to_site_extension(ext: &ExtensionInfo) -> SiteExtension {
    SiteExtension {
        id: ext.toolbar_item_id.clone(),
        name: ext.name.clone(),
        icon: ext.icon.clone(),
        enabled: ext.enabled,
        has_site_access: ext.has_site_access,
        badge_text: ext.badge_text.clone(),
        command: ext.command.clone(),
        source: ContributionSource::Extension {
            extension_id: ext.id.0.clone(),
        },
    }
}

/// Returns the extensions that belong in the Site Data Panel's Extensions tile area.
///
/// Matches the fork's `CustomizableUI` single-location rule: unpinned extensions live in the Site Data Panel, while
/// pinned enabled extensions move out of the panel and into the active toolbar (`AREA_NAVBAR`).
pub fn unpinned_site_extensions(extensions: &[ExtensionInfo]) -> Vec<SiteExtension> {
    extensions
        .iter()
        .filter(|ext| !ext.pinned_to_toolbar)
        .map(extension_to_site_extension)
        .collect()
}

/// Builds the Ladybird/Zen right-click context menu for an extension button (in the Site Data Panel or pinned toolbar).
pub fn build_extension_context_menu(ext: &ExtensionInfo) -> Vec<crate::menus::ResolvedMenuItem> {
    use crate::menus::ResolvedMenuItem;
    let ext_id = ext.id.0.clone();
    let pin_label = if ext.pinned_to_toolbar {
        "Unpin from Toolbar"
    } else {
        "Pin to Toolbar"
    };
    let pin_icon = if ext.pinned_to_toolbar { "unpin" } else { "pin" };
    let next_pinned = if ext.pinned_to_toolbar { "false" } else { "true" };

    let mut pin = ResolvedMenuItem::action(
        format!("ext.ctx.pin.{ext_id}"),
        pin_label,
        "extension.set_toolbar_pinned",
    );
    pin.icon = Some(pin_icon.to_string());
    pin.command_args = vec![
        ("extension_id".to_string(), ext_id.clone()),
        ("pinned".to_string(), next_pinned.to_string()),
    ];

    let mut manage = ResolvedMenuItem::action(format!("ext.ctx.manage.{ext_id}"), "Manage Extension", "settings.open");
    manage.icon = Some("settings".to_string());
    manage.command_args = vec![("group".to_string(), "extensions".to_string())];

    let mut remove = ResolvedMenuItem::action(
        format!("ext.ctx.remove.{ext_id}"),
        "Remove Extension",
        "extension.remove",
    );
    remove.icon = Some("trash".to_string());
    remove.command_args = vec![("extension_id".to_string(), ext_id.clone())];

    let mut items = vec![pin];
    if ext.options_url.is_some() {
        let mut options = ResolvedMenuItem::action(
            format!("ext.ctx.options.{ext_id}"),
            "Extension Options",
            "extension.open_options",
        );
        options.icon = Some("sliders".to_string());
        options.command_args = vec![("extension_id".to_string(), ext_id.clone())];
        items.push(options);
    }
    items.push(manage);
    items.push(ResolvedMenuItem::separator(format!("ext.ctx.sep.{ext_id}")));
    items.push(remove);
    items
}

/// Builds a [`SidePanelEntry`] contribution for a plugin that declares a side panel.
pub fn plugin_to_side_panel(plugin: &PluginInfo) -> Option<SidePanelEntry> {
    let panel_id = plugin.panel_id.clone()?;
    Some(SidePanelEntry {
        id: panel_id,
        title: plugin.name.clone(),
        icon: plugin.icon.clone(),
        default_layout: SidePanelLayout::Float,
        default_side: SidePanelSide::Right,
        default_width: 360.0,
        source: ContributionSource::Plugin {
            plugin_id: plugin.id.0.clone(),
        },
    })
}

/// Returns the status of `service` from a slice of [`ServiceState`], defaulting to `Running` if absent.
pub fn service_status_in(services: &[ServiceState], service: ServiceId) -> ServiceStatus {
    services
        .iter()
        .find(|entry| entry.service == service)
        .map(|entry| entry.status.clone())
        .unwrap_or(ServiceStatus::Running)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_id_parsing_and_status_cycling_work() {
        for svc in MANAGED_SERVICES {
            assert_eq!(parse_service_id(service_id_as_str(*svc)), Some(*svc));
            assert!(!service_label(*svc).is_empty());
            assert!(!service_description(*svc).is_empty());
            assert!(!service_down_summary(*svc).is_empty());
        }

        let s1 = cycle_service_status(ServiceId::ExtensionServer, &ServiceStatus::Running);
        assert_eq!(s1, ServiceStatus::Restarting);
        let s2 = cycle_service_status(ServiceId::ExtensionServer, &s1);
        assert!(matches!(s2, ServiceStatus::Stopped { reason: Some(_) }));
        let s3 = cycle_service_status(ServiceId::ExtensionServer, &s2);
        assert_eq!(s3, ServiceStatus::Running);
    }

    #[test]
    fn sample_extension_starts_in_the_site_panel_only_and_moves_on_pin() {
        let exts = sample_extensions();
        let ext = &exts[0];
        assert!(!ext.pinned_to_toolbar);
        assert!(!extension_to_toolbar_item(ext, &ServiceStatus::Running).visible);
        let site_exts = unpinned_site_extensions(&exts);
        assert_eq!(site_exts.len(), 1);
        assert_eq!(site_exts[0].badge_text.as_deref(), Some("4"));
        assert_eq!(site_exts[0].extension_id(), Some(ext.id.clone()));

        let pinned_ext = ExtensionInfo {
            pinned_to_toolbar: true,
            ..ext.clone()
        };
        assert!(unpinned_site_extensions(std::slice::from_ref(&pinned_ext)).is_empty());
        let item = extension_to_toolbar_item(&pinned_ext, &ServiceStatus::Running);
        assert!(item.visible);
        assert_eq!(item.placement_rules.single_toolbar, Some(ToolbarArea::SidebarUrlBar));
        assert_eq!(
            item.placement_rules.single_toolbar_compact,
            Some(ToolbarArea::SidebarUrlBar)
        );
        assert_eq!(item.placement_rules.multiple_toolbars, Some(ToolbarArea::TopBarEnd));
        assert_eq!(item.placement_rules.collapsed_toolbar, Some(ToolbarArea::TopBarEnd));

        let unpinned_menu = build_extension_context_menu(ext);
        assert_eq!(unpinned_menu[0].label, "Pin to Toolbar");
        let pinned_menu = build_extension_context_menu(&pinned_ext);
        assert_eq!(pinned_menu[0].label, "Unpin from Toolbar");
    }

    #[test]
    fn extension_toolbar_item_reflects_running_and_paused_states() {
        let ext = &ExtensionInfo {
            pinned_to_toolbar: true,
            ..sample_extensions()[0].clone()
        };
        let running = extension_to_toolbar_item(ext, &ServiceStatus::Running);
        assert!(running.visible);
        assert!(running.enabled);
        assert_eq!(running.badge.as_ref().map(|b| b.text.as_str()), Some("4"));
        assert_eq!(
            running.source,
            ContributionSource::Extension {
                extension_id: SAMPLE_EXTENSION_ID.to_string(),
            }
        );

        let paused = extension_to_toolbar_item(
            ext,
            &ServiceStatus::Stopped {
                reason: Some("crashed".to_string()),
            },
        );
        assert!(paused.visible);
        assert!(!paused.enabled);
        assert_eq!(paused.badge.as_ref().map(|b| b.tone), Some(BadgeTone::Warning));
        assert!(paused.tooltip.as_deref().unwrap_or("").contains("paused"));
    }

    #[test]
    fn sample_plugin_exposes_declared_side_panel() {
        let plugins = sample_plugins();
        assert_eq!(plugins.len(), 1);
        let panel = plugin_to_side_panel(&plugins[0]).expect("sample plugin should declare a panel");
        assert_eq!(panel.id, SidePanelId::new(SAMPLE_PLUGIN_PANEL_ID));
        assert!(plugins[0].panel_ui.is_some());
    }
}
