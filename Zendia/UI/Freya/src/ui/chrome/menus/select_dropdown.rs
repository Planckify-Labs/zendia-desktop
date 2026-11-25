/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! HTML `<select>` dropdown popup rendered in Ladybird's native menu look.

use crate::ui::chrome::menus::context_menu::{
    CTX_MENU_ROW_HEIGHT, CTX_MENU_SEP_HEIGHT, context_menu_separator, native_menu_bg, native_menu_hover_bg,
};
use crate::ui::chrome::menus::state::{clamp_popup_origin, use_menu_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::theme::POPUP_OVERLAY_LEVEL;
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::menus::{SelectDropdownItem, SelectOption};
use zendia_browser_core::model::TabId;

const SELECT_MIN_WIDTH: f32 = 160.0;
const SELECT_MAX_WIDTH: f32 = 380.0;
const SELECT_MAX_HEIGHT: f32 = 320.0;

#[derive(Clone, Debug, PartialEq)]
enum FlatSelectRow {
    Option { option: SelectOption, indented: bool },
    GroupHeader { label: String },
    Separator,
}

fn flatten_select_items(items: &[SelectDropdownItem]) -> Vec<FlatSelectRow> {
    let mut rows = Vec::new();
    for item in items {
        match item {
            SelectDropdownItem::Option(option) => rows.push(FlatSelectRow::Option {
                option: option.clone(),
                indented: false,
            }),
            SelectDropdownItem::Group { label, items } => {
                rows.push(FlatSelectRow::GroupHeader { label: label.clone() });
                for option in items {
                    rows.push(FlatSelectRow::Option {
                        option: option.clone(),
                        indented: true,
                    });
                }
            }
            SelectDropdownItem::Separator => rows.push(FlatSelectRow::Separator),
        }
    }
    rows
}

fn next_option_index(rows: &[FlatSelectRow], current: Option<usize>, delta: isize) -> Option<usize> {
    let len = rows.len();
    if len == 0 {
        return None;
    }
    let is_pickable = |idx: usize| {
        matches!(
            rows.get(idx),
            Some(FlatSelectRow::Option { option, .. }) if !option.disabled
        )
    };
    let start = match current {
        Some(idx) => ((idx as isize + delta).rem_euclid(len as isize)) as usize,
        None => {
            if delta >= 0 {
                0
            } else {
                len - 1
            }
        }
    };
    for step in 0..len {
        let idx = ((start as isize + delta * step as isize).rem_euclid(len as isize)) as usize;
        if is_pickable(idx) {
            return Some(idx);
        }
    }
    None
}

fn estimate_select_size(rows: &[FlatSelectRow], minimum_width: f32) -> (f32, f32) {
    let mut height = 10.0;
    let mut max_chars = 14usize;
    for row in rows {
        match row {
            FlatSelectRow::Option { option, indented } => {
                height += CTX_MENU_ROW_HEIGHT;
                max_chars = max_chars.max(option.label.chars().count() + if *indented { 3 } else { 0 });
            }
            FlatSelectRow::GroupHeader { label } => {
                height += 24.0;
                max_chars = max_chars.max(label.chars().count());
            }
            FlatSelectRow::Separator => {
                height += CTX_MENU_SEP_HEIGHT;
            }
        }
    }
    let width = ((max_chars as f32) * 7.2 + 44.0)
        .max(minimum_width)
        .clamp(SELECT_MIN_WIDTH, SELECT_MAX_WIDTH);
    (width, height.min(SELECT_MAX_HEIGHT))
}

/// Overlay host rendering the active `<select>` dropdown popup in [`MenuViewState`].
#[derive(Clone, PartialEq)]
pub struct SelectDropdownOverlay;

impl Component for SelectDropdownOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let menu_state = use_menu_state();
        let Some(open) = menu_state.select_dropdown.read().clone() else {
            return rect().into_element();
        };

        let rows = flatten_select_items(&open.items);
        let root_size = *Platform::get().current_window().root_size.read();
        let window_size = (root_size.width.max(320.0), root_size.height.max(240.0));
        let est_size = estimate_select_size(&rows, open.minimum_width);
        let origin = clamp_popup_origin(open.position, est_size, window_size, 6.0);

        let dismiss_core = core.clone();
        let dismiss_tab = open.tab_id;
        let on_dismiss = move || {
            menu_state.close_select_dropdown();
            dismiss_core.invoke_or_log(Command::PageSelectDropdownChoose {
                tab_id: dismiss_tab,
                item_id: None,
            });
        };

        let on_dismiss_backdrop = on_dismiss.clone();
        let on_choose_core = core.clone();
        let choose_tab = open.tab_id;

        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .child(
                rect()
                    .position(Position::new_global().left(0.0).top(0.0))
                    .width(Size::window_percent(100.0))
                    .height(Size::window_percent(100.0))
                    .on_mouse_down(move |_| on_dismiss_backdrop()),
            )
            .child(
                rect()
                    .position(Position::new_global().left(origin.0).top(origin.1))
                    .child(
                        SelectDropdownCard::new(open.items)
                            .tab_id(choose_tab)
                            .minimum_width(open.minimum_width)
                            .capture_global_keys(true)
                            .on_select(move |item_id: Option<u32>| {
                                menu_state.close_select_dropdown();
                                on_choose_core.invoke_or_log(Command::PageSelectDropdownChoose {
                                    tab_id: choose_tab,
                                    item_id,
                                });
                            }),
                    ),
            )
            .into_element()
    }
}

