/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Tab Management, Spaces, and Glance settings groups.

use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, Pill, use_interactive_feedback};
use crate::ui::settings::controls::{
    SettingsButton, SettingsCheckboxRow, SettingsDropdownRow, SettingsPalette, section_heading,
    section_heading_with_action, settings_card, settings_group_section, subsection_heading,
};
use crate::ui::theme::{Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::library::LibrarySection;
use zendia_browser_core::model::Space;
use zendia_browser_core::settings::{GlanceTrigger, PinnedTabCloseBehavior, SettingsGroupId};
use zendia_module_api::ui::UiTone;

/// Tab Management group (Workspaces tab behavior + Pinned Tabs & Essentials), matching `settings-tab-management-*.png`.
#[derive(Clone, PartialEq)]
pub struct TabManagementGroup;

impl Component for TabManagementGroup {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let settings = core.model().read().settings.clone();

        let toggle_cmd = |key: &'static str| {
            let core = core.clone();
            move |next: bool| {
                core.invoke_or_log(Command::SettingsSet {
                    key: key.to_string(),
                    value: next.to_string(),
                });
            }
        };

        let close_core = core.clone();
        let reset_core = core.clone();
        let close_options: Vec<(String, String)> = PinnedTabCloseBehavior::ALL
            .iter()
            .map(|opt| (opt.as_str().to_string(), opt.label().to_string()))
            .collect();

        let mut workspace_items: Vec<Element> = vec![
            subsection_heading("General settings for workspaces", None),
            SettingsCheckboxRow::new(
                "Sync only pinned tabs in workspaces",
                settings.sync_only_pinned_tabs,
                toggle_cmd("workspaces.sync_only_pinned_tabs"),
            )
            .key("ws-sync-pinned")
            .into_element(),
            SettingsCheckboxRow::new(
                "Hide the default container indicator in the tab bar",
                settings.hide_default_container_indicator,
                toggle_cmd("workspaces.hide_default_container_indicator"),
            )
            .key("ws-hide-container")
            .into_element(),
            SettingsCheckboxRow::new(
                "Switch to workspace where container is set as default when opening container tabs",
                settings.force_container_workspace,
                toggle_cmd("workspaces.force_container_workspace"),
            )
            .key("ws-force-container")
            .into_element(),
            SettingsCheckboxRow::new(
                "Close tab and switch to its owner tab (or most recently used tab) when going back with no history",
                settings.close_on_back_with_no_history,
                toggle_cmd("tabs.close_on_back_with_no_history"),
            )
            .key("tabs-close-back")
            .into_element(),
            SettingsCheckboxRow::new(
                "Ignore Pending tabs when cycling with Ctrl+Tab",
                settings.ctrl_tab_ignore_pending_tabs,
                toggle_cmd("tabs.ctrl_tab_ignore_pending"),
            )
            .key("tabs-ignore-pending")
            .into_element(),
            SettingsCheckboxRow::new(
                "Ctrl+Tab cycles within Essential or Workspace tabs only",
                settings.ctrl_tab_cycle_by_attribute,
                toggle_cmd("tabs.ctrl_tab_cycle_by_attribute"),
            )
            .key("tabs-cycle-attr")
            .into_element(),
        ];

        if settings.ctrl_tab_cycle_by_attribute {
            workspace_items.push(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(6.0, 10.0, 6.0, 10.0))
                    .margin(Gaps::new(0.0, 0.0, 2.0, 28.0))
                    .corner_radius(CornerRadius::new_all(6.0))
                    .background(theme.rgba(ThemeRole::Warning).with_alpha(0.12).to_color())
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.rgba(ThemeRole::Warning).with_alpha(0.35).to_color()),
                    )
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .child(Icon::new("warning").size(14.0).role(ThemeRole::Warning))
                    .child(
                        label()
                            .width(Size::flex(1.0))
                            .text("Ctrl+Tab will only cycle through tabs in the same group (Essentials or current Workspace).")
                            .font_size(12.0)
                            .color(theme.color(ThemeRole::Text)),
                    )
                    .into_element(),
            );
        }

        workspace_items.push(
            SettingsCheckboxRow::new(
                "When closing a tab, switch to the most recently used tab instead of the next tab",
                settings.select_recently_used_on_close,
                toggle_cmd("tabs.select_recently_used_on_close"),
            )
            .key("tabs-mru-close")
            .into_element(),
        );

        rect()
            .width(Size::fill())
            .spacing(22.0)
            .child(section_heading_with_action(
                "Workspaces",
                Some("With workspaces, you can have multiple browsing sessions at once!"),
                Some(
                    SettingsButton::new("Reset defaults", move |_| {
                        reset_core.invoke_or_log(Command::SettingsResetGroup {
                            group: SettingsGroupId::TabManagement,
                        });
                    })
                    .icon("reload")
                    .key("reset-tab-mgmt")
                    .into_element(),
                ),
            ))
            .child(settings_group_section(workspace_items))
            .child(section_heading(
                "Pinned Tabs",
                Some("Manage additional behavior of pinned tabs and essentials."),
            ))
            .child(settings_group_section(vec![
                subsection_heading("General settings for pinned tabs", None),
                SettingsCheckboxRow::new(
                    "Restore pinned tabs to their originally pinned URL on startup",
                    settings.restore_pinned_tabs_on_startup,
                    toggle_cmd("pinned.restore_url_on_startup"),
                )
                .key("pinned-restore")
                .into_element(),
                SettingsCheckboxRow::new(
                    "Enable container-specific essentials",
                    settings.separate_essentials_per_container,
                    toggle_cmd("pinned.separate_essentials"),
                )
                .key("pinned-essentials")
                .into_element(),
                SettingsDropdownRow::new(
                    "Close Tab Shortcut Behavior",
                    settings.pinned_tab_close_behavior.as_str(),
                    close_options,
                    move |val: String| {
                        close_core.invoke_or_log(Command::SettingsSet {
                            key: "pinned.close_shortcut_behavior".to_string(),
                            value: val,
                        });
                    },
                )
                .description("Choose what happens when you press Ctrl+W on a pinned tab.")
                .key("pinned-close-behavior")
                .into_element(),
            ]))
    }
}

