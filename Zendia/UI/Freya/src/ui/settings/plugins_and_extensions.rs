/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Plugins (replacing Zen Mods), WebExtensions, and Background Service health settings groups.

use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, KitSwitch, Pill, use_interactive_feedback};
use crate::ui::settings::controls::{
    ServiceStatusBanner, SettingsButton, SettingsCheckboxRow, SettingsPalette, SettingsToggleRow, section_heading,
    settings_card,
};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{ServiceId, ServiceStatus};
use zendia_browser_core::modules::{
    ExtensionInfo, MANAGED_SERVICES, PluginInfo, PluginStatus, service_description, service_down_summary,
    service_label, service_status_in, service_status_label,
};
use zendia_module_api::ui::UiTone;

/// Plugins group, laid out like `settings-mods-*.png` with master switch, action bar, plugin cards, details, and
/// stopped-plugin recovery.
#[derive(Clone, PartialEq)]
pub struct PluginsGroup;

impl Component for PluginsGroup {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let (plugins_enabled, auto_update, plugins, host_status) = {
            let model = core.model().read();
            (
                model.plugins_enabled,
                model.settings.plugins_auto_update,
                model.plugins.clone(),
                service_status_in(&model.services, ServiceId::PluginHost),
            )
        };

        let host_down = !matches!(host_status, ServiceStatus::Running);
        let core_master = core.clone();
        let core_auto = core.clone();
        let core_reload = core.clone();
        let core_updates = core.clone();
        let core_restart_host = core.clone();

        let plugin_cards: Vec<Element> = plugins
            .iter()
            .map(|plugin| {
                PluginCard {
                    plugin: plugin.clone(),
                    master_enabled: plugins_enabled,
                    host_status: host_status.clone(),
                    key: DiffKey::from(&plugin.id.0),
                }
                .into_element()
            })
            .collect();

        rect()
            .width(Size::fill())
            .spacing(20.0)
            .child(section_heading("Plugins", None))
            .child(
                rect()
                    .width(Size::fill())
                    .spacing(12.0)
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .main_align(Alignment::SpaceBetween)
                            .cross_align(Alignment::center())
                            .child(
                                rect()
                                    .width(Size::flex(1.0))
                                    .spacing(3.0)
                                    .child(
                                        label()
                                            .text("Zendia Plugins")
                                            .font_size(14.5)
                                            .font_weight(FontWeight::SEMI_BOLD)
                                            .color(theme.color(ThemeRole::Text)),
                                    )
                                    .child(
                                        label()
                                            .text("Sandboxed plugins run in isolated PluginHost processes with declared native UI.")
                                            .font_size(12.5)
                                            .color(theme.color(ThemeRole::TextMuted)),
                                    ),
                            )
                            .child(KitSwitch::new(plugins_enabled).on_toggle(move |next| {
                                core_master.invoke_or_log(Command::PluginSetAllEnabled { enabled: next });
                            })),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .content(Content::wrap())
                            .spacing(8.0)
                            .child(
                                SettingsButton::new("Reload Plugins", move |_| {
                                    core_reload.invoke_or_log(Command::ServiceRestart {
                                        service: ServiceId::PluginHost,
                                    });
                                })
                                .icon("reload")
                                .key("reload-plugins"),
                            )
                            .child(
                                SettingsButton::new("Check for Updates", move |_| {
                                    core_updates.invoke_or_log(Command::ToastShow {
                                        message: "All installed plugins are up to date".to_string(),
                                        timeout_ms: Some(2400),
                                        action_label: None,
                                        action_command: None,
                                    });
                                })
                                .icon("sparkles")
                                .key("check-plugin-updates"),
                            ),
                    )
                    .child(
                        SettingsCheckboxRow::new(
                            "Automatically check installed plugins for updates on startup",
                            auto_update,
                            move |next: bool| {
                                core_auto.invoke_or_log(Command::SettingsSet {
                                    key: "plugins.auto_update".to_string(),
                                    value: next.to_string(),
                                });
                            },
                        )
                        .key("plugins-auto-update"),
                    ),
            )
            .maybe_child(host_down.then(|| ServiceStatusBanner {
                service: ServiceId::PluginHost,
                status: host_status.clone(),
                on_restart: EventHandler::new(move |_| {
                    core_restart_host.invoke_or_log(Command::ServiceRestart {
                        service: ServiceId::PluginHost,
                    });
                }),
            }))
            .child(if !plugins_enabled {
                settings_card(vec![
                    label()
                        .text("All plugins are currently disabled. Turn the Plugins switch back on to resume them.")
                        .font_size(13.0)
                        .color(theme.color(ThemeRole::TextMuted))
                        .into_element(),
                ])
            } else if plugin_cards.is_empty() {
                settings_card(vec![
                    label()
                        .text("No plugins installed yet.")
                        .font_size(13.0)
                        .color(theme.color(ThemeRole::TextMuted))
                        .into_element(),
                ])
            } else {
                rect().width(Size::fill()).spacing(12.0).children(plugin_cards).into_element()
            })
    }
}

