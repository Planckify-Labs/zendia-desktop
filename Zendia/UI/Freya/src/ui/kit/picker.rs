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
    /// Optional section header shown above the first item of a contiguous run sharing it.
    pub section: Option<String>,
    /// Optional right-aligned muted detail (used in Zendia's list picker and grid tiles).
    pub detail: Option<String>,
    /// Optional leading jump key badge (`a`, `s`, `d`, ...).
    pub key_badge: Option<String>,
    /// Optional custom raw SVG bytes `(cache_key, bytes)` rendered in place of an `ICON_TABLE` lookup.
    pub custom_icon_svg: Option<(String, Bytes)>,
    /// Optional brand accent color used for selected grid tile glow.
    pub accent_color: Option<Color>,
    /// Visual dimmed appearance (for example, an uninstalled agent tile).
    pub dimmed: bool,
    /// Disables pointer activation when true.
    pub disabled: bool,
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
            section: None,
            detail: None,
            key_badge: None,
            custom_icon_svg: None,
            accent_color: None,
            dimmed: false,
            disabled: false,
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

    pub fn section(mut self, section: impl Into<String>) -> Self {
        self.section = Some(section.into());
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn key_badge(mut self, key: impl Into<String>) -> Self {
        self.key_badge = Some(key.into());
        self
    }

    pub fn custom_icon_svg(mut self, cache_key: impl Into<String>, svg: Bytes) -> Self {
        self.custom_icon_svg = Some((cache_key.into(), svg));
        self
    }

    pub fn accent_color(mut self, color: Color) -> Self {
        self.accent_color = Some(color);
        self
    }

    pub fn dimmed(mut self, dimmed: bool) -> Self {
        self.dimmed = dimmed;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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
    vim_style: bool,
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
            vim_style: false,
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

    /// Uses Zendia's compact Vim picker row geometry (section headings, leading key badges, right-aligned details).
    pub fn vim_style(mut self, enabled: bool) -> Self {
        self.vim_style = enabled;
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

        if self.vim_style {
            let mut section_count = 0usize;
            let mut children: Vec<Element> = Vec::with_capacity(self.items.len() + 4);
            let mut previous_section: Option<&str> = None;

            for (index, item) in self.items.iter().enumerate() {
                if let Some(section) = item.section.as_deref()
                    && previous_section != Some(section)
                {
                    let top_pad = if index == 0 { 2.0 } else { 8.0 };
                    let section_color = if theme.scheme.is_dark() {
                        Color::from_argb(198, 251, 251, 254)
                    } else {
                        Color::from_argb(198, 21, 20, 26)
                    };
                    children.push(
                        rect()
                            .key(format!("sec.{section_count}.{section}"))
                            .width(Size::fill())
                            .padding(Gaps::new(top_pad, 10.0, 3.0, 10.0))
                            .child(
                                label()
                                    .text(section.to_string())
                                    .font_size(11.0)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(section_color)
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis),
                            )
                            .into_element(),
                    );
                    section_count += 1;
                    previous_section = Some(section);
                }

                children.push(
                    VimPickerRow {
                        item: item.clone(),
                        index,
                        highlighted: self.highlighted == Some(index),
                        on_hover: self.on_hover.clone(),
                        on_select: self.on_select.clone(),
                        key: DiffKey::from(&item.id),
                    }
                    .into_element(),
                );
            }

            let estimated_height = self.items.len() as f32 * 38.0 + section_count as f32 * 24.0 + 6.0;
            let container_height = estimated_height.min(self.max_height);

            return ScrollView::new()
                .width(Size::fill())
                .height(Size::px(container_height))
                .show_scrollbar(false)
                .child(
                    rect()
                        .a11y_role(AccessibilityRole::ListBox)
                        .width(Size::fill())
                        .spacing(2.0)
                        .children(children),
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

/// Zendia-styled Vim picker row: leading key badge, icon, title, and right-aligned muted detail.
#[derive(Clone, PartialEq)]
struct VimPickerRow {
    item: PickerItem,
    index: usize,
    highlighted: bool,
    on_hover: Option<EventHandler<usize>>,
    on_select: Option<EventHandler<usize>>,
    key: DiffKey,
}

impl KeyExt for VimPickerRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for VimPickerRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);

        let is_selected = self.highlighted;
        let is_hovered = hovered() && !is_selected;

        let (bg, border_color) = if is_selected {
            let accent_bg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.08)
                    .mix(theme.palette.purple, 0.14)
                    .with_alpha(0.24)
                    .to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(152, 146, 212, 0.42).to_color(),
            };
            let top_border = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(36, 255, 255, 255),
                ResolvedScheme::Light => Color::from_argb(110, 255, 255, 255),
            };
            (accent_bg, top_border)
        } else if is_hovered {
            let hover_bg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.07).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.05).to_color(),
            };
            (hover_bg, Color::TRANSPARENT)
        } else {
            (Color::TRANSPARENT, Color::TRANSPARENT)
        };

        let text_alpha = if self.item.dimmed { 0.52 } else { 0.94 };
        let detail_alpha = if self.item.dimmed { 0.44 } else { 0.70 };
        let fg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(251, 251, 254, text_alpha).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(21, 20, 26, text_alpha).to_color(),
        };
        let detail_fg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(251, 251, 254, detail_alpha).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(21, 20, 26, detail_alpha).to_color(),
        };

        let icon_el: Option<Element> = if let Some((cache_key, bytes)) = &self.item.custom_icon_svg {
            Some(
                SvgViewer::new((cache_key.clone(), bytes.clone()))
                    .width(Size::px(16.0))
                    .height(Size::px(16.0))
                    .show_loader(false)
                    .into_element(),
            )
        } else if let Some((badge_bg, badge_fg)) = self.item.icon_badge {
            let icon_name = self.item.icon.as_deref().unwrap_or("globe");
            let glyph: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(icon_name) {
                SvgViewer::new((("zendia-icon", canonical), bytes))
                    .width(Size::px(11.0))
                    .height(Size::px(11.0))
                    .color(badge_fg)
                    .show_loader(false)
                    .into_element()
            } else {
                Icon::new("globe").size(11.0).role(ThemeRole::Text).into_element()
            };
            Some(
                rect()
                    .width(Size::px(16.0))
                    .height(Size::px(16.0))
                    .corner_radius(CornerRadius::new_all(4.0))
                    .background(badge_bg)
                    .center()
                    .child(glyph)
                    .into_element(),
            )
        } else if let Some(icon_name) = &self.item.icon {
            let glyph: Element = if let Some((canonical, bytes)) = prepared_icon_bytes(icon_name) {
                SvgViewer::new((("zendia-icon", canonical), bytes))
                    .width(Size::px(16.0))
                    .height(Size::px(16.0))
                    .color(fg)
                    .show_loader(false)
                    .into_element()
            } else {
                Icon::new(icon_name.clone())
                    .size(16.0)
                    .role(ThemeRole::Text)
                    .into_element()
            };
            Some(glyph)
        } else {
            None
        };

        let index = self.index;
        let disabled = self.item.disabled;
        let on_hover = self.on_hover.clone();
        let on_select = self.on_select.clone();
        let a11y_label = match &self.item.detail {
            Some(detail) => format!("{} ({detail})", self.item.title),
            None => self.item.title.clone(),
        };

        rect()
            .a11y_role(AccessibilityRole::ListItem)
            .a11y_alt(a11y_label)
            .width(Size::fill())
            .height(Size::px(36.0))
            .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
            .spacing(10.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(11.0))
            .background(bg)
            .border(
                Border::new()
                    .width(if is_selected { 1.0 } else { 0.0 })
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .cursor(if disabled {
                CursorIcon::Default
            } else {
                CursorIcon::Pointer
            })
            .on_pointer_enter(move |_| {
                hovered.set_if_modified(true);
                if let Some(handler) = &on_hover {
                    handler.call(index);
                }
            })
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                if !disabled && let Some(handler) = &on_select {
                    handler.call(index);
                }
            })
            .maybe_child(
                self.item
                    .key_badge
                    .as_ref()
                    .map(|badge| KeyLabel::new(badge.clone()).active(is_selected)),
            )
            .maybe_child(icon_el)
            .child(
                rect().width(Size::flex(1.0)).overflow(Overflow::Clip).child(
                    label()
                        .text(self.item.title.clone())
                        .font_size(13.0)
                        .font_weight(if is_selected {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        })
                        .color(fg)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                ),
            )
            .maybe_child(self.item.detail.as_ref().map(|detail| {
                rect().max_width(Size::percent(44.0)).overflow(Overflow::Clip).child(
                    label()
                        .text(detail.clone())
                        .font_size(11.0)
                        .color(detail_fg)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
            }))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Multi-column icon tile grid used by Zendia's grid-layout picker (such as the Agents picker).
#[derive(Clone, PartialEq)]
pub struct PickerGrid {
    items: Vec<PickerItem>,
    columns: usize,
    highlighted: Option<usize>,
    max_height: f32,
    empty_message: Option<String>,
    on_hover: Option<EventHandler<usize>>,
    on_select: Option<EventHandler<usize>>,
    key: DiffKey,
}

impl KeyExt for PickerGrid {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl PickerGrid {
    pub fn new(items: Vec<PickerItem>) -> Self {
        Self {
            items,
            columns: 3,
            highlighted: None,
            max_height: 360.0,
            empty_message: None,
            on_hover: None,
            on_select: None,
            key: DiffKey::None,
        }
    }

    pub fn columns(mut self, columns: usize) -> Self {
        self.columns = columns.max(1);
        self
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

impl Component for PickerGrid {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();

        if self.items.is_empty() {
            let Some(message) = &self.empty_message else {
                return rect().width(Size::fill()).into_element();
            };
            return rect()
                .width(Size::fill())
                .padding(Gaps::new(26.0, 16.0, 26.0, 16.0))
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

        let cols = self.columns.max(1);
        let row_count = self.items.len().div_ceil(cols);
        let mut rows: Vec<Element> = Vec::with_capacity(row_count);

        for row_idx in 0..row_count {
            let start = row_idx * cols;
            let mut cells: Vec<Element> = Vec::with_capacity(cols);
            for col_idx in 0..cols {
                let item_idx = start + col_idx;
                if let Some(item) = self.items.get(item_idx) {
                    cells.push(
                        PickerGridTile {
                            item: item.clone(),
                            index: item_idx,
                            highlighted: self.highlighted == Some(item_idx),
                            on_hover: self.on_hover.clone(),
                            on_select: self.on_select.clone(),
                            key: DiffKey::from(&item.id),
                        }
                        .into_element(),
                    );
                } else {
                    cells.push(
                        rect()
                            .key(format!("spacer.{row_idx}.{col_idx}"))
                            .width(Size::flex(1.0))
                            .into_element(),
                    );
                }
            }
            rows.push(
                rect()
                    .key(format!("grid-row.{row_idx}"))
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .spacing(4.0)
                    .children(cells)
                    .into_element(),
            );
        }

        let estimated_height = row_count as f32 * 134.0 + (row_count.saturating_sub(1)) as f32 * 4.0 + 4.0;
        let container_height = estimated_height.min(self.max_height);

        ScrollView::new()
            .width(Size::fill())
            .height(Size::px(container_height))
            .show_scrollbar(false)
            .child(
                rect()
                    .a11y_role(AccessibilityRole::ListBox)
                    .width(Size::fill())
                    .spacing(4.0)
                    .children(rows),
            )
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// One keyed icon tile inside [`PickerGrid`].
#[derive(Clone, PartialEq)]
struct PickerGridTile {
    item: PickerItem,
    index: usize,
    highlighted: bool,
    on_hover: Option<EventHandler<usize>>,
    on_select: Option<EventHandler<usize>>,
    key: DiffKey,
}

impl KeyExt for PickerGridTile {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for PickerGridTile {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);

        let is_selected = self.highlighted;
        let is_hovered = hovered() && !is_selected;
        let is_dimmed = self.item.dimmed && !is_selected;

        let (bg, border_color) = if is_selected {
            let accent_bg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.08)
                    .mix(theme.palette.purple, 0.14)
                    .with_alpha(0.24)
                    .to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(152, 146, 212, 0.42).to_color(),
            };
            let top_border = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(36, 255, 255, 255),
                ResolvedScheme::Light => Color::from_argb(110, 255, 255, 255),
            };
            (accent_bg, top_border)
        } else if is_hovered {
            let hover_bg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.07).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.05).to_color(),
            };
            (hover_bg, Color::TRANSPARENT)
        } else {
            (Color::TRANSPARENT, Color::TRANSPARENT)
        };

        let fg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(if is_dimmed { 195 } else { 242 }, 251, 251, 254),
            ResolvedScheme::Light => Color::from_argb(if is_dimmed { 180 } else { 235 }, 21, 20, 26),
        };
        let detail_fg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(165, 251, 251, 254),
            ResolvedScheme::Light => Color::from_argb(165, 21, 20, 26),
        };

        let plate_bg = if is_dimmed {
            match theme.scheme {
                ResolvedScheme::Dark => Color::from_rgb(138, 138, 142),
                ResolvedScheme::Light => Color::from_rgb(224, 224, 228),
            }
        } else {
            Color::WHITE
        };

        let glow_color = self.item.accent_color.unwrap_or_else(|| theme.color(ThemeRole::Hint));
        let plate_shadow = if is_selected {
            Shadow::new()
                .x(0.0)
                .y(8.0)
                .blur(22.0)
                .spread(-2.0)
                .color(Color::from_argb(135, glow_color.r(), glow_color.g(), glow_color.b()))
        } else {
            Shadow::new()
                .x(0.0)
                .y(3.0)
                .blur(10.0)
                .color(Color::from_argb(42, 20, 22, 40))
        };

        let icon_el: Element = if let Some((cache_key, bytes)) = &self.item.custom_icon_svg {
            SvgViewer::new((cache_key.clone(), bytes.clone()))
                .width(Size::px(38.0))
                .height(Size::px(38.0))
                .show_loader(false)
                .into_element()
        } else if let Some(icon_name) = &self.item.icon {
            if let Some((canonical, bytes)) = prepared_icon_bytes(icon_name) {
                SvgViewer::new((("zendia-icon", canonical), bytes))
                    .width(Size::px(34.0))
                    .height(Size::px(34.0))
                    .color(if is_dimmed {
                        Color::from_rgb(90, 92, 102)
                    } else {
                        Color::from_rgb(28, 30, 40)
                    })
                    .show_loader(false)
                    .into_element()
            } else {
                Icon::new(icon_name.clone())
                    .size(34.0)
                    .role(ThemeRole::Ink)
                    .into_element()
            }
        } else {
            Icon::new("sparkles").size(30.0).role(ThemeRole::Ink).into_element()
        };

        let plate = rect()
            .width(Size::px(60.0))
            .height(Size::px(60.0))
            .margin(Gaps::new(0.0, 0.0, 10.0, 0.0))
            .offset_y(if is_selected { -2.0 } else { 0.0 })
            .opacity(if is_dimmed { 0.62 } else { 1.0 })
            .corner_radius(CornerRadius::new_all(16.0))
            .overflow(Overflow::Clip)
            .background(plate_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(Color::from_argb(30, 0, 0, 0)),
            )
            .shadow(plate_shadow)
            .center()
            .child(icon_el);

        let index = self.index;
        let disabled = self.item.disabled;
        let on_hover = self.on_hover.clone();
        let on_select = self.on_select.clone();
        let a11y_label = match &self.item.detail {
            Some(detail) => format!("{} ({detail})", self.item.title),
            None => self.item.title.clone(),
        };

        rect()
            .a11y_role(AccessibilityRole::ListItem)
            .a11y_alt(a11y_label)
            .width(Size::flex(1.0))
            .height(Size::px(130.0))
            .padding(Gaps::new(18.0, 8.0, 12.0, 8.0))
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(14.0))
            .background(bg)
            .border(
                Border::new()
                    .width(if is_selected { 1.0 } else { 0.0 })
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .cursor(if disabled {
                CursorIcon::Default
            } else {
                CursorIcon::Pointer
            })
            .on_pointer_enter(move |_| {
                hovered.set_if_modified(true);
                if let Some(handler) = &on_hover {
                    handler.call(index);
                }
            })
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                if !disabled && let Some(handler) = &on_select {
                    handler.call(index);
                }
            })
            .maybe_child(self.item.key_badge.as_ref().map(|badge| {
                rect()
                    .position(Position::new_absolute().top(7.0).left(7.0))
                    .child(KeyLabel::new(badge.clone()).active(is_selected))
            }))
            .child(plate)
            .child(
                label()
                    .text(self.item.title.clone())
                    .font_size(13.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(fg)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .maybe_child(self.item.detail.as_ref().map(|detail| {
                rect().margin(Gaps::new(2.0, 0.0, 0.0, 0.0)).child(
                    label()
                        .text(detail.clone())
                        .font_size(11.0)
                        .color(detail_fg)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
            }))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
