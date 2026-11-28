/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub backend command handlers, parsers, and descriptors for Phase 13: native settings, plugins, extensions, and
//! background service states.

use super::StubBackend;
use crate::commands::{
    ArgumentSchema, ArgumentType, Command, CommandDescriptor, CommandError, CommandResult, UblockAction,
};
use crate::contributions::ContributionSource;
use crate::events::{Event, ViewRequest};
use crate::model::{BrowserModel, ServiceId, ServiceState, ServiceStatus};
use crate::modules::{
    ExtensionId, PluginId, PluginStatus, SAMPLE_EXTENSION_ID, cycle_service_status, extension_to_toolbar_item,
    parse_service_id, parse_service_status, plugin_to_side_panel, service_down_summary, service_label,
    service_status_in, service_status_label, unpinned_site_extensions,
};
use crate::settings::{SettingsGroupId, apply_browser_setting, reset_browser_settings_group};

fn is_ublock_extension(id: &str, name: &str) -> bool {
    matches!(
        id,
        "uBlock0@raymondhill.net" | "cjpalhdlnbpafiamejdnhcphjbkeiagm" | SAMPLE_EXTENSION_ID
    ) || name.eq_ignore_ascii_case("uBlock Origin")
}

fn find_arg<'a>(args: &'a [(String, String)], key: &str) -> Option<&'a str> {
    args.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn parse_bool_arg(command: &str, args: &[(String, String)], key: &str, default: bool) -> Result<bool, CommandError> {
    match find_arg(args, key) {
        None => Ok(default),
        Some("true") | Some("1") | Some("yes") => Ok(true),
        Some("false") | Some("0") | Some("no") => Ok(false),
        Some(other) => Err(CommandError::InvalidArgument {
            command: command.to_string(),
            message: format!("argument `{key}` expected boolean, got `{other}`"),
        }),
    }
}

fn parse_required_string(command: &str, args: &[(String, String)], key: &str) -> Result<String, CommandError> {
    find_arg(args, key)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| CommandError::InvalidArgument {
            command: command.to_string(),
            message: format!("missing required argument `{key}`"),
        })
}

/// Parses Phase 13 dotted command names (`settings.*`, `plugin.*`, `extension.*`, `service.*`, and `extensions.open`).
pub(crate) fn parse_command(name: &str, args: &[(String, String)]) -> Result<Option<Command>, CommandError> {
    match name {
        "settings.open" => {
            let group = match find_arg(args, "group") {
                None | Some("") => None,
                Some(raw) => Some(
                    SettingsGroupId::parse(raw).ok_or_else(|| CommandError::InvalidArgument {
                        command: name.to_string(),
                        message: format!("unknown settings group `{raw}`"),
                    })?,
                ),
            };
            Ok(Some(Command::SettingsOpen { group }))
        }
        "settings.close" => Ok(Some(Command::SettingsClose)),
        "extensions.open" => Ok(Some(Command::SettingsOpen {
            group: Some(SettingsGroupId::Extensions),
        })),
        "settings.set" => Ok(Some(Command::SettingsSet {
            key: parse_required_string(name, args, "key")?,
            value: find_arg(args, "value").unwrap_or("").to_string(),
        })),
        "settings.reset_group" => {
            let raw = parse_required_string(name, args, "group")?;
            let group = SettingsGroupId::parse(&raw).ok_or_else(|| CommandError::InvalidArgument {
                command: name.to_string(),
                message: format!("unknown settings group `{raw}`"),
            })?;
            Ok(Some(Command::SettingsResetGroup { group }))
        }
        "plugin.toggle" => Ok(Some(Command::PluginToggle {
            plugin_id: PluginId::new(parse_required_string(name, args, "plugin_id")?),
        })),
        "plugin.set_enabled" => Ok(Some(Command::PluginSetEnabled {
            plugin_id: PluginId::new(parse_required_string(name, args, "plugin_id")?),
            enabled: parse_bool_arg(name, args, "enabled", true)?,
        })),
        "plugin.set_all_enabled" => Ok(Some(Command::PluginSetAllEnabled {
            enabled: parse_bool_arg(name, args, "enabled", true)?,
        })),
        "plugin.remove" => Ok(Some(Command::PluginRemove {
            plugin_id: PluginId::new(parse_required_string(name, args, "plugin_id")?),
        })),
        "plugin.open_panel" => Ok(Some(Command::PluginOpenPanel {
            plugin_id: PluginId::new(parse_required_string(name, args, "plugin_id")?),
        })),
        "plugin.restart" => Ok(Some(Command::PluginRestart {
            plugin_id: PluginId::new(parse_required_string(name, args, "plugin_id")?),
        })),
        "extension.toggle" => Ok(Some(Command::ExtensionToggle {
            extension_id: ExtensionId::new(parse_required_string(name, args, "extension_id")?),
        })),
        "extension.set_enabled" => Ok(Some(Command::ExtensionSetEnabled {
            extension_id: ExtensionId::new(parse_required_string(name, args, "extension_id")?),
            enabled: parse_bool_arg(name, args, "enabled", true)?,
        })),
        "extension.set_toolbar_pinned" => Ok(Some(Command::ExtensionSetToolbarPinned {
            extension_id: ExtensionId::new(parse_required_string(name, args, "extension_id")?),
            pinned: parse_bool_arg(name, args, "pinned", true)?,
        })),
        "extension.invoke_action" => {
            let extension_id = ExtensionId::new(parse_required_string(name, args, "extension_id")?);
            if parse_bool_arg(name, args, "from_keyboard", false)? {
                Ok(Some(Command::ExtensionInvokeActionFromKeyboard { extension_id }))
            } else {
                Ok(Some(Command::ExtensionInvokeAction { extension_id }))
            }
        }
        "extension.remove" => Ok(Some(Command::ExtensionRemove {
            extension_id: ExtensionId::new(parse_required_string(name, args, "extension_id")?),
        })),
        "extension.reload" => Ok(Some(Command::ExtensionReload {
            extension_id: ExtensionId::new(parse_required_string(name, args, "extension_id")?),
        })),
        "extension.install_path" => Ok(Some(Command::ExtensionInstallPath {
            path: parse_required_string(name, args, "path")?,
        })),
        "extension.open_options" => Ok(Some(Command::ExtensionOpenOptions {
            extension_id: ExtensionId::new(parse_required_string(name, args, "extension_id")?),
        })),
        "extension.prompt_answer" => {
            let raw_token = parse_required_string(name, args, "token")?;
            let token = raw_token.parse::<u64>().map_err(|_| CommandError::InvalidArgument {
                command: name.to_string(),
                message: format!("argument `token` expected u64, got `{raw_token}`"),
            })?;
            Ok(Some(Command::ExtensionPromptAnswer {
                token,
                accepted: parse_bool_arg(name, args, "accepted", false)?,
            }))
        }
        "extension.context_menu_click" => Ok(Some(Command::ExtensionContextMenuClick {
            extension_id: ExtensionId::new(parse_required_string(name, args, "extension_id")?),
            item_id: parse_required_string(name, args, "item_id")?,
            target_json: find_arg(args, "target_json").unwrap_or("{}").to_string(),
        })),
        "extension.invoke_command" => Ok(Some(Command::ExtensionInvokeCommand {
            extension_id: ExtensionId::new(parse_required_string(name, args, "extension_id")?),
            name: parse_required_string(name, args, "name")?,
        })),
        "extensions.menu.toggle" => Ok(Some(Command::ExtensionsMenuToggle)),
        "service.set_state" => {
            let raw_service = parse_required_string(name, args, "service")?;
            let service = parse_service_id(&raw_service).ok_or_else(|| CommandError::InvalidArgument {
                command: name.to_string(),
                message: format!("unknown service `{raw_service}`"),
            })?;
            let raw_status = parse_required_string(name, args, "status")?;
            let reason = find_arg(args, "reason").map(str::to_string);
            let status = parse_service_status(&raw_status, reason).ok_or_else(|| CommandError::InvalidArgument {
                command: name.to_string(),
                message: format!("unknown service status `{raw_status}`"),
            })?;
            Ok(Some(Command::ServiceSetState { service, status }))
        }
        "service.cycle_state" => {
            let raw_service = parse_required_string(name, args, "service")?;
            let service = parse_service_id(&raw_service).ok_or_else(|| CommandError::InvalidArgument {
                command: name.to_string(),
                message: format!("unknown service `{raw_service}`"),
            })?;
            Ok(Some(Command::ServiceCycleState { service }))
        }
        "service.restart" => {
            let raw_service = parse_required_string(name, args, "service")?;
            let service = parse_service_id(&raw_service).ok_or_else(|| CommandError::InvalidArgument {
                command: name.to_string(),
                message: format!("unknown service `{raw_service}`"),
            })?;
            Ok(Some(Command::ServiceRestart { service }))
        }
        other if other.starts_with("extension.invoke_action:") => {
            let ext_id = other.trim_start_matches("extension.invoke_action:");
            Ok(Some(Command::ExtensionInvokeAction {
                extension_id: ExtensionId::new(ext_id),
            }))
        }
        other => Ok(UblockAction::parse(other).map(|action| Command::UblockInvoke { action })),
    }
}