#[derive(Clone, PartialEq)]
struct PluginCard {
    plugin: PluginInfo,
    master_enabled: bool,
    host_status: ServiceStatus,
    key: DiffKey,
}

impl KeyExt for PluginCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for PluginCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let mut details_open = use_state(|| false);
        let mut confirm_remove = use_state(|| false);

        let effective_status = if !self.master_enabled || !self.plugin.enabled {
            PluginStatus::Disabled
        } else if !matches!(self.host_status, ServiceStatus::Running) {
            match &self.host_status {
                ServiceStatus::Restarting | ServiceStatus::Starting => PluginStatus::Restarting,
                ServiceStatus::Stopped { reason } => PluginStatus::Stopped { reason: reason.clone() },
                ServiceStatus::Running => self.plugin.status.clone(),
            }
        } else {
            self.plugin.status.clone()
        };

        let status_tone = match effective_status {
            PluginStatus::Running => UiTone::Success,
            PluginStatus::Disabled => UiTone::Neutral,
            PluginStatus::Restarting => UiTone::Warning,
            PluginStatus::Stopped { .. } => UiTone::Danger,
        };

        let stopped_reason = match &effective_status {
            PluginStatus::Stopped { reason } => Some(
                reason
                    .clone()
                    .unwrap_or_else(|| "PluginHost stopped this plugin sandbox.".to_string()),
            ),
            _ => None,
        };

        let pid_toggle = self.plugin.id.clone();
        let pid_panel = self.plugin.id.clone();
        let pid_restart = self.plugin.id.clone();
        let pid_remove = self.plugin.id.clone();
        let core_toggle = core.clone();
        let core_panel = core.clone();
        let core_restart = core.clone();
        let core_remove = core.clone();

        let cap_pills: Vec<Element> = self
            .plugin
            .capabilities
            .iter()
            .map(|cap| Pill::new(cap.clone()).tone(UiTone::Neutral).into_element())
            .collect();

        let settings_rows: Vec<Element> = self
            .plugin
            .settings
            .iter()
            .map(|item| {
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::SpaceBetween)
                    .cross_align(Alignment::center())
                    .child(
                        label()
                            .text(item.label.clone())
                            .font_size(12.0)
                            .color(theme.color(ThemeRole::TextMuted)),
                    )
                    .child(
                        rect()
                            .padding(Gaps::new(2.0, 8.0, 2.0, 8.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .background(palette.menulist_bg)
                            .border(
                                Border::new()
                                    .width(1.0)
                                    .alignment(BorderAlignment::Inner)
                                    .fill(palette.card_border),
                            )
                            .child(
                                label()
                                    .text(item.value.clone())
                                    .font_size(11.5)
                                    .font_weight(FontWeight::MEDIUM)
                                    .font_family("monospace")
                                    .color(theme.color(ThemeRole::Text)),
                            ),
                    )
                    .into_element()
            })
            .collect();

        rect()
            .width(Size::fill())
            .padding(Gaps::new(14.0, 16.0, 14.0, 16.0))
            .spacing(10.0)
            .corner_radius(CornerRadius::new_all(10.0))
            .background(palette.card_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(if stopped_reason.is_some() {
                        theme.rgba(ThemeRole::Danger).with_alpha(0.42).to_color()
                    } else {
                        palette.card_border
                    }),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .main_align(Alignment::SpaceBetween)
                    .cross_align(Alignment::center())
                    .spacing(12.0)
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .cross_align(Alignment::center())
                            .spacing(10.0)
                            .child(
                                rect()
                                    .width(Size::px(32.0))
                                    .height(Size::px(32.0))
                                    .corner_radius(CornerRadius::new_all(8.0))
                                    .background(theme.rgba(ThemeRole::Accent).with_alpha(0.14).to_color())
                                    .center()
                                    .child(Icon::new(self.plugin.icon.clone()).size(16.0).role(ThemeRole::Accent)),
                            )
                            .child(
                                rect()
                                    .width(Size::flex(1.0))
                                    .spacing(2.0)
                                    .child(
                                        rect()
                                            .direction(Direction::Horizontal)
                                            .cross_align(Alignment::center())
                                            .spacing(8.0)
                                            .child(
                                                label()
                                                    .text(format!("{} (v{})", self.plugin.name, self.plugin.version))
                                                    .font_size(14.0)
                                                    .font_weight(FontWeight::SEMI_BOLD)
                                                    .color(theme.color(ThemeRole::Text)),
                                            )
                                            .child(Pill::new(effective_status.label()).tone(status_tone)),
                                    )
                                    .child(
                                        label()
                                            .text(format!("By {}", self.plugin.author))
                                            .font_size(11.5)
                                            .color(theme.color(ThemeRole::TextMuted)),
                                    ),
                            ),
                    )
                    .child(KitSwitch::new(self.plugin.enabled).on_toggle(move |next| {
                        core_toggle.invoke_or_log(Command::PluginSetEnabled {
                            plugin_id: pid_toggle.clone(),
                            enabled: next,
                        });
                    })),
            )
            .child(
                label()
                    .text(self.plugin.description.clone())
                    .font_size(12.5)
                    .color(theme.color(ThemeRole::TextMuted)),
            )
            .maybe_child(stopped_reason.map(|reason| {
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(8.0, 10.0, 8.0, 10.0))
                    .corner_radius(CornerRadius::new_all(8.0))
                    .background(theme.rgba(ThemeRole::Danger).with_alpha(0.12).to_color())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .main_align(Alignment::SpaceBetween)
                    .cross_align(Alignment::center())
                    .spacing(10.0)
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .cross_align(Alignment::center())
                            .spacing(8.0)
                            .child(Icon::new("warning").size(14.0).role(ThemeRole::Danger))
                            .child(
                                label()
                                    .width(Size::flex(1.0))
                                    .text(reason)
                                    .font_size(12.0)
                                    .color(theme.color(ThemeRole::Text)),
                            ),
                    )
                    .child(
                        SettingsButton::new("Restart Plugin", move |_| {
                            core_restart.invoke_or_log(Command::PluginRestart {
                                plugin_id: pid_restart.clone(),
                            });
                        })
                        .icon("reload")
                        .key("restart-plugin"),
                    )
            }))
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(6.0)
                    .children(cap_pills),
            )
            .maybe_child((*details_open.read()).then(|| {
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(10.0, 12.0, 10.0, 12.0))
                    .spacing(6.0)
                    .corner_radius(CornerRadius::new_all(8.0))
                    .background(palette.sidebar_bg)
                    .child(
                        label()
                            .text("Plugin Details & Options")
                            .font_size(12.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(theme.color(ThemeRole::Text)),
                    )
                    .children(settings_rows)
            }))
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(8.0)
                    .maybe_child(self.plugin.panel_id.is_some().then(|| {
                        let panel_disabled = !self.master_enabled
                            || !self.plugin.enabled
                            || !matches!(self.host_status, ServiceStatus::Running);
                        SettingsButton::new("Open Panel", move |_| {
                            core_panel.invoke_or_log(Command::PluginOpenPanel {
                                plugin_id: pid_panel.clone(),
                            });
                        })
                        .icon("sidebar-right")
                        .disabled(panel_disabled)
                        .key("open-plugin-panel")
                    }))
                    .child(
                        SettingsButton::new(
                            if *details_open.read() {
                                "Hide Details"
                            } else {
                                "Details"
                            },
                            move |_| {
                                let next = !*details_open.peek();
                                details_open.set(next);
                            },
                        )
                        .icon("sliders")
                        .key("toggle-plugin-details"),
                    )
                    .maybe_child((!*confirm_remove.read()).then(|| {
                        SettingsButton::new("Remove", move |_| {
                            confirm_remove.set(true);
                        })
                        .icon("trash")
                        .danger(true)
                        .key("remove-plugin")
                    }))
                    .maybe_child((*confirm_remove.read()).then(|| {
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(6.0)
                            .child(
                                SettingsButton::new("Confirm remove", move |_| {
                                    confirm_remove.set(false);
                                    core_remove.invoke_or_log(Command::PluginRemove {
                                        plugin_id: pid_remove.clone(),
                                    });
                                })
                                .icon("trash")
                                .danger(true)
                                .key("confirm-remove-plugin"),
                            )
                            .child(
                                SettingsButton::new("Cancel", move |_| {
                                    confirm_remove.set(false);
                                })
                                .key("cancel-remove-plugin"),
                            )
                    })),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Extensions & Services group: WebExtensions cards (with toolbar pin switch) + Background Services table with live
