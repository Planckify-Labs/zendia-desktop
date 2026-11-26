/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 13 gallery section: native Settings window (every group + live search), Plugins (running & stopped),
//! Extension toolbar contributions (normal & paused), and every service-down state.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::chrome::address_bar::SiteDataPanelCard;
use crate::ui::chrome::menus::try_use_menu_state;
use crate::ui::core_view::use_core;
use crate::ui::page::{PageArea, PaneMode};
use crate::ui::panels::try_use_dock_state;
use crate::ui::settings::SettingsView;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::BrowserCore;
use zendia_browser_core::agent::AGENT_PANEL_ID;
use zendia_browser_core::contributions::SidePanelId;
use zendia_browser_core::model::{BrowserModel, ServiceId, ServiceStatus, ToolbarLayout};
use zendia_browser_core::modules::{SAMPLE_PLUGIN_PANEL_ID, default_stopped_reason, sample_stopped_plugin};
use zendia_browser_core::settings::SettingsGroupId;
use zendia_browser_core::stub::build_rich_sample_conversation;
use zendia_browser_core::stub::settings_modules::sync_module_contributions;

/// Gallery section showcasing Phase 13 Settings, Plugins, Extensions, and Service-Down states.
#[derive(Clone)]
pub struct SettingsSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
    key: DiffKey,
}

impl PartialEq for SettingsSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

impl KeyExt for SettingsSection {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl SettingsSection {
    pub fn new(state_id: impl Into<String>, gallery_core: Arc<GalleryCore>) -> Self {
        Self {
            state_id: state_id.into(),
            gallery_core,
            key: DiffKey::None,
        }
    }
}

fn set_service(model: &mut BrowserModel, service: ServiceId, status: ServiceStatus) {
    if let Some(entry) = model.services.iter_mut().find(|s| s.service == service) {
        entry.status = status;
    }
}

fn build_fixture_model(state_id: &str) -> BrowserModel {
    let mut model = super::gallery_sample_model();
    // Start all managed services healthy by default for Phase 13 fixtures, then inject specific failures per state.
    set_service(&mut model, ServiceId::AgentServer, ServiceStatus::Running);
    set_service(&mut model, ServiceId::InputServer, ServiceStatus::Running);
    set_service(&mut model, ServiceId::PluginHost, ServiceStatus::Running);
    set_service(&mut model, ServiceId::ExtensionServer, ServiceStatus::Running);

    match state_id {
        "plugins-stopped" => {
            model.plugins.insert(0, sample_stopped_plugin());
        }
        "ext-toolbar-normal" => {
            model.layout.toolbar_layout = ToolbarLayout::Multiple;
            if let Some(ext) = model.extensions.first_mut() {
                ext.pinned_to_toolbar = true;
            }
        }
        "ext-toolbar-paused" => {
            model.layout.toolbar_layout = ToolbarLayout::Multiple;
            if let Some(ext) = model.extensions.first_mut() {
                ext.pinned_to_toolbar = true;
            }
            set_service(
                &mut model,
                ServiceId::ExtensionServer,
                ServiceStatus::Stopped {
                    reason: Some(default_stopped_reason(ServiceId::ExtensionServer)),
                },
            );
        }
        "service-down-agent" => {
            set_service(
                &mut model,
                ServiceId::AgentServer,
                ServiceStatus::Stopped {
                    reason: Some(default_stopped_reason(ServiceId::AgentServer)),
                },
            );
            let active_tab = model.active_tab;
            let conv = build_rich_sample_conversation(zendia_browser_core::model::ConversationId(99), 500, active_tab);
            if let Some(tid) = active_tab {
                model.active_conversations.insert(tid, conv.id);
            }
            model.conversations.push(conv);
        }
        "service-down-plugin" => {
            set_service(
                &mut model,
                ServiceId::PluginHost,
                ServiceStatus::Stopped {
                    reason: Some(default_stopped_reason(ServiceId::PluginHost)),
                },
            );
        }
        "service-down-input" => {
            model.vim_mode_enabled = true;
            set_service(
                &mut model,
                ServiceId::InputServer,
                ServiceStatus::Stopped {
                    reason: Some(default_stopped_reason(ServiceId::InputServer)),
                },
            );
        }
        "vim" => {
            model.vim_mode_enabled = true;
        }
        _ => {}
    }

    sync_module_contributions(&mut model);
    model
}

fn initial_group_for_state(state_id: &str) -> SettingsGroupId {
    match state_id {
        "tab-management" => SettingsGroupId::TabManagement,
        "spaces" => SettingsGroupId::Spaces,
        "glance" => SettingsGroupId::Glance,
        "vim" | "service-down-input" => SettingsGroupId::Vim,
        "agents" => SettingsGroupId::Agents,
        "webchat" => SettingsGroupId::WebChat,
        "plugins" | "plugins-stopped" => SettingsGroupId::Plugins,
        "extensions-services" => SettingsGroupId::Extensions,
        _ => SettingsGroupId::LookAndFeel,
    }
}

fn initial_search_for_state(state_id: &str) -> String {
    match state_id {
        "search-results" => "sidebar".to_string(),
        "search-empty" => "nonexistent setting".to_string(),
        _ => String::new(),
    }
}

impl Component for SettingsSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let menu_state = try_use_menu_state();
        let dock_state = try_use_dock_state();
        let mut active_group = use_state(|| initial_group_for_state(&self.state_id));
        let mut search_query = use_state(|| initial_search_for_state(&self.state_id));
        let location = use_state(|| "https://zendia.local/docs".to_string());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut applied_state = use_state(String::new);

