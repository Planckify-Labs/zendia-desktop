/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! List row component with keyboard cursor highlight, selection marker, key hint, icon, and muted detail.

use crate::ui::kit::icons::Icon;
use crate::ui::kit::interactive::use_interactive_feedback;
use crate::ui::kit::key_label::KeyLabel;
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;

/// Row used in pickers, command palettes, sidebar lists, and declarative module lists.
#[derive(Clone, PartialEq)]
pub struct ListRow {
    label: String,
    detail: Option<String>,
    icon: Option<String>,
    key_hint: Option<String>,
    cursor: bool,
    selected: bool,
    disabled: bool,
    on_press: Option<EventHandler<Event<PressEventData>>>,
    key: DiffKey,
}

impl KeyExt for ListRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ListRow {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
            icon: None,
            key_hint: None,
            cursor: false,
            selected: false,
            disabled: false,
            on_press: None,
            key: DiffKey::None,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn key_hint(mut self, notation: impl Into<String>) -> Self {
        self.key_hint = Some(notation.into());
        self
    }

    /// Marks the row as the active keyboard cursor target.
    pub fn cursor(mut self, cursor: bool) -> Self {
        self.cursor = cursor;
        self
    }

    /// Marks the row as selected (e.g. multi-select or active option).
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_press(mut self, handler: impl Into<EventHandler<Event<PressEventData>>>) -> Self {
        self.on_press = Some(handler.into());
        self
    }
}

impl Component for ListRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let interactive = !self.disabled && self.on_press.is_some();
        let feedback = use_interactive_feedback(!self.disabled);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let rest_bg = if self.cursor {
            theme.color(ThemeRole::Selection)
        } else if self.selected {
            theme.rgba(ThemeRole::ModeVisual).with_alpha(0.22).to_color()
        } else {
            Color::TRANSPARENT
        };
        let hover_bg = if self.cursor || self.selected {
            rest_bg
        } else {
            theme.color(ThemeRole::Hover)
        };
        let pressed_bg = theme.rgba(ThemeRole::Text).with_alpha(0.14).to_color();
        let background = feedback.background(rest_bg, hover_bg, pressed_bg);

        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(2.0)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::FocusRing))
        } else if self.cursor {
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(theme.rgba(ThemeRole::Accent).with_alpha(0.75).to_color())
        } else if self.selected {
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(theme.rgba(ThemeRole::ModeVisual).with_alpha(0.60).to_color())
        } else {
            Border::new()
        };

        let text_role = if self.disabled {
            ThemeRole::TextMuted
        } else {
            ThemeRole::Text
        };
        let on_press = self.on_press.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_enabled(!self.disabled)
            .a11y_focusable(interactive)
            .a11y_role(AccessibilityRole::ListItem)
            .width(Size::fill())
            .padding(Gaps::new(
                theme.spacing.md,
                theme.spacing.lg,
                theme.spacing.md,
                theme.spacing.lg,
            ))
            .spacing(theme.spacing.lg)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(theme.radii.lg))
            .background(background)
            .border(border)
            .scale(if interactive { feedback.scale() } else { 1.0 })
            .opacity(if self.disabled { 0.55 } else { 1.0 })
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .maybe(interactive, |el| {
                el.cursor(CursorIcon::Pointer).on_press(move |e| {
                    a11y_id.request_focus();
                    if let Some(handler) = &on_press {
                        handler.call(e);
                    }
                })
            })
            .maybe_child(
                self.key_hint
                    .as_ref()
                    .map(|hint| KeyLabel::new(hint.clone()).active(self.cursor)),
            )
            .maybe_child(
                self.icon
                    .as_ref()
                    .map(|name| Icon::new(name.clone()).size(16.0).role(text_role)),
            )
            .child(
                label()
                    .text(self.label.clone())
                    .width(Size::flex(1.0))
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .font_size(theme.font_sizes.md)
                    .font_weight(if self.cursor || self.selected {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .color(theme.color(text_role)),
            )
            .maybe_child(self.detail.as_ref().map(|detail| {
                label()
                    .text(detail.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .font_size(theme.font_sizes.xs)
                    .color(theme.color(ThemeRole::TextMuted))
            }))
            .maybe_child(
                self.selected
                    .then(|| Icon::new("check").size(14.0).role(ThemeRole::Accent)),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