/// Cycle State & Restart actions.
#[derive(Clone, PartialEq)]
pub struct ExtensionsAndServicesGroup;

impl Component for ExtensionsAndServicesGroup {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let mut install_path = use_state(String::new);
        let (extensions, services) = {
            let model = core.model().read();
            (model.extensions.clone(), model.services.clone())
        };

        let ext_status = service_status_in(&services, ServiceId::ExtensionServer);
        let ext_down = !matches!(ext_status, ServiceStatus::Running);
        let core_restart_ext = core.clone();
        let core_install = core.clone();
        let core_install_submit = core.clone();

        let ext_cards: Vec<Element> = extensions
            .iter()
            .map(|ext| {
                ExtensionCard {
                    extension: ext.clone(),
                    service_status: ext_status.clone(),
                    key: DiffKey::from(&ext.id.0),
                }
                .into_element()
            })
            .collect();

        let service_rows: Vec<Element> = MANAGED_SERVICES
            .iter()
            .copied()
            .map(|service| {
                let status = service_status_in(&services, service);
                ServiceRow {
                    service,
                    status,
                    key: DiffKey::from(&service_label(service)),
                }
                .into_element()
            })
            .collect();

        let install_bar = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .child(
                crate::ui::kit::TextField::new(install_path)
                    .placeholder("Path to unpacked extension folder or .crx / .xpi / .zip package")
                    .width(Size::flex(1.0))
                    .padding(Gaps::new(7.0, 10.0, 7.0, 10.0))
                    .corner_radius(CornerRadius::new_all(8.0))
                    .background(palette.menulist_bg)
                    .focus_background(palette.menulist_bg)
                    .border(palette.card_border, 1.0)
                    .focus_border(theme.color(ThemeRole::Accent), 1.0)
                    .on_submit(move |raw: String| {
                        let trimmed = raw.trim().to_string();
                        if !trimmed.is_empty() {
                            core_install_submit.invoke_or_log(Command::ExtensionInstallPath { path: trimmed });
                            install_path.set(String::new());
                        }
                    }),
            )
            .child(
                SettingsButton::new("Load Extension", move |_| {
                    let trimmed = install_path.peek().trim().to_string();
                    if !trimmed.is_empty() {
                        core_install.invoke_or_log(Command::ExtensionInstallPath { path: trimmed });
                        install_path.set(String::new());
                    }
                })
                .icon("plus")
                .disabled(ext_down)
                .key("load-extension-path"),
            );