/// Reusable `<select>` dropdown popup card matching Ladybird's native menu look.
#[derive(Clone, PartialEq)]
pub struct SelectDropdownCard {
    items: Vec<SelectDropdownItem>,
    tab_id: Option<TabId>,
    minimum_width: f32,
    capture_global_keys: bool,
    on_select: Option<EventHandler<Option<u32>>>,
    key: DiffKey,
}

impl KeyExt for SelectDropdownCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl SelectDropdownCard {
    pub fn new(items: Vec<SelectDropdownItem>) -> Self {
        Self {
            items,
            tab_id: None,
            minimum_width: SELECT_MIN_WIDTH,
            capture_global_keys: false,
            on_select: None,
            key: DiffKey::None,
        }
    }

    pub fn tab_id(mut self, tab_id: Option<TabId>) -> Self {
        self.tab_id = tab_id;
        self
    }

    pub fn minimum_width(mut self, width: f32) -> Self {
        self.minimum_width = width;
        self
    }

    pub fn capture_global_keys(mut self, capture: bool) -> Self {
        self.capture_global_keys = capture;
        self
    }

    pub fn on_select(mut self, handler: impl Into<EventHandler<Option<u32>>>) -> Self {
        self.on_select = Some(handler.into());
        self
    }
}

impl Component for SelectDropdownCard {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let core = use_core();
        let rows = flatten_select_items(&self.items);

        let initial_selected = rows
            .iter()
            .position(|row| matches!(row, FlatSelectRow::Option { option, .. } if option.selected && !option.disabled));
        let mut active_index = use_state(move || initial_selected);

        let rows_for_keys = rows.clone();
        let on_select_keys = self.on_select.clone();
        let core_keys = core.clone();
        let tab_id_keys = self.tab_id;
        let capture = self.capture_global_keys;

        let handle_key = move |e: Event<KeyboardEventData>| {
            if !capture {
                return;
            }
            match &e.key {
                Key::Named(NamedKey::Escape) => {
                    e.stop_propagation();
                    if let Some(handler) = &on_select_keys {
                        handler.call(None);
                    } else {
                        core_keys.invoke_or_log(Command::PageSelectDropdownChoose {
                            tab_id: tab_id_keys,
                            item_id: None,
                        });
                    }
                }
                Key::Named(NamedKey::ArrowDown) => {
                    e.stop_propagation();
                    let next = next_option_index(&rows_for_keys, *active_index.peek(), 1);
                    active_index.set(next);
                }
                Key::Named(NamedKey::ArrowUp) => {
                    e.stop_propagation();
                    let next = next_option_index(&rows_for_keys, *active_index.peek(), -1);
                    active_index.set(next);
                }
                Key::Named(NamedKey::Enter) => {
                    e.stop_propagation();
                    if let Some(idx) = *active_index.peek()
                        && let Some(FlatSelectRow::Option { option, .. }) = rows_for_keys.get(idx)
                        && !option.disabled
                    {
                        if let Some(handler) = &on_select_keys {
                            handler.call(Some(option.id));
                        } else {
                            core_keys.invoke_or_log(Command::PageSelectDropdownChoose {
                                tab_id: tab_id_keys,
                                item_id: Some(option.id),
                            });
                        }
                    }
                }
                _ => {}
            }
        };

