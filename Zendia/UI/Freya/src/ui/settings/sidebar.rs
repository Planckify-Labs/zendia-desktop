/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Left navigation sidebar of Zendia's native settings window.

use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, focus_ring_border, use_interactive_feedback};
use crate::ui::settings::controls::SettingsPalette;
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::settings::SettingsGroupId;

#[derive(Clone, PartialEq)]
pub struct SettingsSidebar {
    pub active_group: SettingsGroupId,
    pub searching: bool,
    pub compact: bool,
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
                .margin(Gaps::new(6.0, 12.0, 6.0, 12.0))
                .background(palette.divider)
        };

        let nav_item = |group: SettingsGroupId| {
            let handler = on_select.clone();
            let hint_badge = crate::ui::settings::settings_vim_hint_for_group(group);
            SettingsNavRow {
                label: group.label().to_string(),
                icon: group.icon().to_string(),
                active: !self.searching && self.active_group == group,
                trailing_icon: None,
                hint_badge,
                on_press: EventHandler::new(move |_| handler.call(group)),
                key: DiffKey::from(&group.as_str()),
            }
        };

        let sidebar_width = if self.compact { 200.0 } else { 248.0 };

        rect()
            .width(Size::px(sidebar_width))
            .height(Size::fill())
            .background(palette.sidebar_bg)
            .padding(Gaps::new(22.0, 10.0, 16.0, 14.0))
            .spacing(10.0)
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(0.0, 10.0, 6.0, 12.0))
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
                            hint_badge: crate::ui::settings::settings_vim_hint_for_link("ladybird"),
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
                            hint_badge: crate::ui::settings::settings_vim_hint_for_link("about"),
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
    hint_badge: Option<String>,
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
        let feedback = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_press = self.on_press.clone();

        let rest_bg = if self.active {
            palette.selected_pill_bg
        } else {
            Color::TRANSPARENT
        };
        let bg = feedback.background(rest_bg, palette.row_hover_bg, palette.selected_pill_bg);

        let icon_role = if self.active {
            ThemeRole::Accent
        } else {
            ThemeRole::TextMuted
        };

        let border = focus_ring_border(focus() == Focus::Keyboard, &theme, Border::new());

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label.clone())
            .width(Size::fill())
            .height(Size::px(40.0))
            .padding(Gaps::new(0.0, 12.0, 0.0, 4.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .border(border)
            .scale(feedback.scale())
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .on_press(move |_| on_press.call(()))
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if e.key == Key::Named(NamedKey::Enter) || matches!(&e.key, Key::Character(c) if c == " ") {
                    e.stop_propagation();
                }
            })
            .child(
                rect()
                    .width(Size::px(3.0))
                    .height(Size::px(20.0))
                    .corner_radius(CornerRadius::new_all(2.0))
                    .background(if self.active {
                        theme.color(ThemeRole::Accent)
                    } else {
                        Color::TRANSPARENT
                    }),
            )
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .spacing(10.0)
                    .child(Icon::new(self.icon.clone()).size(16.0).role(icon_role))
                    .child(
                        label()
                            .width(Size::flex(1.0))
                            .text(self.label.clone())
                            .font_size(13.5)
                            .font_weight(if self.active {
                                FontWeight::SEMI_BOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .color(if self.active {
                                theme.color(ThemeRole::Accent)
                            } else {
                                theme.color(ThemeRole::Text)
                            })
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    ),
            )
            .maybe_child(
                self.trailing_icon
                    .as_ref()
                    .map(|ico| Icon::new(ico.clone()).size(12.5).role(ThemeRole::TextMuted)),
            )
            .maybe_child(self.hint_badge.as_ref().map(|badge| {
                rect()
                    .height(Size::px(16.0))
                    .padding(Gaps::new(0.0, 5.0, 0.0, 5.0))
                    .corner_radius(CornerRadius::new_all(4.0))
                    .background(Color::from_rgb(0xa2, 0x57, 0x72))
                    .center()
                    .child(
                        label()
                            .text(badge.clone())
                            .font_size(11.0)
                            .font_weight(FontWeight::BOLD)
                            .font_family("monospace")
                            .color(Color::from_rgb(0xee, 0xf5, 0xff)),
                    )
            }))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
