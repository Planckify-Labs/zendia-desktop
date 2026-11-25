/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Generic selectable picker list component shared by the floating address bar, command palette,
//! future Vim pickers, and declarative plugin UI.

use crate::ui::kit::icons::{Icon, prepared_icon_bytes};
use crate::ui::kit::key_label::KeyLabel;
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;

/// Data describing one selectable row in a [`PickerList`].
#[derive(Clone, Debug, PartialEq)]
pub struct PickerItem {
    /// Stable unique identifier used as the Freya component key.
    pub id: String,
    /// Primary row title.
    pub title: String,
    /// Optional secondary detail rendered inline after an em-dash (`title — subtitle`).
    pub subtitle: Option<String>,
    /// Optional icon name from `ICON_TABLE`.
    pub icon: Option<String>,
    /// Optional colored badge behind the icon (used for site/tab favicons).
    pub icon_badge: Option<(Color, Color)>,
    /// Optional shortcut rendered on the right with [`KeyLabel`].
    pub key_hint: Option<String>,
    /// Optional trailing action chip label (such as `"Switch to Tab"` with an arrow chip).
    pub action_chip: Option<String>,
    /// When true, highlighting this row uses the full accent fill (Zen heuristic row style).
    pub accent_highlight: bool,
}

impl PickerItem {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            icon: None,
            icon_badge: None,
            key_hint: None,
            action_chip: None,
            accent_highlight: false,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn icon_badge(mut self, bg: Color, fg: Color) -> Self {
        self.icon_badge = Some((bg, fg));
        self
    }

    pub fn key_hint(mut self, notation: impl Into<String>) -> Self {
        self.key_hint = Some(notation.into());
        self
    }

    pub fn action_chip(mut self, label: impl Into<String>) -> Self {
        self.action_chip = Some(label.into());
        self
    }

    pub fn accent_highlight(mut self, accent: bool) -> Self {
        self.accent_highlight = accent;
        self
    }
}

/// Scrollable list of [`PickerItem`] rows with keyboard highlight, pointer hover, and click selection.
#[derive(Clone, PartialEq)]
pub struct PickerList {
    items: Vec<PickerItem>,
    highlighted: Option<usize>,
    max_height: f32,
    empty_message: Option<String>,
    on_hover: Option<EventHandler<usize>>,
    on_select: Option<EventHandler<usize>>,
    key: DiffKey,
}

impl KeyExt for PickerList {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl PickerList {
    pub fn new(items: Vec<PickerItem>) -> Self {
        Self {
            items,
            highlighted: None,
            max_height: 272.0,
            empty_message: None,
            on_hover: None,
            on_select: None,
            key: DiffKey::None,
        }
    }

    pub fn highlighted(mut self, index: Option<usize>) -> Self {
        self.highlighted = index;
        self
    }

    pub fn max_height(mut self, max_height: f32) -> Self {
        self.max_height = max_height;
        self
    }

    pub fn empty_message(mut self, message: impl Into<String>) -> Self {
        self.empty_message = Some(message.into());
        self
    }

    pub fn on_hover(mut self, handler: impl Into<EventHandler<usize>>) -> Self {
        self.on_hover = Some(handler.into());
        self
    }

    pub fn on_select(mut self, handler: impl Into<EventHandler<usize>>) -> Self {
        self.on_select = Some(handler.into());
        self
    }
}

impl Component for PickerList {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();

        if self.items.is_empty() {
            let Some(message) = &self.empty_message else {
                return rect().width(Size::fill()).into_element();
            };
            return rect()
                .width(Size::fill())
                .padding(Gaps::new(22.0, 16.0, 22.0, 16.0))
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .spacing(6.0)
                .child(Icon::new("search-glass").size(18.0).role(ThemeRole::TextMuted))
                .child(
                    label()
                        .text(message.clone())
                        .font_size(13.5)
                        .color(theme.color(ThemeRole::TextMuted)),
                )
                .into_element();
        }

        let rows = self.items.iter().enumerate().map(|(index, item)| {
            PickerRow {
                item: item.clone(),
                index,
                highlighted: self.highlighted == Some(index),
                on_hover: self.on_hover.clone(),
                on_select: self.on_select.clone(),
                key: DiffKey::from(&item.id),
            }
            .into_element()
        });

        // Estimate exact height (44px row + 2px spacing + 16px vertical padding) so rows never clip mid-line.
        let estimated_height = self.items.len() as f32 * 46.0 + 14.0;
        let container_height = estimated_height.min(self.max_height);

        ScrollView::new()
            .width(Size::fill())
            .height(Size::px(container_height))
            .show_scrollbar(false)
            .child(
                rect()
                    .a11y_role(AccessibilityRole::ListBox)
                    .width(Size::fill())
                    .padding(Gaps::new(8.0, 10.0, 8.0, 10.0))
                    .spacing(2.0)
                    .children(rows),
            )
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// One keyed row inside [`PickerList`]. Kept as its own component so hover hooks stay unconditional per row.
#[derive(Clone, PartialEq)]
struct PickerRow {
    item: PickerItem,
    index: usize,
    highlighted: bool,
    on_hover: Option<EventHandler<usize>>,
    on_select: Option<EventHandler<usize>>,
    key: DiffKey,
}

impl KeyExt for PickerRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for PickerRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);

        let is_accent = self.highlighted && self.item.accent_highlight;
        let is_neutral_highlight = (self.highlighted && !self.item.accent_highlight) || hovered();