        let ext_list: Element = if ext_cards.is_empty() {
            settings_card(vec![
                label()
                    .text("No WebExtensions installed. Load an unpacked folder or .crx/.xpi package above, or install from the Chrome Web Store or Firefox Add-ons.")
                    .font_size(12.5)
                    .color(theme.color(ThemeRole::TextMuted))
                    .into_element(),
            ])
        } else {
            rect()
                .width(Size::fill())
                .spacing(12.0)
                .children(ext_cards)
                .into_element()
        };

        rect()
            .width(Size::fill())
            .spacing(22.0)
            .child(section_heading(
                "Extensions",
                Some("WebExtensions run in ExtensionServer and contribute ordinary toolbar buttons and site panel tiles."),
            ))
            .maybe_child(ext_down.then(|| ServiceStatusBanner {
                service: ServiceId::ExtensionServer,
                status: ext_status.clone(),
                on_restart: EventHandler::new(move |_| {
                    core_restart_ext.invoke_or_log(Command::ServiceRestart {
                        service: ServiceId::ExtensionServer,
                    });
                }),
            }))
            .child(install_bar)
            .child(ext_list)
            .child(section_heading(
                "Background Services",
                Some("Each Zendia feature runs in its own sandboxed process. Cycle a service to inspect its service-down state."),
            ))
            .child(settings_card(service_rows))
    }
}