        if *applied_state.peek() != self.state_id {
            applied_state.set(self.state_id.clone());
            let mut fixture = build_fixture_model(&self.state_id);
            fixture.theme = self.gallery_core.model().theme;
            self.gallery_core.stub().replace_model(fixture);
            active_group.set(initial_group_for_state(&self.state_id));
            search_query.set(initial_search_for_state(&self.state_id));
            if let Some(ms) = menu_state {
                ms.close_all();
            }
            if let Some(mut ds) = dock_state {
                match self.state_id.as_str() {
                    "service-down-agent" => {
                        ds.open_panels.set(vec![SidePanelId::new(AGENT_PANEL_ID)]);
                        ds.focused_panel.set(Some(SidePanelId::new(AGENT_PANEL_ID)));
                    }
                    "service-down-plugin" => {
                        ds.open_panels.set(vec![SidePanelId::new(SAMPLE_PLUGIN_PANEL_ID)]);
                        ds.focused_panel.set(Some(SidePanelId::new(SAMPLE_PLUGIN_PANEL_ID)));
                    }
                    _ => {
                        ds.open_panels.set(Vec::new());
                        ds.focused_panel.set(None);
                    }
                }
            }
        }

        let is_shell_state = matches!(
            self.state_id.as_str(),
            "ext-toolbar-normal" | "ext-toolbar-paused" | "service-down-agent" | "service-down-plugin"
        );

        if is_shell_state {
            let show_site_panel_preview = matches!(self.state_id.as_str(), "ext-toolbar-normal" | "ext-toolbar-paused");
            let (site_info, site_extensions) = {
                let model = core.model().read();
                (model.active_site_info(), model.site_extensions.clone())
            };

            let stage = rect()
                .width(Size::fill())
                .height(Size::fill())
                .child(PageArea::new(PaneMode::Placeholder))
                .maybe_child(show_site_panel_preview.then(|| {
                    rect()
                        .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                        .position(Position::new_absolute().top(14.0).right(18.0))
                        .child(SiteDataPanelCard::new(site_info, site_extensions))
                }));

            return rect()
                .width(Size::fill())
                .height(Size::fill())
                .padding(Gaps::new_all(12.0))
                .background(theme.color(ThemeRole::Surface))
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::fill())
                        .corner_radius(CornerRadius::new_all(12.0))
                        .overflow(Overflow::Clip)
                        .border(
                            Border::new()
                                .width(1.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(theme.color(ThemeRole::BorderSubtle)),
                        )
                        .child(WindowShell::new(location, location_a11y_id, stage).interactive_window_controls(false)),
                );
        }

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .child(SettingsView::new(active_group, search_query))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