/// Spaces group: session continuity, active spaces overview, and a button that opens the space creation form.
#[derive(Clone, PartialEq)]
pub struct SpacesGroup;

impl Component for SpacesGroup {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let (spaces, active_space, continue_where_left_off, routing_rules_count) = {
            let model = core.model().read();
            (
                model.spaces.clone(),
                model.active_space,
                model.settings.continue_where_left_off,
                model.space_routing.rules.len(),
            )
        };

        let core_continue = core.clone();
        let core_create = core.clone();
        let core_routing = core.clone();

        let space_rows: Vec<Element> = spaces
            .iter()
            .map(|space| {
                let sw_core = core.clone();
                let sid = space.id;
                SpaceSummaryRow {
                    space: space.clone(),
                    active: active_space == Some(space.id),
                    on_switch: EventHandler::new(move |_| {
                        sw_core.invoke_or_log(Command::SpaceSwitch { space_id: sid });
                    }),
                    key: DiffKey::from(&space.id.0),
                }
                .into_element()
            })
            .collect();

        rect()
            .width(Size::fill())
            .spacing(22.0)
            .child(section_heading(
                "Spaces",
                Some("Organize tabs, pinned items, and themes into separate browsing spaces."),
            ))
            .child(settings_group_section(vec![
                subsection_heading("Startup & Spaces", None),
                SettingsCheckboxRow::new(
                    "Continue where you left off",
                    continue_where_left_off,
                    move |next: bool| {
                        core_continue.invoke_or_log(Command::SettingsSet {
                            key: "spaces.continue_where_left_off".to_string(),
                            value: next.to_string(),
                        });
                    },
                )
                .description("Restore open spaces and tabs from your previous session when Zendia starts.")
                .key("spaces-continue")
                .into_element(),
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(4.0, 8.0, 2.0, 8.0))
                    .direction(Direction::Horizontal)
                    .spacing(10.0)
                    .child(
                        SettingsButton::new("Create New Space…", move |_| {
                            core_create.invoke_or_log(Command::SpaceCreateFormOpen);
                        })
                        .icon("plus")
                        .primary(true)
                        .key("create-space-btn"),
                    )
                    .child(
                        SettingsButton::new(format!("Space Routing ({routing_rules_count} rules)"), move |_| {
                            core_routing.invoke_or_log(Command::LibraryOpen {
                                section: Some(LibrarySection::SpaceRouting),
                            });
                        })
                        .icon("library-space-routing")
                        .key("space-routing-btn"),
                    )
                    .into_element(),
            ]))
            .child(settings_card(vec![
                subsection_heading("Your Spaces", Some("Switch spaces or manage them in the Library.")),
                rect().width(Size::fill()).spacing(6.0).children(space_rows).into_element(),
            ]))
            .child(
                label()
                    .text("Tip: Edit space gradients and icons from the sidebar space header or the Library's Spaces section.")
                    .font_size(theme.font_sizes.xs)
                    .color(theme.color(ThemeRole::TextMuted)),
            )
    }
}