        let (width, _) = estimate_select_size(&rows, self.minimum_width);
        let current_active = *active_index.read();

        let rendered_rows = rows.iter().enumerate().map(|(idx, row)| match row {
            FlatSelectRow::Separator => context_menu_separator(&theme, idx),
            FlatSelectRow::GroupHeader { label: group_label } => rect()
                .key(idx)
                .width(Size::fill())
                .height(Size::px(24.0))
                .padding(Gaps::new(4.0, 10.0, 2.0, 10.0))
                .cross_align(Alignment::center())
                .direction(Direction::Horizontal)
                .child(
                    label()
                        .text(group_label.clone())
                        .font_size(11.5)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(theme.color(ThemeRole::TextMuted))
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
                .into_element(),
            FlatSelectRow::Option { option, indented } => {
                let highlighted = current_active == Some(idx) && !option.disabled;
                let bg = if highlighted {
                    native_menu_hover_bg(&theme)
                } else {
                    Color::TRANSPARENT
                };
                let text_color = if option.disabled {
                    theme.rgba(ThemeRole::TextMuted).with_alpha(0.55).to_color()
                } else {
                    theme.color(ThemeRole::Text)
                };
                let opt_id = option.id;
                let disabled = option.disabled;
                let on_select = self.on_select.clone();
                let core_press = core.clone();
                let tab_id = self.tab_id;
                let left_pad = if *indented { 18.0 } else { 8.0 };

                rect()
                    .key(idx)
                    .a11y_role(AccessibilityRole::ListItem)
                    .a11y_alt(option.label.clone())
                    .width(Size::fill())
                    .height(Size::px(CTX_MENU_ROW_HEIGHT))
                    .padding(Gaps::new(0.0, 10.0, 0.0, left_pad))
                    .spacing(8.0)
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .corner_radius(CornerRadius::new_all(4.0))
                    .background(bg)
                    .cursor(if disabled {
                        CursorIcon::Default
                    } else {
                        CursorIcon::Pointer
                    })
                    .on_pointer_enter(move |_| {
                        if !disabled {
                            active_index.set(Some(idx));
                        }
                    })
                    .on_press(move |_| {
                        if disabled {
                            return;
                        }
                        if let Some(handler) = &on_select {
                            handler.call(Some(opt_id));
                        } else {
                            core_press.invoke_or_log(Command::PageSelectDropdownChoose {
                                tab_id,
                                item_id: Some(opt_id),
                            });
                        }
                    })
                    .child(
                        rect()
                            .width(Size::px(14.0))
                            .height(Size::px(14.0))
                            .center()
                            .maybe_child(option.selected.then(|| {
                                Icon::new("check").size(13.0).role(if disabled {
                                    ThemeRole::TextMuted
                                } else {
                                    ThemeRole::Text
                                })
                            })),
                    )
                    .child(
                        label()
                            .text(option.label.clone())
                            .width(Size::flex(1.0))
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                            .font_size(12.5)
                            .color(text_color),
                    )
                    .into_element()
            }
        });

        let estimated_content_height = estimate_select_size(&rows, self.minimum_width).1;
        let body: Element = if estimated_content_height > SELECT_MAX_HEIGHT {
            ScrollView::new()
                .width(Size::fill())
                .height(Size::px(SELECT_MAX_HEIGHT - 8.0))
                .children(rendered_rows)
                .into_element()
        } else {
            rect().width(Size::fill()).children(rendered_rows).into_element()
        };

        rect()
            .a11y_role(AccessibilityRole::ListBox)
            .width(Size::px(width))
            .max_height(Size::px(SELECT_MAX_HEIGHT))
            .padding(Gaps::new_all(4.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(native_menu_bg(&theme))
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::Border)),
            )
            .shadow(theme.shadows.card.to_shadow())
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .on_global_key_down(handle_key)
            .child(body)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
