/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Live settings search results view (searching by title, description, section, and keywords).

use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, Pill};
use crate::ui::settings::controls::{
    SettingsButton, SettingsChoiceRow, SettingsPalette, SettingsToggleRow, section_heading,
};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::settings::{
    SettingControl, SettingEntry, SettingsGroupId, command_for_setting_change, search_settings, setting_bool_value,
    setting_choice_value,
};
use zendia_module_api::ui::UiTone;

#[derive(Clone, PartialEq)]
pub struct SettingsSearchResults {
    pub query: String,
    pub on_clear: EventHandler<()>,
    pub on_jump_group: EventHandler<SettingsGroupId>,
}

impl Component for SettingsSearchResults {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let results = search_settings(&self.query);
        let on_clear = self.on_clear.clone();
        let on_jump = self.on_jump_group.clone();

        if results.is_empty() {
            return rect()
                .width(Size::fill())
                .spacing(16.0)
                .child(section_heading("Search Results", None))
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(36.0, 24.0, 36.0, 24.0))
                        .corner_radius(CornerRadius::new_all(12.0))
                        .background(palette.card_bg)
                        .border(
                            Border::new()
                                .width(1.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(palette.card_border),
                        )
                        .center()
                        .spacing(10.0)
                        .child(Icon::new("search-glass").size(28.0).role(ThemeRole::TextMuted))
                        .child(
                            label()
                                .text(format!("No settings match \"{}\"", self.query.trim()))
                                .font_size(15.0)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .color(theme.color(ThemeRole::Text)),
                        )
                        .child(
                            label()
                                .text("Try searching for \"layout\", \"compact\", \"pinned\", \"glance\", \"vim\", or \"plugins\".")
                                .font_size(12.5)
                                .color(theme.color(ThemeRole::TextMuted)),
                        )
                        .child(SettingsButton::new("Clear Search", move |_| on_clear.call(())).key("clear-search-btn")),
                );
        }

        let count_text = if results.len() == 1 {
            format!("1 setting matches \"{}\"", self.query.trim())
        } else {
            format!("{} settings match \"{}\"", results.len(), self.query.trim())
        };

        let cards: Vec<Element> = results
            .into_iter()
            .map(|entry| {
                let jump = on_jump.clone();
                let key_id = entry.key.clone();
                SearchResultCard {
                    entry,
                    on_jump_group: jump,
                    key: DiffKey::from(&key_id),
                }
                .into_element()
            })
            .collect();

        rect()
            .width(Size::fill())
            .spacing(16.0)
            .child(section_heading("Search Results", Some(&count_text)))
            .child(rect().width(Size::fill()).spacing(10.0).children(cards))
    }
}

#[derive(Clone, PartialEq)]
struct SearchResultCard {
    entry: SettingEntry,
    on_jump_group: EventHandler<SettingsGroupId>,
    key: DiffKey,
}

impl KeyExt for SearchResultCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SearchResultCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let model = core.model().read().clone();
        let group = self.entry.group;
        let on_jump = self.on_jump_group.clone();

        let control_el: Element = match &self.entry.control {
            SettingControl::Toggle => {
                let checked = setting_bool_value(&model, &self.entry.key).unwrap_or(false);
                let key = self.entry.key.clone();
                let core_set = core.clone();
                SettingsToggleRow::new(self.entry.title.clone(), checked, move |next| {
                    if let Some(cmd) = command_for_setting_change(&key, if next { "true" } else { "false" }) {
                        core_set.invoke_or_log(cmd);
                    }
                })
                .description(self.entry.description.clone())
                .key(format!("sr-toggle-{}", self.entry.key))
                .into_element()
            }
            SettingControl::Choice(options) => {
                let current = setting_choice_value(&model, &self.entry.key).unwrap_or_default();
                let opts: Vec<(String, String)> = options.iter().map(|o| (o.value.clone(), o.label.clone())).collect();
                let key = self.entry.key.clone();
                let core_set = core.clone();
                SettingsChoiceRow::new(self.entry.title.clone(), current, opts, move |val: String| {
                    if let Some(cmd) = command_for_setting_change(&key, &val) {
                        core_set.invoke_or_log(cmd);
                    }
                })
                .description(self.entry.description.clone())
                .key(format!("sr-choice-{}", self.entry.key))
                .into_element()
            }
            SettingControl::Action { button_label } => {
                let key = self.entry.key.clone();
                let core_act = core.clone();
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(4.0, 8.0, 4.0, 8.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .main_align(Alignment::SpaceBetween)
                    .cross_align(Alignment::center())
                    .spacing(12.0)
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .spacing(2.0)
                            .child(
                                label()
                                    .text(self.entry.title.clone())
                                    .font_size(13.0)
                                    .font_weight(FontWeight::MEDIUM)
                                    .color(theme.color(ThemeRole::Text)),
                            )
                            .child(
                                label()
                                    .text(self.entry.description.clone())
                                    .font_size(11.5)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                    )
                    .child(
                        SettingsButton::new(button_label.clone(), move |_| {
                            if let Some(cmd) = command_for_setting_change(&key, "") {
                                core_act.invoke_or_log(cmd);
                            }
                        })
                        .primary(true)
                        .key(format!("sr-act-{}", self.entry.key)),
                    )
                    .into_element()
            }
        };

        rect()
            .width(Size::fill())
            .padding(Gaps::new(10.0, 12.0, 10.0, 12.0))
            .spacing(6.0)
            .corner_radius(CornerRadius::new_all(10.0))
            .background(palette.card_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(palette.card_border),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::SpaceBetween)
                    .cross_align(Alignment::center())
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(6.0)
                            .child(Pill::new(group.label()).icon(group.icon()).tone(UiTone::Accent))
                            .child(
                                label()
                                    .text(self.entry.section.clone())
                                    .font_size(11.5)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                    )
                    .child(
                        SettingsButton::new(format!("Open {}", group.label()), move |_| on_jump.call(group))
                            .key(format!("sr-jump-{}", self.entry.key)),
                    ),
            )
            .child(control_el)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