#[derive(Clone, PartialEq)]
struct SpaceSummaryRow {
    space: Space,
    active: bool,
    on_switch: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for SpaceSummaryRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SpaceSummaryRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let feedback = use_interactive_feedback(true);
        let on_switch = self.on_switch.clone();
        let dot_color = self
            .space
            .gradient
            .stops
            .first()
            .map(|(r, g, b)| Rgba::rgb(*r, *g, *b).to_color())
            .unwrap_or_else(|| theme.color(ThemeRole::Accent));

        rect()
            .width(Size::fill())
            .padding(Gaps::new(8.0, 10.0, 8.0, 10.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(feedback.background(Color::TRANSPARENT, palette.row_hover_bg, palette.row_hover_bg))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .spacing(10.0)
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(10.0)
                    .child(
                        rect()
                            .width(Size::px(12.0))
                            .height(Size::px(12.0))
                            .corner_radius(CornerRadius::new_all(999.0))
                            .background(dot_color),
                    )
                    .child(
                        Icon::new(if self.space.icon.is_empty() {
                            "library-spaces".to_string()
                        } else {
                            self.space.icon.clone()
                        })
                        .size(15.0)
                        .role(ThemeRole::Text),
                    )
                    .child(
                        label()
                            .text(self.space.name.clone())
                            .font_size(13.0)
                            .font_weight(FontWeight::MEDIUM)
                            .color(theme.color(ThemeRole::Text)),
                    )
                    .maybe_child(self.active.then(|| Pill::new("Active").tone(UiTone::Accent))),
            )
            .maybe_child(
                (!self.active).then(|| SettingsButton::new("Switch", move |_| on_switch.call(())).key("sw-btn")),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Glance group: enable toggle and link-click trigger modifier.
#[derive(Clone, PartialEq)]
pub struct GlanceGroup;

impl Component for GlanceGroup {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let settings = core.model().read().settings.clone();

        let core_enable = core.clone();
        let core_trigger = core.clone();
        let core_reset = core.clone();
        let trigger_options: Vec<(String, String)> = GlanceTrigger::ALL
            .iter()
            .map(|t| (t.as_str().to_string(), t.label().to_string()))
            .collect();

        rect()
            .width(Size::fill())
            .spacing(22.0)
            .child(section_heading_with_action(
                "Glance",
                Some("Get a quick overview of your links without opening them in a new tab."),
                Some(
                    SettingsButton::new("Reset defaults", move |_| {
                        core_reset.invoke_or_log(Command::SettingsResetGroup {
                            group: SettingsGroupId::Glance,
                        });
                    })
                    .icon("reload")
                    .key("reset-glance")
                    .into_element(),
                ),
            ))
            .child(settings_group_section(vec![
                subsection_heading("General settings for Glance", None),
                SettingsCheckboxRow::new("Enable Glance", settings.glance_enabled, move |next: bool| {
                    core_enable.invoke_or_log(Command::SettingsSet {
                        key: "glance.enabled".to_string(),
                        value: next.to_string(),
                    });
                })
                .description("Preview clicked links in a centered card over the current page.")
                .key("glance-enabled")
                .into_element(),
                SettingsDropdownRow::new(
                    "Trigger method",
                    settings.glance_trigger.as_str(),
                    trigger_options,
                    move |val: String| {
                        core_trigger.invoke_or_log(Command::SettingsSet {
                            key: "glance.trigger".to_string(),
                            value: val,
                        });
                    },
                )
                .description("Hold this modifier key while clicking a link to preview it in Glance.")
                .key("glance-trigger")
                .into_element(),
            ]))
    }
}