        let (bg, title_color, subtitle_color, icon_color) = if is_accent {
            let accent_bg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(86, 106, 160, 0.88).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(98, 146, 246, 0.96).to_color(),
            };
            (
                accent_bg,
                Color::WHITE,
                Color::from_argb(220, 255, 255, 255),
                Color::WHITE,
            )
        } else if is_neutral_highlight {
            let hover_bg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.07).to_color(),
            };
            let fg = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(235, 255, 255, 255),
                ResolvedScheme::Light => Color::from_argb(230, 20, 22, 30),
            };
            let muted = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(165, 255, 255, 255),
                ResolvedScheme::Light => Color::from_argb(150, 40, 44, 58),
            };
            (hover_bg, fg, muted, fg)
        } else {
            let fg = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(215, 255, 255, 255),
                ResolvedScheme::Light => Color::from_argb(215, 24, 26, 36),
            };
            let muted = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(150, 255, 255, 255),
                ResolvedScheme::Light => Color::from_argb(145, 50, 54, 68),
            };
            (Color::TRANSPARENT, fg, muted, muted)
        };

        let index = self.index;
        let on_hover = self.on_hover.clone();
        let on_select = self.on_select.clone();

        let leading_icon: Element = if let Some((badge_bg, badge_fg)) = self.item.icon_badge {
            let icon_name = self.item.icon.as_deref().unwrap_or("globe");
            let glyph: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(icon_name) {
                SvgViewer::new((("zendia-icon", canonical), bytes))
                    .width(Size::px(12.0))
                    .height(Size::px(12.0))
                    .color(badge_fg)
                    .show_loader(false)
                    .into_element()
            } else {
                Icon::new("globe").size(12.0).role(ThemeRole::Text).into_element()
            };
            rect()
                .width(Size::px(20.0))
                .height(Size::px(20.0))
                .corner_radius(CornerRadius::new_all(4.5))
                .background(badge_bg)
                .center()
                .child(glyph)
                .into_element()
        } else if let Some(icon_name) = &self.item.icon {
            let glyph: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(icon_name) {
                SvgViewer::new((("zendia-icon", canonical), bytes))
                    .width(Size::px(15.0))
                    .height(Size::px(15.0))
                    .color(icon_color)
                    .show_loader(false)
                    .into_element()
            } else {
                Icon::new(icon_name.clone())
                    .size(15.0)
                    .role(ThemeRole::TextMuted)
                    .into_element()
            };
            rect()
                .width(Size::px(20.0))
                .height(Size::px(20.0))
                .center()
                .child(glyph)
                .into_element()
        } else {
            rect().width(Size::px(20.0)).height(Size::px(20.0)).into_element()
        };

        let subtitle_el = self.item.subtitle.as_ref().map(|sub| {
            rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .spacing(6.0)
                .child(label().text("—").font_size(13.5).color(subtitle_color))
                .child(
                    label()
                        .text(sub.clone())
                        .font_size(13.5)
                        .font_weight(FontWeight::NORMAL)
                        .color(subtitle_color)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
        });

        let trailing_el: Option<Element> = if let Some(action_label) = &self.item.action_chip {
            let chip_bg = if is_accent {
                Color::from_argb(235, 255, 255, 255)
            } else if theme.scheme.is_dark() {
                Color::from_argb(220, 232, 234, 240)
            } else {
                Color::from_argb(215, 58, 62, 74)
            };
            let chip_fg = if is_accent {
                theme.color(ThemeRole::Accent)
            } else if theme.scheme.is_dark() {
                Color::from_rgb(28, 30, 38)
            } else {
                Color::WHITE
            };
            let arrow_el: Element = if let Some((canonical, bytes)) = prepared_icon_bytes("arrow-right") {
                SvgViewer::new((("zendia-icon", canonical), bytes))
                    .width(Size::px(12.0))
                    .height(Size::px(12.0))
                    .color(chip_fg)
                    .show_loader(false)
                    .into_element()
            } else {
                label().text("→").font_size(11.0).color(chip_fg).into_element()
            };
            Some(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .child(
                        label()
                            .text(action_label.clone())
                            .font_size(12.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(if is_accent { Color::WHITE } else { subtitle_color }),
                    )
                    .child(
                        rect()
                            .width(Size::px(22.0))
                            .height(Size::px(22.0))
                            .corner_radius(CornerRadius::new_all(5.0))
                            .background(chip_bg)
                            .center()
                            .child(arrow_el),
                    )
                    .into_element(),
            )
        } else {
            self.item.key_hint.as_ref().map(|shortcut| {
                KeyLabel::new(shortcut.clone())
                    .subtle(true)
                    .active(is_accent)
                    .into_element()
            })
        };

        let a11y_label = match &self.item.subtitle {
            Some(sub) => format!("{} — {sub}", self.item.title),
            None => self.item.title.clone(),
        };

        rect()
            .a11y_role(AccessibilityRole::ListItem)
            .a11y_alt(a11y_label)
            .width(Size::fill())
            .height(Size::px(44.0))
            .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
            .spacing(10.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(8.0))
            .background(bg)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| {
                hovered.set_if_modified(true);
                if let Some(handler) = &on_hover {
                    handler.call(index);
                }
            })
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                if let Some(handler) = &on_select {
                    handler.call(index);
                }
            })
            .child(leading_icon)
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .overflow(Overflow::Clip)
                    .child(
                        label()
                            .text(self.item.title.clone())
                            .font_size(14.0)
                            .font_weight(if self.highlighted || self.item.action_chip.is_some() {
                                FontWeight::SEMI_BOLD
                            } else {
                                FontWeight::MEDIUM
                            })
                            .color(title_color)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    )
                    .maybe_child(subtitle_el),
            )
            .maybe_child(trailing_el)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