#[derive(Clone, PartialEq)]
struct ExtensionCard {
    extension: ExtensionInfo,
    service_status: ServiceStatus,
    key: DiffKey,
}

impl KeyExt for ExtensionCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ExtensionCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let paused = !matches!(self.service_status, ServiceStatus::Running);

        let (status_label, status_tone) = if !self.extension.enabled {
            ("Disabled", UiTone::Neutral)
        } else if paused {
            ("Paused", UiTone::Warning)
        } else {
            ("Active", UiTone::Success)
        };

        let eid_en = self.extension.id.clone();
        let eid_pin = self.extension.id.clone();
        let eid_act = self.extension.id.clone();
        let eid_opt = self.extension.id.clone();
        let eid_reload = self.extension.id.clone();
        let eid_remove = self.extension.id.clone();
        let core_en = core.clone();
        let core_pin = core.clone();
        let core_act = core.clone();
        let core_opt = core.clone();
        let core_reload = core.clone();
        let core_remove = core.clone();

        let perm_pills: Vec<Element> = self
            .extension
            .permissions
            .iter()
            .map(|p| Pill::new(p.clone()).tone(UiTone::Neutral).into_element())
            .collect();

        rect()
            .width(Size::fill())
            .padding(Gaps::new(14.0, 16.0, 14.0, 16.0))
            .spacing(10.0)
            .corner_radius(CornerRadius::new_all(10.0))
            .background(palette.card_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(if paused {
                        theme.rgba(ThemeRole::Warning).with_alpha(0.45).to_color()
                    } else {
                        palette.card_border
                    }),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .main_align(Alignment::SpaceBetween)
                    .cross_align(Alignment::center())
                    .spacing(12.0)
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .cross_align(Alignment::center())
                            .spacing(10.0)
                            .child(
                                rect()
                                    .width(Size::px(32.0))
                                    .height(Size::px(32.0))
                                    .corner_radius(CornerRadius::new_all(8.0))
                                    .background(
                                        theme
                                            .rgba(if paused { ThemeRole::Warning } else { ThemeRole::Accent })
                                            .with_alpha(0.15)
                                            .to_color(),
                                    )
                                    .center()
                                    .child(crate::ui::extensions::ExtensionIconView::new(
                                        format!("settings-ext-{}", self.extension.id.as_str()),
                                        self.extension.icon.clone(),
                                        self.extension.icon_data.clone(),
                                        18.0,
                                        if paused { ThemeRole::Warning } else { ThemeRole::Accent },
                                    )),
                            )
                            .child(
                                rect()
                                    .width(Size::flex(1.0))
                                    .spacing(2.0)
                                    .child(
                                        rect()
                                            .direction(Direction::Horizontal)
                                            .cross_align(Alignment::center())
                                            .spacing(8.0)
                                            .child(
                                                label()
                                                    .text(format!(
                                                        "{} (v{})",
                                                        self.extension.name, self.extension.version
                                                    ))
                                                    .font_size(14.0)
                                                    .font_weight(FontWeight::SEMI_BOLD)
                                                    .color(theme.color(ThemeRole::Text)),
                                            )
                                            .child(Pill::new(status_label).tone(status_tone)),
                                    )
                                    .child(
                                        label()
                                            .text(self.extension.description.clone())
                                            .font_size(12.0)
                                            .color(theme.color(ThemeRole::TextMuted)),
                                    ),
                            ),
                    )
                    .child(KitSwitch::new(self.extension.enabled).on_toggle(move |next| {
                        core_en.invoke_or_log(Command::ExtensionSetEnabled {
                            extension_id: eid_en.clone(),
                            enabled: next,
                        });
                    })),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(6.0)
                    .children(perm_pills),
            )
            .child(
                SettingsToggleRow::new(
                    "Pin button to browser toolbar",
                    self.extension.pinned_to_toolbar,
                    move |next| {
                        core_pin.invoke_or_log(Command::ExtensionSetToolbarPinned {
                            extension_id: eid_pin.clone(),
                            pinned: next,
                        });
                    },
                )
                .disabled(!self.extension.enabled)
                .key("pin-ext-toolbar"),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(8.0)
                    .child(
                        SettingsButton::new("Trigger Action", move |_| {
                            core_act.invoke_or_log(Command::ExtensionInvokeAction {
                                extension_id: eid_act.clone(),
                            });
                        })
                        .icon("bolt")
                        .disabled(!self.extension.enabled || paused)
                        .key("ext-trigger-action"),
                    )
                    .maybe_child(self.extension.options_url.is_some().then(|| {
                        SettingsButton::new("Options", move |_| {
                            core_opt.invoke_or_log(Command::ExtensionOpenOptions {
                                extension_id: eid_opt.clone(),
                            });
                        })
                        .icon("sliders")
                        .disabled(!self.extension.enabled || paused)
                        .key("ext-open-options")
                    }))
                    .child(
                        SettingsButton::new("Reload", move |_| {
                            core_reload.invoke_or_log(Command::ExtensionReload {
                                extension_id: eid_reload.clone(),
                            });
                        })
                        .icon("reload")
                        .disabled(paused)
                        .key("ext-reload"),
                    )
                    .child(
                        SettingsButton::new("Remove", move |_| {
                            core_remove.invoke_or_log(Command::ExtensionRemove {
                                extension_id: eid_remove.clone(),
                            });
                        })
                        .icon("trash")
                        .danger(true)
                        .key("ext-remove"),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct ServiceRow {
    service: ServiceId,
    status: ServiceStatus,
    key: DiffKey,
}

impl KeyExt for ServiceRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ServiceRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let feedback = use_interactive_feedback(true);

        let is_running = matches!(self.status, ServiceStatus::Running);
        let tone = match self.status {
            ServiceStatus::Running => UiTone::Success,
            ServiceStatus::Starting | ServiceStatus::Restarting => UiTone::Warning,
            ServiceStatus::Stopped { .. } => UiTone::Danger,
        };
        let detail = match &self.status {
            ServiceStatus::Running => service_description(self.service).to_string(),
            ServiceStatus::Stopped { reason: Some(r) } => r.clone(),
            _ => service_down_summary(self.service).to_string(),
        };

        let svc_cycle = self.service;
        let svc_restart = self.service;
        let core_cycle = core.clone();
        let core_restart = core.clone();

        rect()
            .width(Size::fill())
            .padding(Gaps::new(8.0, 10.0, 8.0, 10.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(feedback.background(Color::TRANSPARENT, palette.row_hover_bg, palette.row_hover_bg))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .spacing(12.0)
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .spacing(2.0)
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(8.0)
                            .child(
                                label()
                                    .text(service_label(self.service))
                                    .font_size(13.5)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(theme.color(ThemeRole::Text)),
                            )
                            .child(Pill::new(service_status_label(&self.status)).tone(tone)),
                    )
                    .child(
                        label()
                            .text(detail)
                            .font_size(12.0)
                            .color(theme.color(ThemeRole::TextMuted)),
                    ),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .child(
                        SettingsButton::new("Cycle State", move |_| {
                            core_cycle.invoke_or_log(Command::ServiceCycleState { service: svc_cycle });
                        })
                        .key("cycle-svc"),
                    )
                    .maybe_child((!is_running).then(|| {
                        SettingsButton::new("Restart", move |_| {
                            core_restart.invoke_or_log(Command::ServiceRestart { service: svc_restart });
                        })
                        .icon("reload")
                        .primary(true)
                        .key("restart-svc")
                    })),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
