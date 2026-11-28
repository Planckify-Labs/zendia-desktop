/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Generic selectable picker list component shared by the floating address bar, command palette,
//! future Vim pickers, and declarative plugin UI.

use crate::ui::kit::icons::{Icon, prepared_icon_bytes};
use crate::ui::kit::key_label::KeyLabel;
use crate::ui::kit::list_nav::scroll_item_into_view;
use crate::ui::kit::popup_presence::StaggeredRow;
use crate::ui::kit::tooltip::KitTooltip;
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, motion, use_theme};
use freya::animation::*;
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
    /// Optional custom pill color for the trailing action chip (used for workspace switch pills).
    pub action_chip_color: Option<Color>,
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
    /// Optional decoded/encoded WebExtension icon payload (`EXT-05`).
    pub extension_icon: Option<zendia_browser_core::modules::ExtensionIcon>,
    /// Optional brand accent color used for selected grid tile glow.
    pub accent_color: Option<Color>,
    /// When true in grid layout, the icon fills 100% of its rounded icon plate.
    pub icon_fill: bool,
    /// Visual dimmed appearance (for example, an uninstalled agent tile).
    pub dimmed: bool,
    /// Disables pointer activation when true.
    pub disabled: bool,
    /// Optional hover tooltip text (e.g. install instructions for uninstalled agents, `VIM-PK-07`).
    pub tooltip: Option<String>,
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
            action_chip_color: None,
            accent_highlight: false,
            section: None,
            detail: None,
            key_badge: None,
            custom_icon_svg: None,
            extension_icon: None,
            accent_color: None,
            icon_fill: false,
            dimmed: false,
            disabled: false,
            tooltip: None,
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

    pub fn action_chip_color(mut self, color: Color) -> Self {
        self.action_chip_color = Some(color);
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

    pub fn extension_icon(mut self, icon: zendia_browser_core::modules::ExtensionIcon) -> Self {
        self.extension_icon = Some(icon);
        self
    }

    pub fn accent_color(mut self, color: Color) -> Self {
        self.accent_color = Some(color);
        self
    }

    pub fn icon_fill(mut self, fill: bool) -> Self {
        self.icon_fill = fill;
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

    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

/// Height of a row in a [`PickerList`], and of one in its vim style.
const ROW_HEIGHT: f32 = 44.0;
const VIM_ROW_HEIGHT: f32 = 36.0;
/// Gap between rows, in both styles.
const ROW_GAP: f32 = 2.0;
/// Space above the first row, and above and below all rows together.
const LIST_PAD_TOP: f32 = 8.0;
const LIST_PAD_VERTICAL: f32 = 14.0;
const VIM_SECTION_HEIGHT: f32 = 24.0;

/// Scrollable list of [`PickerItem`] rows with keyboard highlight, pointer hover, and click selection.
/// The list scrolls to keep the highlighted row on screen.
#[derive(Clone, PartialEq)]
pub struct PickerList {
    items: Vec<PickerItem>,
    highlighted: Option<usize>,
    max_height: f32,
    empty_message: Option<String>,
    highlight_query: Option<String>,
    vim_style: bool,
    stagger_trigger: u64,
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
            highlight_query: None,
            vim_style: false,
            stagger_trigger: 0,
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

    /// Highlights matching query tokens in bold inside row titles and subtitles.
    pub fn highlight_query(mut self, query: impl Into<String>) -> Self {
        let q = query.into();
        self.highlight_query = if q.trim().is_empty() { None } else { Some(q) };
        self
    }

    /// Uses Zendia's compact Vim picker row geometry (section headings, leading key badges, right-aligned details).
    pub fn vim_style(mut self, enabled: bool) -> Self {
        self.vim_style = enabled;
        self
    }

    /// Triggers a capped per-row entrance stagger whenever `trigger > 0` changes.
    pub fn stagger_trigger(mut self, trigger: u64) -> Self {
        self.stagger_trigger = trigger;
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
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        // The highlight and row count the scroll position was last brought up to date for, and the row the pointer
        // last highlighted: scrolling to a hovered row would pull the list out from under the pointer.
        let mut shown = use_state(|| None::<(Option<usize>, usize)>);
        let mut pointed = use_state(|| None::<usize>);

        // Gliding cursor bar animation state for `vim_style` (`VIM-PK-01`).
        let target_cursor_y: Option<f32> = if self.vim_style
            && let Some(idx) = self.highlighted
            && idx < self.items.len()
        {
            Some(vim_row_span(&self.items, idx).0)
        } else {
            None
        };
        let mut prev_cursor_y = use_state(|| target_cursor_y.unwrap_or(0.0));
        let mut from_cursor_y = use_state(|| target_cursor_y.unwrap_or(0.0));
        let mut last_hl_row = use_state(|| self.highlighted);
        let live_cursor_y = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(target_cursor_y.unwrap_or(0.0))));
        let mut cursor_glide_seq = use_state(|| 0u64);
        if *last_hl_row.peek() != self.highlighted {
            last_hl_row.set(self.highlighted);
            if let Some(ty) = target_cursor_y {
                from_cursor_y.set(live_cursor_y.get());
                prev_cursor_y.set(ty);
                let next_seq = cursor_glide_seq.peek().wrapping_add(1);
                cursor_glide_seq.set(next_seq);
            }
        } else if let Some(ty) = target_cursor_y
            && (*prev_cursor_y.peek() - ty).abs() > 0.5
        {
            from_cursor_y.set(ty);
            prev_cursor_y.set(ty);
        }
        let glide_seq = *cursor_glide_seq.read();
        let glide_from = *from_cursor_y.read();
        let glide_to = target_cursor_y.unwrap_or(*prev_cursor_y.read());
        let cursor_glide = use_animation_with_dependencies(
            &(glide_seq, glide_from.to_bits(), glide_to.to_bits()),
            move |conf, _| {
                conf.on_creation(OnCreation::Finish);
                conf.on_change(OnChange::Rerun);
                AnimNum::new(0.0, 1.0)
                    .duration(motion::PICKER_CURSOR)
                    .ease(Ease::Out)
                    .function(motion::spring_fn())
            },
        );
        let animated_cursor_y = glide_from + (glide_to - glide_from) * cursor_glide.get().value();
        live_cursor_y.set(animated_cursor_y);

        let current = (self.highlighted, self.items.len());
        if *shown.peek() != Some(current) {
            shown.set(Some(current));
            let by_pointer = self.highlighted.is_some() && *pointed.peek() == self.highlighted;
            pointed.set_if_modified(None);
            if !by_pointer {
                let index = self.highlighted.unwrap_or(0).min(self.items.len().saturating_sub(1));
                let (top, height, content) = if self.vim_style {
                    let (top, content) = vim_row_span(&self.items, index);
                    (top, VIM_ROW_HEIGHT, content)
                } else {
                    let stride = ROW_HEIGHT + ROW_GAP;
                    (
                        LIST_PAD_TOP + index as f32 * stride,
                        ROW_HEIGHT,
                        self.items.len() as f32 * stride + LIST_PAD_VERTICAL,
                    )
                };
                if index == 0 {
                    scroll.scroll_to_y(0);
                } else {
                    scroll_item_into_view(&mut scroll, top, height, content.min(self.max_height), content);
                }
            }
        }

        let hover_handler = self.on_hover.clone();
        let on_hover: Option<EventHandler<usize>> = hover_handler.map(|handler| {
            EventHandler::new(move |index: usize| {
                pointed.set(Some(index));
                handler.call(index);
            })
        });

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

                let row_el = VimPickerRow {
                    item: item.clone(),
                    index,
                    highlighted: self.highlighted == Some(index),
                    gliding_cursor: true,
                    on_hover: on_hover.clone(),
                    on_select: self.on_select.clone(),
                    key: DiffKey::from(&item.id),
                }
                .into_element();
                let wrapped = if self.stagger_trigger > 0 {
                    StaggeredRow::new(index)
                        .trigger(self.stagger_trigger)
                        .key(("vim-row-stagger", &item.id))
                        .child(row_el)
                        .into_element()
                } else {
                    row_el
                };
                children.push(wrapped);
            }

            let (_, content_height) = vim_row_span(&self.items, 0);
            let container_height = content_height.min(self.max_height);
            let needs_scroll = content_height > self.max_height;

            let gliding_bar = target_cursor_y.map(|_| {
                let (cursor_bg, top_border, drop_shadow) = match theme.scheme {
                    ResolvedScheme::Dark => (
                        Rgba::from_rgb_alpha(255, 255, 255, 0.08)
                            .mix(theme.palette.purple, 0.26)
                            .with_alpha(0.28)
                            .to_color(),
                        Color::from_argb(32, 255, 255, 255),
                        Shadow::new().y(1.0).blur(3.0).color(Color::from_argb(64, 0, 0, 0)),
                    ),
                    ResolvedScheme::Light => (
                        Rgba::from_rgb_alpha(255, 255, 255, 0.40)
                            .mix(theme.palette.purple, 0.26)
                            .with_alpha(0.45)
                            .to_color(),
                        Color::from_argb(140, 255, 255, 255),
                        Shadow::new().y(1.0).blur(3.0).color(Color::from_argb(26, 20, 22, 40)),
                    ),
                };
                rect()
                    .position(Position::new_absolute().top(animated_cursor_y).left(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::px(VIM_ROW_HEIGHT))
                    .corner_radius(CornerRadius::new_all(11.0))
                    .background(cursor_bg)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(top_border),
                    )
                    .shadow(drop_shadow)
                    .interactive(Interactive::No)
            });

            let scrollbar_base = theme.rgba(ThemeRole::Scrollbar);
            let sb_thumb = scrollbar_base.with_alpha(0.45).to_color();
            let sb_hover = scrollbar_base.with_alpha(0.75).to_color();
            let sb_active = scrollbar_base.with_alpha(0.90).to_color();

            let sv = ScrollView::new_controlled(scroll)
                .width(Size::fill())
                .height(Size::px(container_height))
                .show_scrollbar(needs_scroll)
                .auto_hide_scrollbars(false)
                .scrollbar(move |context: ScrollBarContext| {
                    ScrollBar::new(context)
                        .theme(ScrollBarThemePartial {
                            background: Some(Preference::Specific(Color::TRANSPARENT)),
                            thumb_background: Some(Preference::Specific(sb_thumb)),
                            hover_thumb_background: Some(Preference::Specific(sb_hover)),
                            active_thumb_background: Some(Preference::Specific(sb_active)),
                            thumb_cross_size: Some(Preference::Specific(3.5)),
                            expanded_thumb_cross_size: Some(Preference::Specific(5.5)),
                            opacity: Some(Preference::Specific(1.0)),
                            expanded_opacity: Some(Preference::Specific(1.0)),
                            cross_gap: Some(Preference::Specific(2.0)),
                            expanded_cross_gap: Some(Preference::Specific(2.0)),
                        })
                        .into_element()
                })
                .child(
                    rect()
                        .a11y_role(AccessibilityRole::ListBox)
                        .width(Size::fill())
                        .padding(Gaps::new(0.0, if needs_scroll { 8.0 } else { 0.0 }, 0.0, 0.0))
                        .maybe_child(gliding_bar)
                        .child(rect().width(Size::fill()).spacing(2.0).children(children)),
                );
            return sv.into_element();
        }

        let rows = self.items.iter().enumerate().map(|(index, item)| {
            PickerRow {
                item: item.clone(),
                index,
                highlighted: self.highlighted == Some(index),
                highlight_query: self.highlight_query.clone(),
                on_hover: on_hover.clone(),
                on_select: self.on_select.clone(),
                key: DiffKey::from(&item.id),
            }
            .into_element()
        });

        // Estimate exact height (44px row + 2px spacing + 16px vertical padding) so rows never clip mid-line.
        let estimated_height = self.items.len() as f32 * (ROW_HEIGHT + ROW_GAP) + LIST_PAD_VERTICAL;
        let container_height = estimated_height.min(self.max_height);

        ScrollView::new_controlled(scroll)
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
/// The top of vim-style row `index` and the height of all rows with their section headings.
fn vim_row_span(items: &[PickerItem], index: usize) -> (f32, f32) {
    let mut y = 0.0;
    let mut top = 0.0;
    let mut previous_section: Option<&str> = None;
    for (i, item) in items.iter().enumerate() {
        if let Some(section) = item.section.as_deref()
            && previous_section != Some(section)
        {
            y += VIM_SECTION_HEIGHT + ROW_GAP;
            previous_section = Some(section);
        }
        if i == index {
            top = y;
        }
        y += VIM_ROW_HEIGHT + ROW_GAP;
    }
    (top, y + 6.0)
}