/// Command descriptors registered in `BrowserCore::commands()` for Phase 13.
pub(crate) fn command_descriptors() -> Vec<CommandDescriptor> {
    vec![
        CommandDescriptor {
            name: "settings.open".to_string(),
            title: "Open Settings".to_string(),
            description: "Open Zendia's native settings window, optionally focused on a specific group".to_string(),
            arguments: vec![ArgumentSchema::new(
                "group",
                ArgumentType::String,
                false,
                "Settings group id (look-and-feel, tab-management, spaces, glance, vim, agents, webchat, plugins, extensions)",
            )],
        },
        CommandDescriptor {
            name: "settings.close".to_string(),
            title: "Close Settings".to_string(),
            description: "Close the native settings window".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "settings.set".to_string(),
            title: "Set Browser Setting".to_string(),
            description: "Update a profile setting by key and string value".to_string(),
            arguments: vec![
                ArgumentSchema::new("key", ArgumentType::String, true, "Setting key"),
                ArgumentSchema::new("value", ArgumentType::String, true, "Setting value"),
            ],
        },
        CommandDescriptor {
            name: "settings.reset_group".to_string(),
            title: "Reset Settings Group".to_string(),
            description: "Reset profile settings in a group back to their defaults".to_string(),
            arguments: vec![ArgumentSchema::new(
                "group",
                ArgumentType::String,
                true,
                "Settings group id",
            )],
        },
        CommandDescriptor {
            name: "plugin.toggle".to_string(),
            title: "Toggle Plugin".to_string(),
            description: "Enable or disable an installed plugin".to_string(),
            arguments: vec![ArgumentSchema::new(
                "plugin_id",
                ArgumentType::String,
                true,
                "Plugin id",
            )],
        },
        CommandDescriptor {
            name: "plugin.set_enabled".to_string(),
            title: "Set Plugin Enabled".to_string(),
            description: "Explicitly enable or disable an installed plugin".to_string(),
            arguments: vec![
                ArgumentSchema::new("plugin_id", ArgumentType::String, true, "Plugin id"),
                ArgumentSchema::new("enabled", ArgumentType::Bool, true, "Whether the plugin is enabled"),
            ],
        },
        CommandDescriptor {
            name: "plugin.set_all_enabled".to_string(),
            title: "Enable or Disable All Plugins".to_string(),
            description: "Toggle all installed plugins on or off".to_string(),
            arguments: vec![ArgumentSchema::new(
                "enabled",
                ArgumentType::Bool,
                true,
                "Master plugins switch",
            )],
        },
        CommandDescriptor {
            name: "plugin.remove".to_string(),
            title: "Remove Plugin".to_string(),
            description: "Uninstall a plugin and remove its contributed panels".to_string(),
            arguments: vec![ArgumentSchema::new(
                "plugin_id",
                ArgumentType::String,
                true,
                "Plugin id",
            )],
        },
        CommandDescriptor {
            name: "plugin.open_panel".to_string(),
            title: "Open Plugin Panel".to_string(),
            description: "Open a plugin's declared UI side panel in the dock frame".to_string(),
            arguments: vec![ArgumentSchema::new(
                "plugin_id",
                ArgumentType::String,
                true,
                "Plugin id",
            )],
        },
        CommandDescriptor {
            name: "plugin.restart".to_string(),
            title: "Restart Plugin".to_string(),
            description: "Restart a stopped plugin sandbox".to_string(),
            arguments: vec![ArgumentSchema::new(
                "plugin_id",
                ArgumentType::String,
                true,
                "Plugin id",
            )],
        },
        CommandDescriptor {
            name: "extension.toggle".to_string(),
            title: "Toggle Extension".to_string(),
            description: "Enable or disable an installed WebExtension".to_string(),
            arguments: vec![ArgumentSchema::new(
                "extension_id",
                ArgumentType::String,
                true,
                "Extension id",
            )],
        },
        CommandDescriptor {
            name: "extension.set_enabled".to_string(),
            title: "Set Extension Enabled".to_string(),
            description: "Explicitly enable or disable an installed WebExtension".to_string(),
            arguments: vec![
                ArgumentSchema::new("extension_id", ArgumentType::String, true, "Extension id"),
                ArgumentSchema::new("enabled", ArgumentType::Bool, true, "Whether the extension is enabled"),
            ],
        },
        CommandDescriptor {
            name: "extension.set_toolbar_pinned".to_string(),
            title: "Pin or Unpin Extension Button".to_string(),
            description: "Show or hide an extension's contributed toolbar button".to_string(),
            arguments: vec![
                ArgumentSchema::new("extension_id", ArgumentType::String, true, "Extension id"),
                ArgumentSchema::new("pinned", ArgumentType::Bool, true, "Whether pinned to toolbar"),
            ],
        },
        CommandDescriptor {
            name: "extension.remove".to_string(),
            title: "Remove Extension".to_string(),
            description: "Uninstall a WebExtension".to_string(),
            arguments: vec![ArgumentSchema::new(
                "extension_id",
                ArgumentType::String,
                true,
                "Extension id",
            )],
        },
        CommandDescriptor {
            name: "extension.reload".to_string(),
            title: "Reload Extension".to_string(),
            description: "Reload an installed WebExtension".to_string(),
            arguments: vec![ArgumentSchema::new(
                "extension_id",
                ArgumentType::String,
                true,
                "Extension id",
            )],
        },
        CommandDescriptor {
            name: "extension.install_path".to_string(),
            title: "Install Extension From Path".to_string(),
            description: "Install an unpacked extension folder or .crx/.xpi/.zip package".to_string(),
            arguments: vec![ArgumentSchema::new(
                "path",
                ArgumentType::String,
                true,
                "Path to extension directory or package",
            )],
        },
        CommandDescriptor {
            name: "extension.open_options".to_string(),
            title: "Open Extension Options".to_string(),
            description: "Open an installed WebExtension's options page in a new tab".to_string(),
            arguments: vec![ArgumentSchema::new(
                "extension_id",
                ArgumentType::String,
                true,
                "Extension id",
            )],
        },
        CommandDescriptor {
            name: "extension.prompt_answer".to_string(),
            title: "Answer Extension Prompt".to_string(),
            description: "Accept or cancel a pending WebExtension consent prompt".to_string(),
            arguments: vec![
                ArgumentSchema::new("token", ArgumentType::Integer, true, "Prompt token"),
                ArgumentSchema::new("accepted", ArgumentType::Bool, true, "Whether user accepted"),
            ],
        },
        CommandDescriptor {
            name: "extension.context_menu_click".to_string(),
            title: "Click Extension Context Menu Item".to_string(),
            description: "Invoke an extension-registered page context menu item".to_string(),
            arguments: vec![
                ArgumentSchema::new("extension_id", ArgumentType::String, true, "Extension id"),
                ArgumentSchema::new("item_id", ArgumentType::String, true, "Context menu item id"),
                ArgumentSchema::new("target_json", ArgumentType::String, false, "Serialized click target"),
            ],
        },
        CommandDescriptor {
            name: "extension.invoke_command".to_string(),
            title: "Invoke Extension Command".to_string(),
            description: "Invoke a manifest-declared extension keyboard command".to_string(),
            arguments: vec![
                ArgumentSchema::new("extension_id", ArgumentType::String, true, "Extension id"),
                ArgumentSchema::new("name", ArgumentType::String, true, "Command name"),
            ],
        },
        CommandDescriptor {
            name: "extensions.menu.toggle".to_string(),
            title: "Extensions Menu".to_string(),
            description: "Open the menu listing every installed extension".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "extension.invoke_action".to_string(),
            title: "Run Extension Toolbar Action".to_string(),
            description: "Trigger an installed WebExtension's toolbar button action".to_string(),
            arguments: vec![ArgumentSchema::new(
                "extension_id",
                ArgumentType::String,
                true,
                "Extension id",
            )],
        },
        CommandDescriptor {
            name: "service.set_state".to_string(),
            title: "Set Service State".to_string(),
            description: "Set a background service's status (running, restarting, stopped)".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "service",
                    ArgumentType::String,
                    true,
                    "Service id (agent_server, input_server, plugin_host, extension_server)",
                ),
                ArgumentSchema::new(
                    "status",
                    ArgumentType::String,
                    true,
                    "Target status (running, restarting, stopped)",
                ),
                ArgumentSchema::new("reason", ArgumentType::String, false, "Optional stopped reason"),
            ],
        },
        CommandDescriptor {
            name: "service.cycle_state".to_string(),
            title: "Cycle Service State (Debug)".to_string(),
            description: "Cycle a background service through Running -> Restarting -> Stopped -> Running".to_string(),
            arguments: vec![ArgumentSchema::new(
                "service",
                ArgumentType::String,
                true,
                "Service id (agent_server, input_server, plugin_host, extension_server)",
            )],
        },
        CommandDescriptor {
            name: "service.restart".to_string(),
            title: "Restart Service".to_string(),
            description: "Restart a background service process back to Running".to_string(),
            arguments: vec![ArgumentSchema::new(
                "service",
                ArgumentType::String,
                true,
                "Service id (agent_server, input_server, plugin_host, extension_server)",
            )],
        },
        CommandDescriptor {
            name: "agent.select_and_open".to_string(),
            title: "Select Agent and Open Panel".to_string(),
            description: "Switch to an installed AI agent and open the Agent panel".to_string(),
            arguments: vec![ArgumentSchema::new("agent_id", ArgumentType::String, true, "Agent id")],
        },
        CommandDescriptor {
            name: "statusline.set_enabled".to_string(),
            title: "Show or Hide Vim Mode Indicator".to_string(),
            description: "Enable or disable showing Zendia's Vim mode indicator in the corner".to_string(),
            arguments: vec![ArgumentSchema::new(
                "enabled",
                ArgumentType::Bool,
                true,
                "Whether the Vim mode indicator is shown",
            )],
        },
        CommandDescriptor {
            name: "ublock.popup".to_string(),
            title: "uBlock Origin Popup".to_string(),
            description: "Open uBlock Origin's popup with keyboard hints".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "ublock.zapper".to_string(),
            title: "uBlock Element Zapper".to_string(),
            description: "Launch uBlock Origin's element zapper on the active page".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "ublock.picker".to_string(),
            title: "uBlock Element Picker".to_string(),
            description: "Launch uBlock Origin's element picker on the active page".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "ublock.logger".to_string(),
            title: "uBlock Logger".to_string(),
            description: "Open uBlock Origin's request logger".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "ublock.dashboard".to_string(),
            title: "uBlock Settings Dashboard".to_string(),
            description: "Open uBlock Origin's dashboard".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "ublock.relax".to_string(),
            title: "uBlock Relax Blocking".to_string(),
            description: "Relax uBlock Origin's blocking mode on the active page".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "ublock.toggle_cosmetic".to_string(),
            title: "uBlock Toggle Cosmetic Filtering".to_string(),
            description: "Toggle uBlock Origin cosmetic filtering on the active site".to_string(),
            arguments: Vec::new(),
        },
        CommandDescriptor {
            name: "ublock.toggle_javascript".to_string(),
            title: "uBlock Toggle JavaScript".to_string(),
            description: "Toggle JavaScript blocking in uBlock Origin on the active site".to_string(),
            arguments: Vec::new(),
        },
    ]
}

