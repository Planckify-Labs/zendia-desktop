/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Sidebar notification card at the foot of the expanded sidebar (Zen's `ZenSidebarNotification`).

use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{NotificationId, SidebarNotification, SidebarNotificationLink};

/// Renders a core-published [`SidebarNotification`] card above the sidebar foot row.
#[derive(Clone, PartialEq)]
pub struct SidebarNotificationCard {
    pub notification: SidebarNotification,
    key: DiffKey,
}

impl KeyExt for SidebarNotificationCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl SidebarNotificationCard {
    pub fn new(notification: SidebarNotification) -> Self {
        let key = DiffKey::from(&notification.id.0);
        Self { notification, key }
    }
}

impl Component for SidebarNotificationCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut close_hovered = use_state(|| false);
        let close_a11y = use_a11y();

        let notification_id = self.notification.id;
        let (card_bg, card_border, header_divider, heading_color, shadow) = match theme.scheme {
            ResolvedScheme::Dark => (
                Rgba::from_rgb_alpha(12, 13, 18, 0.96).to_color(),
                Rgba::from_rgb_alpha(255, 255, 255, 0.14).to_color(),
                Rgba::from_rgb_alpha(255, 255, 255, 0.10).to_color(),
                Rgba::from_rgb_alpha(255, 255, 255, 0.92).to_color(),
                Shadow::new()
                    .x(0.0)
                    .y(4.0)
                    .blur(12.0)
                    .color(Color::from_argb(96, 0, 0, 0)),
            ),
            ResolvedScheme::Light => (
                Rgba::from_rgb_alpha(250, 250, 253, 0.96).to_color(),
                Rgba::from_rgb_alpha(0, 0, 0, 0.10).to_color(),
                Rgba::from_rgb_alpha(0, 0, 0, 0.09).to_color(),
                Rgba::from_rgb_alpha(20, 22, 32, 0.92).to_color(),
                Shadow::new()
                    .x(0.0)
                    .y(3.0)
                    .blur(10.0)
                    .color(Color::from_argb(36, 0, 0, 0)),
            ),
        };

        let dismiss_core = core.clone();
        let on_dismiss = move |_: Event<PressEventData>| {
            dismiss_core.invoke_or_log(Command::NotificationDismiss { notification_id });
        };

        let links = self.notification.links.iter().enumerate().map(|(idx, link)| {
            NotificationLinkRow {
                notification_id,
                link: link.clone(),
                key: DiffKey::from(&idx),
            }
            .into_element()
        });

        rect()
            .a11y_role(AccessibilityRole::Alert)
            .a11y_alt(self.notification.heading.clone())
            .width(Size::fill())
            .corner_radius(CornerRadius::new_all(10.0))
            .background(card_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(card_border),
            )
            .shadow(shadow)
            .overflow(Overflow::Clip)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(34.0))
                    .padding(Gaps::new(0.0, 6.0, 0.0, 12.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(Color::TRANSPARENT),
                    )
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .main_align(Alignment::center())
                            .cross_align(Alignment::center())
                            .child(
                                label()
                                    .text(self.notification.heading.clone())
                                    .font_size(12.0)
                                    .font_weight(FontWeight::MEDIUM)
                                    .color(heading_color)
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis),
                            ),
                    )
                    .child(
                        rect()
                            .a11y_id(close_a11y)
                            .a11y_focusable(true)
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Dismiss notification")
                            .width(Size::px(20.0))
                            .height(Size::px(20.0))
                            .corner_radius(CornerRadius::new_all(5.0))
                            .background(if close_hovered() {
                                Rgba::from_rgb_alpha(128, 128, 140, 0.22).to_color()
                            } else {
                                Color::TRANSPARENT
                            })
                            .opacity(if close_hovered() { 0.95 } else { 0.5 })
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_pointer_enter(move |_| close_hovered.set_if_modified(true))
                            .on_pointer_leave(move |_| close_hovered.set_if_modified(false))
                            .on_press(on_dismiss)
                            .child(Icon::new("close").size(11.0).role(ThemeRole::Text)),
                    ),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(1.0))
                    .background(header_divider),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(6.0, 6.0, 6.0, 6.0))
                    .spacing(2.0)
                    .children(links),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct NotificationLinkRow {
    notification_id: NotificationId,
    link: SidebarNotificationLink,
    key: DiffKey,
}

impl KeyExt for NotificationLinkRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for NotificationLinkRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();

        let notification_id = self.notification_id;
        let url = self.link.url.clone();
        let command = self.link.command.clone();
        let special = self.link.special;

        let text_color = if special {
            theme.color(ThemeRole::Accent)
        } else if hovered() {
            theme.color(ThemeRole::Text)
        } else {
            theme.color(ThemeRole::TextMuted)
        };
        let icon_role = if special {
            ThemeRole::Accent
        } else if hovered() {
            ThemeRole::Text
        } else {
            ThemeRole::TextMuted
        };

        let hover_bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.05).to_color(),
        };

        let on_press = move |_: Event<PressEventData>| {
            if let Some(cmd_name) = &command
                && let Ok(parsed) = Command::from_name_and_args(cmd_name, &[])
            {
                core.invoke_or_log(parsed);
            } else if let Some(url) = &url {
                core.invoke_or_log(Command::TabNew {
                    url: Some(url.clone()),
                    after: None,
                    activate: true,
                });
            }
            core.invoke_or_log(Command::NotificationDismiss { notification_id });
        };

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.link.label.clone())
            .width(Size::fill())
            .padding(Gaps::new(5.0, 6.0, 5.0, 6.0))
            .spacing(7.0)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(6.0))
            .background(if hovered() { hover_bg } else { Color::TRANSPARENT })
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(on_press)
            .child(Icon::new(self.link.icon.clone()).size(13.0).role(icon_role))
            .child(
                label()
                    .text(self.link.label.clone())
                    .font_size(11.5)
                    .font_weight(if special {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .color(text_color)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