/// Splits `text` into `(segment, matched)` runs where whitespace-separated tokens of `query` match case-insensitively.
fn split_highlighted_runs(text: &str, query: Option<&str>) -> Vec<(String, bool)> {
    let Some(query) = query.map(str::trim).filter(|q| !q.is_empty()) else {
        return vec![(text.to_string(), false)];
    };
    let tokens: Vec<Vec<char>> = query
        .split_whitespace()
        .map(|tok| tok.chars().flat_map(char::to_lowercase).collect())
        .filter(|tok: &Vec<char>| !tok.is_empty())
        .collect();
    if tokens.is_empty() {
        return vec![(text.to_string(), false)];
    }

    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    let mut marked = vec![false; chars.len()];
    for tok in &tokens {
        let len = tok.len();
        if len == 0 || len > lower.len() {
            continue;
        }
        let mut i = 0;
        while i + len <= lower.len() {
            if lower[i..i + len] == tok[..] {
                for m in &mut marked[i..i + len] {
                    *m = true;
                }
                i += len;
            } else {
                i += 1;
            }
        }
    }

    let mut runs = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let flag = marked[i];
        let start = i;
        while i < chars.len() && marked[i] == flag {
            i += 1;
        }
        runs.push((chars[start..i].iter().collect(), flag));
    }
    if runs.is_empty() {
        runs.push((text.to_string(), false));
    }
    runs
}