/// Synchronizes extension toolbar items, site extensions, and plugin side panels inside `model` with current module and
/// service states.
pub fn sync_module_contributions(model: &mut BrowserModel) {
    let ext_status = service_status_in(&model.services, ServiceId::ExtensionServer);

    // Replace only Extension-sourced toolbar items, preserving all built-in items and order.
    model
        .contributions
        .toolbar_items
        .retain(|item| !matches!(item.source, ContributionSource::Extension { .. }));
    for ext in &model.extensions {
        if ext.enabled && ext.pinned_to_toolbar {
            model
                .contributions
                .toolbar_items
                .push(extension_to_toolbar_item(ext, &ext_status));
        }
    }

    // Keep site panel extension tiles in sync with unpinned extensions (single-location rule).
    model.site_extensions = unpinned_site_extensions(&model.extensions);

    // Replace only Plugin-sourced side panel entries.
    model
        .contributions
        .side_panels
        .retain(|panel| !matches!(panel.source, ContributionSource::Plugin { .. }));
    for plugin in &model.plugins {
        if let Some(panel) = plugin_to_side_panel(plugin) {
            model.contributions.side_panels.push(panel);
        }
    }
}

impl StubBackend {
    /// Initializes live module contributions for the real browser window (`new_for_external_tabs`).
    ///
    /// Clears the gallery's sample WebExtension and sets `ExtensionServer` to `Starting` so the real
    /// `ExtensionServer` process owns installed extensions and service status.
    pub(crate) fn enable_live_modules(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.model.extensions.clear();
        state.model.extension_commands.clear();
        state.model.pending_extension_prompt = None;
        for svc in &mut state.model.services {
            if svc.service == ServiceId::PluginHost {
                svc.status = ServiceStatus::Running;
            } else if svc.service == ServiceId::ExtensionServer {
                svc.status = ServiceStatus::Starting;
            }
        }
        sync_module_contributions(&mut state.model);
    }

