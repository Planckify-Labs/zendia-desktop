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
use crate::ui::kit::{
    Icon, ListNavAction, ListNavConfig, TypeaheadState, classify_list_key, compute_scroll_into_view_y,
    max_popup_height, moved_index, scroll_item_into_view,
};
use crate::ui::theme::POPUP_OVERLAY_LEVEL;
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::time::Instant;
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

const GROUP_HEADER_HEIGHT: f32 = 24.0;
/// Space between the popup's outer edge and its first row, top and bottom together.
const SELECT_CHROME_HEIGHT: f32 = 10.0;

fn row_height(row: &FlatSelectRow) -> f32 {
    match row {
        FlatSelectRow::Option { .. } => CTX_MENU_ROW_HEIGHT,
        FlatSelectRow::GroupHeader { .. } => GROUP_HEADER_HEIGHT,
        FlatSelectRow::Separator => CTX_MENU_SEP_HEIGHT,
    }
}

fn rows_height(rows: &[FlatSelectRow]) -> f32 {
    rows.iter().map(row_height).sum()
}

fn is_pickable(rows: &[FlatSelectRow], idx: usize) -> bool {
    matches!(rows.get(idx), Some(FlatSelectRow::Option { option, .. }) if !option.disabled)
}

fn option_label(rows: &[FlatSelectRow], idx: usize) -> Option<String> {
    match rows.get(idx) {
        Some(FlatSelectRow::Option { option, .. }) => Some(option.label.clone()),
        _ => None,
    }
}

/// The popup's `(width, height)`, with the height capped at `max_height`; taller lists scroll.
fn estimate_select_size(rows: &[FlatSelectRow], minimum_width: f32, max_height: f32) -> (f32, f32) {
    let mut max_chars = 14usize;
    for row in rows {
        match row {
            FlatSelectRow::Option { option, indented } => {
                max_chars = max_chars.max(option.label.chars().count() + if *indented { 3 } else { 0 });
            }
            FlatSelectRow::GroupHeader { label } => max_chars = max_chars.max(label.chars().count()),
            FlatSelectRow::Separator => {}
        }
    }
    let width = ((max_chars as f32) * 7.2 + 44.0)
        .max(minimum_width)
        .clamp(SELECT_MIN_WIDTH, SELECT_MAX_WIDTH);
    (width, (rows_height(rows) + SELECT_CHROME_HEIGHT).min(max_height))
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
        let max_height = max_popup_height(window_size.1, 12.0, SELECT_MAX_HEIGHT);
        let est_size = estimate_select_size(&rows, open.minimum_width, max_height);
        let seq = *menu_state.select_seq.read();
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
                            .key(seq)
                            .max_height(max_height)
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
    max_height: f32,
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
            max_height: SELECT_MAX_HEIGHT,
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

    pub fn max_height(mut self, height: f32) -> Self {
        self.max_height = height;
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
        let mut typeahead = use_state(TypeaheadState::default);

        let viewport_height = self.max_height - SELECT_CHROME_HEIGHT;
        let content_height = rows_height(&rows);
        let scrolls = content_height > viewport_height;
        // The popup opens with the chosen option on screen.
        let mut scroll = use_hook(|| {
            let top = initial_selected.map_or(0.0, |idx| rows_height(&rows[..idx]));
            let y = compute_scroll_into_view_y(0, top, CTX_MENU_ROW_HEIGHT, viewport_height, content_height);
            ScrollController::new(0, y.unwrap_or(0))
        });

        let rows_for_keys = rows.clone();
        let on_select_keys = self.on_select.clone();
        let core_keys = core.clone();
        let tab_id_keys = self.tab_id;
        let capture = self.capture_global_keys;

        let handle_key = move |e: Event<KeyboardEventData>| {
            if !capture {
                return;
            }
            let vim_enabled = core_keys.model().peek().vim_mode_enabled;
            let Some(mut action) = classify_list_key(&e, ListNavConfig::select().with_vim_keys(vim_enabled)) else {
                if vim_enabled {
                    e.stop_propagation();
                    e.prevent_default();
                }
                return;
            };
            if matches!(&e.key, Key::Character(ch) if ch == " ") && typeahead.peek().is_typing(Instant::now()) {
                action = ListNavAction::Typeahead(' ');
            }
            e.stop_propagation();
            e.prevent_default();
            let choose = |item_id: Option<u32>| {
                if let Some(handler) = &on_select_keys {
                    handler.call(item_id);
                } else {
                    core_keys.invoke_or_log(Command::PageSelectDropdownChoose {
                        tab_id: tab_id_keys,
                        item_id,
                    });
                }
            };
            match action {
                ListNavAction::Dismiss => choose(None),
                ListNavAction::Activate => {
                    if let Some(idx) = *active_index.peek()
                        && let Some(FlatSelectRow::Option { option, .. }) = rows_for_keys.get(idx)
                        && !option.disabled
                    {
                        choose(Some(option.id));
                    }
                }
                _ => {
                    let current = *active_index.peek();
                    let next = moved_index(
                        &action,
                        &mut typeahead.write(),
                        rows_for_keys.len(),
                        current,
                        false,
                        |idx| option_label(&rows_for_keys, idx),
                        |idx| is_pickable(&rows_for_keys, idx),
                    );
                    if let Some(idx) = next {
                        active_index.set(Some(idx));
                        let top = rows_height(&rows_for_keys[..idx]);
                        scroll_item_into_view(&mut scroll, top, CTX_MENU_ROW_HEIGHT, viewport_height, content_height);
                    }
                }
            }
        };

        let (width, _) = estimate_select_size(&rows, self.minimum_width, self.max_height);
        let current_active = *active_index.read();

        let rendered_rows = rows.iter().enumerate().map(|(idx, row)| match row {
            FlatSelectRow::Separator => context_menu_separator(&theme, idx),
            FlatSelectRow::GroupHeader { label: group_label } => rect()
                .key(idx)
                .width(Size::fill())
                .height(Size::px(GROUP_HEADER_HEIGHT))
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

        let body: Element = if scrolls {
            ScrollView::new_controlled(scroll)
                .width(Size::fill())
                .height(Size::px(viewport_height))
                .children(rendered_rows)
                .into_element()
        } else {
            rect().width(Size::fill()).children(rendered_rows).into_element()
        };

        rect()
            .a11y_role(AccessibilityRole::ListBox)
            .width(Size::px(width))
            .max_height(Size::px(self.max_height))
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