fn highlighted_text_el(
    text: &str,
    query: Option<&str>,
    font_size: f32,
    base_weight: FontWeight,
    color: Color,
) -> Element {
    let runs = split_highlighted_runs(text, query);
    if runs.len() == 1 && !runs[0].1 {
        return label()
            .text(text.to_string())
            .font_size(font_size)
            .font_weight(base_weight)
            .color(color)
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis)
            .into_element();
    }
    let mut p = paragraph().max_lines(1).text_overflow(TextOverflow::Ellipsis);
    for (segment, is_match) in runs {
        let weight = if is_match { FontWeight::BOLD } else { base_weight };
        p = p.span(Span::new(segment).font_size(font_size).font_weight(weight).color(color));
    }
    p.into_element()
}

/// One keyed row inside [`PickerList`]. Kept as its own component so hover hooks stay unconditional per row.
#[derive(Clone, PartialEq)]
struct PickerRow {
    item: PickerItem,
    index: usize,
    highlighted: bool,
    highlight_query: Option<String>,
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
                .child(highlighted_text_el(
                    sub,
                    self.highlight_query.as_deref(),
                    13.5,
                    FontWeight::NORMAL,
                    subtitle_color,
                ))
        });

        let trailing_el: Option<Element> = if let Some(action_label) = &self.item.action_chip {
            let chip_bg = if let Some(custom) = self.item.action_chip_color {
                if is_accent {
                    Color::from_argb(235, 255, 255, 255)
                } else {
                    custom
                }
            } else if is_accent {
                Color::from_argb(235, 255, 255, 255)
            } else if theme.scheme.is_dark() {
                Color::from_argb(220, 232, 234, 240)
            } else {
                Color::from_argb(215, 58, 62, 74)
            };
            let chip_fg = if is_accent {
                self.item
                    .action_chip_color
                    .unwrap_or_else(|| theme.color(ThemeRole::Accent))
            } else if self.item.action_chip_color.is_some() {
                Color::WHITE
            } else if theme.scheme.is_dark() {
                Color::from_rgb(28, 30, 38)
            } else {
                Color::WHITE
            };
            let arrow_el: Element = if let Some((canonical, bytes)) = prepared_icon_bytes("urlbar-arrow") {
                SvgViewer::new((("zendia-icon", canonical), bytes))
                    .width(Size::px(14.0))
                    .height(Size::px(14.0))
                    .color(chip_fg)
                    .show_loader(false)
                    .into_element()
            } else {
                label().text("→").font_size(11.0).color(chip_fg).into_element()
            };
            let chip_opacity = if is_accent { 1.0 } else { 0.7 };
            Some(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .opacity(chip_opacity)
                    .scale(0.93)
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
                    .child(highlighted_text_el(
                        &self.item.title,
                        self.highlight_query.as_deref(),
                        14.0,
                        if self.highlighted || self.item.action_chip.is_some() {
                            FontWeight::SEMI_BOLD
                        } else {
                            FontWeight::MEDIUM
                        },
                        title_color,
                    ))
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
    gliding_cursor: bool,
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

        let (bg, border_color) = if is_selected && !self.gliding_cursor {
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

        let icon_el: Option<Element> = if let Some(ext_icon) = &self.item.extension_icon {
            Some(
                crate::ui::extensions::ExtensionIconView::new(
                    self.item.id.clone(),
                    self.item.icon.clone().unwrap_or_else(|| "extension".to_string()),
                    Some(ext_icon.clone()),
                    16.0,
                    ThemeRole::Text,
                )
                .into_element(),
            )
        } else if let Some((cache_key, bytes)) = &self.item.custom_icon_svg {
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

        let row = rect()
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
                    .width(if is_selected && !self.gliding_cursor { 1.0 } else { 0.0 })
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
            }));

        if let Some(tip) = &self.item.tooltip {
            KitTooltip::new(tip.clone()).fill_width(true).child(row).into_element()
        } else {
            row.into_element()
        }
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
    stagger_trigger: u64,
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
            stagger_trigger: 0,
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

    pub fn stagger_trigger(mut self, trigger: u64) -> Self {
        self.stagger_trigger = trigger;
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

const GRID_TILE_HEIGHT: f32 = 130.0;
const GRID_GAP: f32 = 4.0;

impl Component for PickerGrid {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let mut grid_width = use_state(|| 486.0f32);
        let mut shown = use_state(|| None::<(Option<usize>, usize)>);

        let cols = self.columns.max(1);
        let row_count = self.items.len().div_ceil(cols);
        let estimated_height = row_count as f32 * (GRID_TILE_HEIGHT + GRID_GAP) + 4.0;
        let container_height = estimated_height.min(self.max_height);
        let needs_scroll = estimated_height > self.max_height;

        let inner_w = (*grid_width.read() - if needs_scroll { 8.0 } else { 0.0 }).max(120.0);
        let tile_w = ((inner_w - GRID_GAP * (cols.saturating_sub(1)) as f32) / cols as f32).max(40.0);
        let target_pos: Option<(f32, f32)> = self.highlighted.filter(|&i| i < self.items.len()).map(|idx| {
            let r = idx / cols;
            let c = idx % cols;
            (c as f32 * (tile_w + GRID_GAP), r as f32 * (GRID_TILE_HEIGHT + GRID_GAP))
        });
        let mut prev_pos = use_state(|| target_pos.unwrap_or((0.0, 0.0)));
        let mut from_pos = use_state(|| target_pos.unwrap_or((0.0, 0.0)));
        let mut last_hl_tile = use_state(|| self.highlighted);
        let live_pos = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(target_pos.unwrap_or((0.0, 0.0)))));
        let mut glide_seq = use_state(|| 0u64);
        if *last_hl_tile.peek() != self.highlighted {
            last_hl_tile.set(self.highlighted);
            if let Some((tx, ty)) = target_pos {
                from_pos.set(live_pos.get());
                prev_pos.set((tx, ty));
                let next = glide_seq.peek().wrapping_add(1);
                glide_seq.set(next);
            }
        } else if let Some((tx, ty)) = target_pos {
            let (px, py) = *prev_pos.peek();
            if (px - tx).abs() > 0.5 || (py - ty).abs() > 0.5 {
                from_pos.set((tx, ty));
                prev_pos.set((tx, ty));
            }
        }
        let g_seq = *glide_seq.read();
        let (fx, fy) = *from_pos.read();
        let (tx, ty) = target_pos.unwrap_or(*prev_pos.read());
        let cursor_glide = use_animation_with_dependencies(
            &(g_seq, fx.to_bits(), fy.to_bits(), tx.to_bits(), ty.to_bits()),
            move |conf, _| {
                conf.on_creation(OnCreation::Finish);
                conf.on_change(OnChange::Rerun);
                AnimNum::new(0.0, 1.0)
                    .duration(motion::PICKER_CURSOR)
                    .ease(Ease::Out)
                    .function(motion::spring_fn())
            },
        );
        let gt = cursor_glide.get().value();
        let cur_x = fx + (tx - fx) * gt;
        let cur_y = fy + (ty - fy) * gt;
        live_pos.set((cur_x, cur_y));

        let current = (self.highlighted, self.items.len());
        if *shown.peek() != Some(current) {
            shown.set(Some(current));
            if let Some(idx) = self.highlighted
                && idx < self.items.len()
            {
                let row = idx / cols;
                let top = row as f32 * (GRID_TILE_HEIGHT + GRID_GAP);
                if row == 0 {
                    scroll.scroll_to_y(0);
                } else {
                    scroll_item_into_view(&mut scroll, top, GRID_TILE_HEIGHT, container_height, estimated_height);
                }
            }
        }

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

        let mut rows: Vec<Element> = Vec::with_capacity(row_count);
        for row_idx in 0..row_count {
            let start = row_idx * cols;
            let mut cells: Vec<Element> = Vec::with_capacity(cols);
            for col_idx in 0..cols {
                let item_idx = start + col_idx;
                if let Some(item) = self.items.get(item_idx) {
                    let tile_el = PickerGridTile {
                        item: item.clone(),
                        index: item_idx,
                        highlighted: self.highlighted == Some(item_idx),
                        gliding_cursor: true,
                        on_hover: self.on_hover.clone(),
                        on_select: self.on_select.clone(),
                        key: DiffKey::from(&item.id),
                    }
                    .into_element();
                    let wrapped = if self.stagger_trigger > 0 {
                        StaggeredRow::new(item_idx)
                            .trigger(self.stagger_trigger)
                            .key(("grid-tile-stagger", &item.id))
                            .child(tile_el)
                            .into_element()
                    } else {
                        tile_el
                    };
                    cells.push(
                        rect()
                            .key(format!("cell.{}", item.id))
                            .width(Size::flex(1.0))
                            .child(wrapped)
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
                    .spacing(GRID_GAP)
                    .children(cells)
                    .into_element(),
            );
        }

        let gliding_tile = target_pos.map(|_| {
            let (cursor_bg, top_border, drop_shadow) = match theme.scheme {
                ResolvedScheme::Dark => (
                    Rgba::from_rgb_alpha(255, 255, 255, 0.08)
                        .mix(theme.palette.purple, 0.26)
                        .with_alpha(0.28)
                        .to_color(),
                    Color::from_argb(32, 255, 255, 255),
                    Shadow::new().y(1.0).blur(3.0).color(Color::from_argb(64, 0, 0, 0)),
                ),
                ResolvedScheme::Light => (
                    Rgba::from_rgb_alpha(255, 255, 255, 0.40)
                        .mix(theme.palette.purple, 0.26)
                        .with_alpha(0.45)
                        .to_color(),
                    Color::from_argb(140, 255, 255, 255),
                    Shadow::new().y(1.0).blur(3.0).color(Color::from_argb(26, 20, 22, 40)),
                ),
            };
            rect()
                .position(Position::new_absolute().left(cur_x).top(cur_y))
                .width(Size::px(tile_w))
                .height(Size::px(GRID_TILE_HEIGHT))
                .corner_radius(CornerRadius::new_all(14.0))
                .background(cursor_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(top_border),
                )
                .shadow(drop_shadow)
                .interactive(Interactive::No)
        });

        let scrollbar_base = theme.rgba(ThemeRole::Scrollbar);
        let sb_thumb = scrollbar_base.with_alpha(0.45).to_color();
        let sb_hover = scrollbar_base.with_alpha(0.75).to_color();
        let sb_active = scrollbar_base.with_alpha(0.90).to_color();

        ScrollView::new_controlled(scroll)
            .width(Size::fill())
            .height(Size::px(container_height))
            .show_scrollbar(needs_scroll)
            .auto_hide_scrollbars(false)
            .scrollbar(move |context: ScrollBarContext| {
                ScrollBar::new(context)
                    .theme(ScrollBarThemePartial {
                        background: Some(Preference::Specific(Color::TRANSPARENT)),
                        thumb_background: Some(Preference::Specific(sb_thumb)),
                        hover_thumb_background: Some(Preference::Specific(sb_hover)),
                        active_thumb_background: Some(Preference::Specific(sb_active)),
                        thumb_cross_size: Some(Preference::Specific(3.5)),
                        expanded_thumb_cross_size: Some(Preference::Specific(5.5)),
                        opacity: Some(Preference::Specific(1.0)),
                        expanded_opacity: Some(Preference::Specific(1.0)),
                        cross_gap: Some(Preference::Specific(2.0)),
                        expanded_cross_gap: Some(Preference::Specific(2.0)),
                    })
                    .into_element()
            })
            .child(
                rect()
                    .a11y_role(AccessibilityRole::ListBox)
                    .width(Size::fill())
                    .padding(Gaps::new(0.0, if needs_scroll { 8.0 } else { 0.0 }, 0.0, 0.0))
                    .on_sized(move |e: Event<SizedEventData>| {
                        let w = e.area.width();
                        if w > 40.0 && (*grid_width.peek() - w).abs() > 0.5 {
                            grid_width.set(w);
                        }
                    })
                    .maybe_child(gliding_tile)
                    .child(rect().width(Size::fill()).spacing(GRID_GAP).children(rows)),
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
    gliding_cursor: bool,
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
        let lift_anim = use_animation_with_dependencies(&is_selected, move |conf, selected| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *selected { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::PICKER_PLATE_LIFT)
                .ease(Ease::Out)
                .function(motion::spring_fn())
        });
        let glow_anim = use_animation_with_dependencies(&is_selected, move |conf, selected| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *selected { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::PICKER_PLATE_GLOW)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let lift_t = lift_anim.get().value().clamp(0.0, 1.05);
        let glow_t = glow_anim.get().value().clamp(0.0, 1.0);
        let is_hovered = hovered() && !is_selected;
        let is_dimmed = self.item.dimmed && !is_selected;

        let (bg, border_color) = if is_selected && !self.gliding_cursor {
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
        let rest_shadow = Shadow::new().x(0.0).y(3.0).blur(10.0).color(Color::from_argb(
            ((1.0 - glow_t) * 41.0).round() as u8,
            20,
            22,
            40,
        ));
        let glow_shadow = Shadow::new()
            .x(0.0)
            .y(10.0)
            .blur(26.0)
            .spread(-4.0)
            .color(Color::from_argb(
                (glow_t * 178.0).round() as u8,
                glow_color.r(),
                glow_color.g(),
                glow_color.b(),
            ));

        let icon_dim = if self.item.icon_fill { 60.0 } else { 38.0 };
        let icon_el: Element = if let Some(ext_icon) = &self.item.extension_icon {
            crate::ui::extensions::ExtensionIconView::new(
                self.item.id.clone(),
                self.item.icon.clone().unwrap_or_else(|| "extension".to_string()),
                Some(ext_icon.clone()),
                icon_dim,
                ThemeRole::Ink,
            )
            .into_element()
        } else if let Some((cache_key, bytes)) = &self.item.custom_icon_svg {
            SvgViewer::new((cache_key.clone(), bytes.clone()))
                .width(Size::px(icon_dim))
                .height(Size::px(icon_dim))
                .show_loader(false)
                .into_element()
        } else if let Some(icon_name) = &self.item.icon {
            let glyph_dim = if self.item.icon_fill { 60.0 } else { 34.0 };
            if let Some((canonical, bytes)) = prepared_icon_bytes(icon_name) {
                SvgViewer::new((("zendia-icon", canonical), bytes))
                    .width(Size::px(glyph_dim))
                    .height(Size::px(glyph_dim))
                    .color(if is_dimmed {
                        Color::from_rgb(90, 92, 102)
                    } else {
                        Color::from_rgb(28, 30, 40)
                    })
                    .show_loader(false)
                    .into_element()
            } else {
                Icon::new(icon_name.clone())
                    .size(glyph_dim)
                    .role(ThemeRole::Ink)
                    .into_element()
            }
        } else {
            Icon::new("sparkles").size(30.0).role(ThemeRole::Ink).into_element()
        };

        let plate_scale = 1.0 + 0.08 * lift_t;
        let plate_offset_y = -2.0 * lift_t;
        let plate = rect()
            .width(Size::px(60.0))
            .height(Size::px(60.0))
            .margin(Gaps::new(0.0, 0.0, 10.0, 0.0))
            .offset_y(plate_offset_y)
            .scale((plate_scale, plate_scale))
            .opacity(if is_dimmed { 0.55 } else { 1.0 })
            .corner_radius(CornerRadius::new_all(16.0))
            .overflow(Overflow::Clip)
            .background(plate_bg)
            .border(
                Border::new()
                    .width(0.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(Color::from_argb(30, 0, 0, 0)),
            )
            .shadow(rest_shadow)
            .shadow(glow_shadow)
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

        let tile = rect()
            .a11y_role(AccessibilityRole::ListItem)
            .a11y_alt(a11y_label)
            .width(Size::fill())
            .height(Size::px(GRID_TILE_HEIGHT))
            .padding(Gaps::new(18.0, 8.0, 12.0, 8.0))
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(14.0))
            .background(bg)
            .border(
                Border::new()
                    .width(if is_selected && !self.gliding_cursor { 1.0 } else { 0.0 })
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
            }));

        if let Some(tip) = &self.item.tooltip {
            KitTooltip::new(tip.clone()).fill_width(true).child(tile).into_element()
        } else {
            tile.into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