    pub(crate) fn handle_settings_or_module_command(&self, command: Command) -> CommandResult {
        match command {
            Command::SettingsOpen { group } => {
                self.request_view(ViewRequest::SettingsOpen { group });
                Ok(())
            }
            Command::SettingsClose => {
                self.request_view(ViewRequest::SettingsClose);
                Ok(())
            }
            Command::SettingsSet { key, value } => {
                let settings = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    apply_browser_setting(&mut state.model.settings, &key, &value).map_err(|message| {
                        CommandError::InvalidArgument {
                            command: "settings.set".to_string(),
                            message,
                        }
                    })?;
                    state.model.settings.clone()
                };
                if key == "vim.leader" {
                    self.refresh_active_vim_profile();
                }
                self.bus.publish(Event::BrowserSettingsChanged { settings });
                Ok(())
            }
            Command::SettingsResetGroup { group } => {
                let settings = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    reset_browser_settings_group(&mut state.model.settings, group);
                    state.model.settings.clone()
                };
                if group == SettingsGroupId::Vim {
                    self.refresh_active_vim_profile();
                }
                self.bus.publish(Event::BrowserSettingsChanged { settings });
                Ok(())
            }
            Command::PluginToggle { plugin_id } => {
                let target_enabled = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state
                        .model
                        .plugins
                        .iter()
                        .find(|p| p.id == plugin_id)
                        .map(|p| !p.enabled)
                        .ok_or_else(|| CommandError::Failed(format!("unknown plugin `{}`", plugin_id.0)))?
                };
                self.handle_settings_or_module_command(Command::PluginSetEnabled {
                    plugin_id,
                    enabled: target_enabled,
                })
            }
            Command::PluginSetEnabled { plugin_id, enabled } => {
                let (plugins_enabled, plugins, contributions) = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let host_running = matches!(
                        service_status_in(&state.model.services, ServiceId::PluginHost),
                        ServiceStatus::Running
                    );
                    let Some(plugin) = state.model.plugins.iter_mut().find(|p| p.id == plugin_id) else {
                        return Err(CommandError::Failed(format!("unknown plugin `{}`", plugin_id.0)));
                    };
                    plugin.enabled = enabled;
                    plugin.status = if !enabled {
                        PluginStatus::Disabled
                    } else if host_running {
                        PluginStatus::Running
                    } else {
                        PluginStatus::Stopped {
                            reason: Some("PluginHost is not running".to_string()),
                        }
                    };
                    sync_module_contributions(&mut state.model);
                    (
                        state.model.plugins_enabled,
                        state.model.plugins.clone(),
                        state.model.contributions.clone(),
                    )
                };
                self.bus.publish(Event::PluginsChanged {
                    enabled: plugins_enabled,
                    plugins,
                });
                self.bus.publish(Event::ContributionsChanged { contributions });
                Ok(())
            }
            Command::PluginSetAllEnabled { enabled } => {
                let (plugins, contributions) = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let host_running = matches!(
                        service_status_in(&state.model.services, ServiceId::PluginHost),
                        ServiceStatus::Running
                    );
                    state.model.plugins_enabled = enabled;
                    for plugin in &mut state.model.plugins {
                        if !enabled {
                            plugin.status = PluginStatus::Disabled;
                        } else if plugin.enabled {
                            plugin.status = if host_running {
                                PluginStatus::Running
                            } else {
                                PluginStatus::Stopped {
                                    reason: Some("PluginHost is not running".to_string()),
                                }
                            };
                        }
                    }
                    sync_module_contributions(&mut state.model);
                    (state.model.plugins.clone(), state.model.contributions.clone())
                };
                self.bus.publish(Event::PluginsChanged { enabled, plugins });
                self.bus.publish(Event::ContributionsChanged { contributions });
                Ok(())
            }
            Command::PluginRemove { plugin_id } => {
                let (plugins_enabled, plugins, contributions, panel_to_close) = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let panel_to_close = state
                        .model
                        .plugins
                        .iter()
                        .find(|p| p.id == plugin_id)
                        .and_then(|p| p.panel_id.clone());
                    let before = state.model.plugins.len();
                    state.model.plugins.retain(|p| p.id != plugin_id);
                    if state.model.plugins.len() == before {
                        return Err(CommandError::Failed(format!("unknown plugin `{}`", plugin_id.0)));
                    }
                    sync_module_contributions(&mut state.model);
                    (
                        state.model.plugins_enabled,
                        state.model.plugins.clone(),
                        state.model.contributions.clone(),
                        panel_to_close,
                    )
                };
                if let Some(panel_id) = panel_to_close {
                    self.request_view(ViewRequest::PanelClose { panel_id });
                }
                self.bus.publish(Event::PluginsChanged {
                    enabled: plugins_enabled,
                    plugins,
                });
                self.bus.publish(Event::ContributionsChanged { contributions });
                Ok(())
            }
            Command::PluginOpenPanel { plugin_id } => {
                let (panel_id, contributions) = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    sync_module_contributions(&mut state.model);
                    let panel_id = state
                        .model
                        .plugins
                        .iter()
                        .find(|p| p.id == plugin_id)
                        .and_then(|p| p.panel_id.clone())
                        .ok_or_else(|| {
                            CommandError::Failed(format!("plugin `{}` does not declare a panel", plugin_id.0))
                        })?;
                    (panel_id, state.model.contributions.clone())
                };
                self.bus.publish(Event::ContributionsChanged { contributions });
                self.request_view(ViewRequest::PanelOpen { panel_id, focus: true });
                Ok(())
            }
            Command::PluginRestart { plugin_id } => {
                let (plugins_enabled, plugins, svc_state, contributions) = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    if let Some(svc) = state
                        .model
                        .services
                        .iter_mut()
                        .find(|s| s.service == ServiceId::PluginHost)
                    {
                        svc.status = ServiceStatus::Running;
                    }
                    let Some(plugin) = state.model.plugins.iter_mut().find(|p| p.id == plugin_id) else {
                        return Err(CommandError::Failed(format!("unknown plugin `{}`", plugin_id.0)));
                    };
                    plugin.enabled = true;
                    plugin.status = PluginStatus::Running;
                    sync_module_contributions(&mut state.model);
                    (
                        state.model.plugins_enabled,
                        state.model.plugins.clone(),
                        ServiceState {
                            service: ServiceId::PluginHost,
                            status: ServiceStatus::Running,
                        },
                        state.model.contributions.clone(),
                    )
                };
                self.bus.publish(Event::ServiceStateChanged { state: svc_state });
                self.bus.publish(Event::PluginsChanged {
                    enabled: plugins_enabled,
                    plugins,
                });
                self.bus.publish(Event::ContributionsChanged { contributions });
                Ok(())
            }
            Command::ExtensionToggle { extension_id } => {
                let next = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state
                        .model
                        .extensions
                        .iter()
                        .find(|e| e.id == extension_id)
                        .map(|e| !e.enabled)
                        .ok_or_else(|| CommandError::Failed(format!("unknown extension `{}`", extension_id.0)))?
                };
                self.handle_settings_or_module_command(Command::ExtensionSetEnabled {
                    extension_id,
                    enabled: next,
                })
            }
            Command::ExtensionSetEnabled { extension_id, enabled } => {
                if let Some(source) = self.current_extension_source() {
                    return source.set_enabled(&extension_id, enabled);
                }
                let (extensions, contributions) = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let Some(ext) = state.model.extensions.iter_mut().find(|e| e.id == extension_id) else {
                        return Err(CommandError::Failed(format!("unknown extension `{}`", extension_id.0)));
                    };
                    ext.enabled = enabled;
                    sync_module_contributions(&mut state.model);
                    (state.model.extensions.clone(), state.model.contributions.clone())
                };
                self.bus.publish(Event::ExtensionsChanged { extensions });
                self.bus.publish(Event::ContributionsChanged { contributions });
                Ok(())
            }
            Command::ExtensionSetToolbarPinned { extension_id, pinned } => {
                if let Some(source) = self.current_extension_source() {
                    return source.set_toolbar_pinned(&extension_id, pinned);
                }
                let (extensions, contributions) = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let Some(ext) = state.model.extensions.iter_mut().find(|e| e.id == extension_id) else {
                        return Err(CommandError::Failed(format!("unknown extension `{}`", extension_id.0)));
                    };
                    ext.pinned_to_toolbar = pinned;
                    sync_module_contributions(&mut state.model);
                    (state.model.extensions.clone(), state.model.contributions.clone())
                };
                self.bus.publish(Event::ExtensionsChanged { extensions });
                self.bus.publish(Event::ContributionsChanged { contributions });
                Ok(())
            }
            Command::ExtensionInvokeActionFromKeyboard { extension_id } => {
                self.mark_extension_popup_from_keyboard();
                self.handle_settings_or_module_command(Command::ExtensionInvokeAction { extension_id })
            }
            Command::ExtensionInvokeAction { extension_id } => {
                if let Some(source) = self.current_extension_source() {
                    let status = {
                        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                        service_status_in(&state.model.services, ServiceId::ExtensionServer)
                    };
                    if !matches!(status, ServiceStatus::Running) {
                        let ext_name = {
                            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                            state
                                .model
                                .extensions
                                .iter()
                                .find(|e| e.id == extension_id)
                                .map(|e| e.name.clone())
                                .unwrap_or_else(|| extension_id.0.clone())
                        };
                        self.show_toast(
                            format!("{ext_name} is paused — ExtensionServer is not running"),
                            None,
                            Some(2800),
                        );
                        return Ok(());
                    }
                    return source.invoke_action(&extension_id);
                }
                let (ext_name, service_status) = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let name = state
                        .model
                        .extensions
                        .iter()
                        .find(|e| e.id == extension_id)
                        .map(|e| e.name.clone())
                        .unwrap_or_else(|| extension_id.0.clone());
                    let status = service_status_in(&state.model.services, ServiceId::ExtensionServer);
                    (name, status)
                };
                let message = if matches!(service_status, ServiceStatus::Running) {
                    format!("{ext_name}: action triggered (ExtensionServer stub)")
                } else {
                    format!("{ext_name} is paused — ExtensionServer is not running")
                };
                self.show_toast(message, None, Some(2800));
                Ok(())
            }
            Command::ExtensionRemove { extension_id } => {
                if let Some(source) = self.current_extension_source() {
                    {
                        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                        let was_ublock = state
                            .model
                            .extensions
                            .iter()
                            .any(|e| e.id == extension_id && is_ublock_extension(&e.id.0, &e.name));
                        if was_ublock {
                            state.model.shield_enabled = false;
                        }
                    }
                    return source.remove(&extension_id);
                }
                let (extensions, contributions) = {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let was_ublock = state
                        .model
                        .extensions
                        .iter()
                        .any(|e| e.id == extension_id && is_ublock_extension(&e.id.0, &e.name));
                    let before = state.model.extensions.len();
                    state.model.extensions.retain(|e| e.id != extension_id);
                    if state.model.extensions.len() == before {
                        return Err(CommandError::Failed(format!("unknown extension `{}`", extension_id.0)));
                    }
                    if was_ublock {
                        state.model.shield_enabled = false;
                    }
                    sync_module_contributions(&mut state.model);
                    (state.model.extensions.clone(), state.model.contributions.clone())
                };
                self.bus.publish(Event::ExtensionsChanged { extensions });
                self.bus.publish(Event::ContributionsChanged { contributions });
                Ok(())
            }
            Command::UblockInvoke { action } => {
                let target_ext = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    let running = matches!(
                        service_status_in(&state.model.services, ServiceId::ExtensionServer),
                        ServiceStatus::Running
                    );
                    if !running {
                        None
                    } else {
                        state
                            .model
                            .extensions
                            .iter()
                            .find(|e| e.enabled && is_ublock_extension(&e.id.0, &e.name))
                            .map(|e| e.id.clone())
                    }
                };
                let Some(extension_id) = target_ext else {
                    self.show_toast(
                        "uBlock Origin is turned off or not installed.".to_string(),
                        None,
                        Some(2800),
                    );
                    return Ok(());
                };
                match action.manifest_command() {
                    None => {
                        self.mark_extension_popup_from_keyboard();
                        self.handle_settings_or_module_command(Command::ExtensionInvokeAction { extension_id })
                    }
                    Some(cmd_name) => self.handle_settings_or_module_command(Command::ExtensionInvokeCommand {
                        extension_id,
                        name: cmd_name.to_string(),
                    }),
                }
            }
            Command::ExtensionReload { extension_id } => {
                if let Some(source) = self.current_extension_source() {
                    return source.reload(&extension_id);
                }
                self.show_toast(format!("Reloaded extension {}", extension_id.0), None, Some(2000));
                Ok(())
            }
            Command::ExtensionInstallPath { path } => {
                if let Some(source) = self.current_extension_source() {
                    return source.install_path(&path);
                }
                self.show_toast(
                    format!("Extension install requires ExtensionServer ({path})"),
                    None,
                    Some(2600),
                );
                Ok(())
            }
            Command::ExtensionOpenOptions { extension_id } => {
                let options_url = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    state
                        .model
                        .extensions
                        .iter()
                        .find(|e| e.id == extension_id)
                        .and_then(|e| e.options_url.clone())
                };
                let Some(url) = options_url else {
                    return Err(CommandError::Failed(format!(
                        "extension `{}` has no options page",
                        extension_id.0
                    )));
                };
                use crate::BrowserCore;
                self.invoke(Command::TabNew {
                    url: Some(url),
                    after: None,
                    activate: true,
                })
            }
            Command::ExtensionPromptAnswer { token, accepted } => {
                self.feed_extension_prompt(None);
                if let Some(source) = self.current_extension_source() {
                    return source.answer_prompt(token, accepted);
                }
                Ok(())
            }
            Command::ExtensionContextMenuClick {
                extension_id,
                item_id,
                target_json,
            } => {
                if let Some(source) = self.current_extension_source() {
                    return source.context_menu_clicked(&extension_id, &item_id, &target_json);
                }
                Ok(())
            }
            Command::ExtensionInvokeCommand { extension_id, name } => {
                if let Some(source) = self.current_extension_source() {
                    return source.invoke_command(&extension_id, &name);
                }
                Ok(())
            }
            Command::ExtensionsMenuToggle => {
                self.request_view(ViewRequest::ExtensionsMenuToggle);
                Ok(())
            }
            Command::ServiceSetState { service, status } => {
                self.apply_service_status(service, status);
                Ok(())
            }
            Command::ServiceCycleState { service } => {
                let current = {
                    let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    service_status_in(&state.model.services, service)
                };
                let next = cycle_service_status(service, &current);
                let label = service_label(service);
                let status_text = service_status_label(&next);
                self.apply_service_status(service, next);
                self.show_toast(format!("{label}: {status_text}"), None, Some(2200));
                Ok(())
            }
            Command::ServiceRestart { service } => {
                if service == ServiceId::ExtensionServer
                    && let Some(source) = self.current_extension_source()
                {
                    self.apply_service_status(service, ServiceStatus::Restarting);
                    source.restart_service()?;
                    self.show_toast("Restarting ExtensionServer…".to_string(), None, Some(2200));
                    return Ok(());
                }
                self.apply_service_status(service, ServiceStatus::Running);
                self.show_toast(format!("{} restarted", service_label(service)), None, Some(2200));
                Ok(())
            }
            _ => unreachable!("non-settings/module command routed to settings_modules handler"),
        }
    }

    pub(crate) fn apply_service_status_pub(&self, service: ServiceId, status: ServiceStatus) {
        self.apply_service_status(service, status);
    }

    fn apply_service_status(&self, service: ServiceId, status: ServiceStatus) {
        let (svc_state, plugins_update, contributions_update) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let svc_state = ServiceState {
                service,
                status: status.clone(),
            };
            if let Some(existing) = state.model.services.iter_mut().find(|s| s.service == service) {
                *existing = svc_state.clone();
            } else {
                state.model.services.push(svc_state.clone());
            }

            let mut plugins_update = None;
            let mut contributions_update = None;

            if service == ServiceId::PluginHost {
                let all_enabled = state.model.plugins_enabled;
                for plugin in &mut state.model.plugins {
                    if !all_enabled || !plugin.enabled {
                        plugin.status = PluginStatus::Disabled;
                    } else {
                        plugin.status = match &status {
                            ServiceStatus::Running => PluginStatus::Running,
                            ServiceStatus::Starting | ServiceStatus::Restarting => PluginStatus::Restarting,
                            ServiceStatus::Stopped { reason } => PluginStatus::Stopped {
                                reason: reason
                                    .clone()
                                    .or_else(|| Some(service_down_summary(ServiceId::PluginHost).to_string())),
                            },
                        };
                    }
                }
                plugins_update = Some((state.model.plugins_enabled, state.model.plugins.clone()));
            }

            if matches!(service, ServiceId::ExtensionServer | ServiceId::PluginHost) {
                sync_module_contributions(&mut state.model);
                contributions_update = Some(state.model.contributions.clone());
            }

            (svc_state, plugins_update, contributions_update)
        };

        self.bus.publish(Event::ServiceStateChanged { state: svc_state });
        if let Some((enabled, plugins)) = plugins_update {
            self.bus.publish(Event::PluginsChanged { enabled, plugins });
        }
        if let Some(contributions) = contributions_update {
            self.bus.publish(Event::ContributionsChanged { contributions });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrowserCore;
    use crate::modules::{SAMPLE_EXTENSION_ID, SAMPLE_PLUGIN_ID};
    use crate::settings::{GlanceTrigger, PinnedTabCloseBehavior};

    #[test]
    fn settings_open_and_set_commands_round_trip() {
        let core = StubBackend::new();
        let sub = core.subscribe();

        let open_cmd = Command::from_name_and_args("settings.open", &[("group".into(), "glance".into())]).unwrap();
        core.invoke(open_cmd).unwrap();
        assert_eq!(
            sub.try_recv(),
            Ok(Event::ViewRequested {
                window: None,
                request: ViewRequest::SettingsOpen {
                    group: Some(SettingsGroupId::Glance),
                },
            })
        );

        let set_cmd = Command::from_name_and_args(
            "settings.set",
            &[("key".into(), "glance.trigger".into()), ("value".into(), "alt".into())],
        )
        .unwrap();
        core.invoke(set_cmd).unwrap();
        assert_eq!(core.model().settings.glance_trigger, GlanceTrigger::Alt);

        let set_pinned = Command::SettingsSet {
            key: "pinned.close_shortcut_behavior".to_string(),
            value: "close".to_string(),
        };
        core.invoke(set_pinned).unwrap();
        assert_eq!(
            core.model().settings.pinned_tab_close_behavior,
            PinnedTabCloseBehavior::Close
        );

        let mut model = core.model();
        core.invoke(Command::SettingsResetGroup {
            group: SettingsGroupId::Glance,
        })
        .unwrap();
        while let Ok(ev) = sub.try_recv() {
            model.apply_event(&ev);
        }
        assert_eq!(model.settings.glance_trigger, GlanceTrigger::Ctrl);
    }

    #[test]
    fn plugin_extension_and_service_cycle_commands_update_model_and_contributions() {
        let core = StubBackend::new();
        let sub = core.subscribe();
        let mut view_model = core.model();

        // Start ExtensionServer and pin sample extension to toolbar.
        core.invoke(Command::ServiceSetState {
            service: ServiceId::ExtensionServer,
            status: ServiceStatus::Running,
        })
        .unwrap();
        core.invoke(Command::ExtensionSetToolbarPinned {
            extension_id: ExtensionId::new(SAMPLE_EXTENSION_ID),
            pinned: true,
        })
        .unwrap();
        while let Ok(ev) = sub.try_recv() {
            view_model.apply_event(&ev);
        }
        let ext_item = view_model
            .contributions
            .toolbar_items
            .iter()
            .find(|item| matches!(item.source, ContributionSource::Extension { .. }))
            .expect("pinned sample extension should contribute a toolbar button");
        assert!(ext_item.enabled);
        assert!(
            view_model.site_extensions.is_empty(),
            "pinned extension must leave the site panel (single-location rule)"
        );

        // Cycle ExtensionServer -> Restarting -> Stopped -> Running.
        core.invoke(Command::ServiceCycleState {
            service: ServiceId::ExtensionServer,
        })
        .unwrap();
        assert_eq!(
            service_status_in(&core.model().services, ServiceId::ExtensionServer),
            ServiceStatus::Restarting
        );
        core.invoke(Command::ServiceCycleState {
            service: ServiceId::ExtensionServer,
        })
        .unwrap();
        assert!(matches!(
            service_status_in(&core.model().services, ServiceId::ExtensionServer),
            ServiceStatus::Stopped { .. }
        ));
        let paused_item = core
            .model()
            .contributions
            .toolbar_items
            .into_iter()
            .find(|item| matches!(item.source, ContributionSource::Extension { .. }))
            .unwrap();
        assert!(!paused_item.enabled);

        // Toggle sample plugin and restart it.
        core.invoke(Command::PluginToggle {
            plugin_id: PluginId::new(SAMPLE_PLUGIN_ID),
        })
        .unwrap();
        assert!(!core.model().plugins[0].enabled);

        core.invoke(Command::PluginRestart {
            plugin_id: PluginId::new(SAMPLE_PLUGIN_ID),
        })
        .unwrap();
        assert!(core.model().plugins[0].enabled);
        assert_eq!(core.model().plugins[0].status, PluginStatus::Running);

        // Unpin extension removes it from toolbar contributions and restores it to the site panel.
        core.invoke(Command::ExtensionSetToolbarPinned {
            extension_id: ExtensionId::new(SAMPLE_EXTENSION_ID),
            pinned: false,
        })
        .unwrap();
        while let Ok(ev) = sub.try_recv() {
            view_model.apply_event(&ev);
        }
        assert!(
            !core
                .model()
                .contributions
                .toolbar_items
                .iter()
                .any(|item| matches!(item.source, ContributionSource::Extension { .. }))
        );
        assert_eq!(
            view_model.site_extensions.len(),
            1,
            "unpinned extension must return to the site panel in view model"
        );
        assert_eq!(view_model.site_extensions[0].badge_text.as_deref(), Some("4"));
    }

    #[test]
    fn extensions_open_parses_to_settings_extensions_group() {
        let cmd = Command::from_name_and_args("extensions.open", &[]).unwrap();
        assert_eq!(
            cmd,
            Command::SettingsOpen {
                group: Some(SettingsGroupId::Extensions),
            }
        );
    }

    #[test]
    fn ublock_popup_marks_compact_auto_toggle_and_panel_rules_match_fork() {
        use crate::agent::AGENT_PANEL_ID;
        use crate::contributions::{SidePanelId, SidePanelLayout};
        use crate::model::CompactHide;
        use crate::webchat::WEBCHAT_PANEL_ID;

        let core = StubBackend::new();
        let sub = core.subscribe();

        // CFG-03: Enabling vim mode auto-enables compact mode (`Both`) until user customizes compact mode.
        assert!(!core.model().layout.compact_mode);
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        assert!(core.model().layout.compact_mode);
        assert_eq!(core.model().layout.compact_hide, CompactHide::Both);
        core.invoke(Command::VimSetEnabled { enabled: false }).unwrap();
        assert!(!core.model().layout.compact_mode);

        core.invoke(Command::LayoutSetCompactMode { enabled: false }).unwrap();
        assert!(core.user_customized_compact());
        core.invoke(Command::VimSetEnabled { enabled: true }).unwrap();
        assert!(!core.model().layout.compact_mode);
        core.invoke(Command::from_name_and_args("layout.reset", &[]).unwrap())
            .unwrap();
        assert!(!core.user_customized_compact());
        assert!(core.model().layout.compact_mode);

        // EXT-06 & EXT-01: When ExtensionServer is stopped, ublock.popup toasts; when running, marks popup keyboard-opened.
        core.invoke(Command::ServiceSetState {
            service: ServiceId::ExtensionServer,
            status: ServiceStatus::Stopped { reason: None },
        })
        .unwrap();
        core.invoke(Command::from_name_and_args("ublock.popup", &[]).unwrap())
            .unwrap();
        assert!(
            core.model()
                .toasts
                .iter()
                .any(|t| t.message == "uBlock Origin is turned off or not installed.")
        );

        core.invoke(Command::ServiceSetState {
            service: ServiceId::ExtensionServer,
            status: ServiceStatus::Running,
        })
        .unwrap();
        while sub.try_recv().is_ok() {}
        core.invoke(Command::from_name_and_args("ublock.popup", &[]).unwrap())
            .unwrap();
        core.open_extension_popup(ExtensionId::new(SAMPLE_EXTENSION_ID), "ext://popup.html".to_string());
        let mut saw_kbd_popup = false;
        while let Ok(ev) = sub.try_recv() {
            if let Event::ViewRequested {
                request: ViewRequest::ExtensionPopupOpen { from_keyboard, .. },
                ..
            } = ev
            {
                saw_kbd_popup = from_keyboard;
            }
        }
        assert!(saw_kbd_popup, "ublock.popup should mark popup as keyboard-opened");

        // AGK-06 & AGK-07: `agent.select_and_open` selects the agent and closes WebChat; Explorer closes non-Split Agent panel.
        core.invoke(Command::PanelOpen {
            panel_id: SidePanelId::new(WEBCHAT_PANEL_ID),
            focus: true,
        })
        .unwrap();
        while sub.try_recv().is_ok() {}
        core.invoke(
            Command::from_name_and_args("agent.select_and_open", &[("agent_id".into(), "codex".into())]).unwrap(),
        )
        .unwrap();
        assert_eq!(core.model().default_agent_id.unwrap().0, "codex");
        let mut closed_webchat = false;
        while let Ok(ev) = sub.try_recv() {
            if matches!(
                ev,
                Event::ViewRequested {
                    request: ViewRequest::PanelClose { ref panel_id },
                    ..
                } if panel_id.as_str() == WEBCHAT_PANEL_ID
            ) {
                closed_webchat = true;
            }
        }
        assert!(closed_webchat);

        core.invoke(Command::ExplorerOpen).unwrap();
        let mut closed_float_agent = false;
        while let Ok(ev) = sub.try_recv() {
            if matches!(
                ev,
                Event::ViewRequested {
                    request: ViewRequest::PanelClose { ref panel_id },
                    ..
                } if panel_id.as_str() == AGENT_PANEL_ID
            ) {
                closed_float_agent = true;
            }
        }
        assert!(closed_float_agent, "floating agent panel should close on Explorer open");
        core.invoke(Command::ExplorerClose).unwrap();

        core.invoke(Command::PanelSetLayout {
            panel_id: SidePanelId::new(AGENT_PANEL_ID),
            layout: SidePanelLayout::Split,
        })
        .unwrap();
        core.invoke(Command::PanelOpen {
            panel_id: SidePanelId::new(AGENT_PANEL_ID),
            focus: true,
        })
        .unwrap();
        while sub.try_recv().is_ok() {}
        core.invoke(Command::ExplorerOpen).unwrap();
        let mut closed_split_agent = false;
        while let Ok(ev) = sub.try_recv() {
            if matches!(
                ev,
                Event::ViewRequested {
                    request: ViewRequest::PanelClose { ref panel_id },
                    ..
                } if panel_id.as_str() == AGENT_PANEL_ID
            ) {
                closed_split_agent = true;
            }
        }
        assert!(!closed_split_agent, "split agent panel must stay open on Explorer open");
    }
}
