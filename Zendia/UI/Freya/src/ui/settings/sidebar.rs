/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Left navigation sidebar of Zendia's native settings window.

use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::settings::controls::SettingsPalette;
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::settings::SettingsGroupId;

#[derive(Clone, PartialEq)]
pub struct SettingsSidebar {
    pub active_group: SettingsGroupId,
    pub searching: bool,
    pub on_select_group: EventHandler<SettingsGroupId>,
}

impl Component for SettingsSidebar {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let on_select = self.on_select_group.clone();
        let open_ladybird_core = core.clone();
        let about_core = core.clone();

        let divider = || {
            rect()
                .width(Size::fill())
                .height(Size::px(1.0))
                .margin(Gaps::new(6.0, 8.0, 6.0, 8.0))
                .background(palette.divider)
        };

        let nav_item = |group: SettingsGroupId| {
            let handler = on_select.clone();
            SettingsNavRow {
                label: group.label().to_string(),
                icon: group.icon().to_string(),
                active: !self.searching && self.active_group == group,
                trailing_icon: None,
                on_press: EventHandler::new(move |_| handler.call(group)),
                key: DiffKey::from(&group.as_str()),
            }
        };

        rect()
            .width(Size::px(248.0))
            .height(Size::fill())
            .background(palette.sidebar_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(palette.card_border),
            )
            .padding(Gaps::new(20.0, 12.0, 16.0, 12.0))
            .spacing(12.0)
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(0.0, 8.0, 6.0, 8.0))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(10.0)
                    .child(
                        rect()
                            .width(Size::px(26.0))
                            .height(Size::px(26.0))
                            .corner_radius(CornerRadius::new_all(8.0))
                            .background(theme.rgba(ThemeRole::Accent).with_alpha(0.18).to_color())
                            .center()
                            .child(Icon::new("settings").size(15.0).role(ThemeRole::Accent)),
                    )
                    .child(
                        label()
                            .text("Settings")
                            .font_size(19.0)
                            .font_weight(FontWeight::BOLD)
                            .color(theme.color(ThemeRole::Text)),
                    ),
            )
            .child(
                ScrollView::new().width(Size::fill()).height(Size::flex(1.0)).child(
                    rect()
                        .width(Size::fill())
                        .spacing(2.0)
                        .children(SettingsGroupId::ZEN_GROUPS.iter().copied().map(nav_item))
                        .child(divider())
                        .children(SettingsGroupId::ZENDIA_GROUPS.iter().copied().map(nav_item))
                        .child(divider())
                        .children(SettingsGroupId::MODULE_GROUPS.iter().copied().map(nav_item))
                        .child(divider())
                        .child(SettingsNavRow {
                            label: "Ladybird Settings".to_string(),
                            icon: "sliders".to_string(),
                            active: false,
                            trailing_icon: Some("open".to_string()),
                            on_press: EventHandler::new(move |_| {
                                open_ladybird_core.invoke_or_log(Command::TabNew {
                                    url: Some("about:settings".to_string()),
                                    after: None,
                                    activate: true,
                                });
                            }),
                            key: DiffKey::from(&"about-settings-link"),
                        })
                        .child(SettingsNavRow {
                            label: "About Zendia".to_string(),
                            icon: "info".to_string(),
                            active: false,
                            trailing_icon: None,
                            on_press: EventHandler::new(move |_| {
                                about_core.invoke_or_log(Command::AppAbout);
                            }),
                            key: DiffKey::from(&"about-zendia-link"),
                        }),
                ),
            )
    }
}

#[derive(Clone, PartialEq)]
struct SettingsNavRow {
    label: String,
    icon: String,
    active: bool,
    trailing_icon: Option<String>,
    on_press: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for SettingsNavRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SettingsNavRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_press = self.on_press.clone();

        let bg = if self.active {
            palette.selected_pill_bg
        } else if *hovered.read() {
            palette.row_hover_bg
        } else {
            Color::TRANSPARENT
        };

        let icon_role = if self.active {
            ThemeRole::Accent
        } else {
            ThemeRole::TextMuted
        };

        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::FocusRing))
        } else {
            Border::new()
        };

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label.clone())
            .width(Size::fill())
            .height(Size::px(34.0))
            .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(bg)
            .border(border)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |_| on_press.call(()))
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .spacing(10.0)
                    .child(Icon::new(self.icon.clone()).size(15.0).role(icon_role))
                    .child(
                        label()
                            .width(Size::flex(1.0))
                            .text(self.label.clone())
                            .font_size(13.0)
                            .font_weight(if self.active {
                                FontWeight::SEMI_BOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .color(theme.color(ThemeRole::Text))
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    ),
            )
            .maybe_child(
                self.trailing_icon
                    .as_ref()
                    .map(|ico| Icon::new(ico.clone()).size(12.5).role(ThemeRole::TextMuted)),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
