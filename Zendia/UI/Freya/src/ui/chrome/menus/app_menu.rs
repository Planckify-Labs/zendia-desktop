/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Zen-styled application menu panel with icons, right-aligned shortcuts, submenu cascades, the built-in zoom row,
//! and the Vim mode switch backed by [`BrowserCore`](zendia_browser_core::BrowserCore).

use crate::ui::chrome::menus::state::{AnchorRect, compute_app_menu_origin, invoke_menu_command, use_menu_state};
use crate::ui::core_view::{CoreView, use_core};
use crate::ui::kit::{
    Icon, KitSwitch, KitTooltip, ListNavAction, ListNavConfig, PopupContent, PopupPresence, StaggeredRow,
    TypeaheadState, classify_list_key, first_selectable, last_selectable, max_popup_height, page_step_selectable,
    scroll_item_into_view, step_selectable, use_interactive_feedback,
};
use crate::ui::theme::{ResolvedTheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Instant;
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::{MenuEntryKind, MenuLocation};
use zendia_browser_core::menus::{
    APP_MENU_GROUPS, ResolvedMenuItem, ZOOM_CONTROLS_WIDGET_ID, resolve_contributed_menu,
};
use zendia_keymap::KeyTable;

const APP_MENU_WIDTH: f32 = 348.0;
const APP_MENU_ROW_HEIGHT: f32 = 32.0;
const APP_MENU_SEP_HEIGHT: f32 = 11.0;
const APP_MENU_MAX_HEIGHT: f32 = 640.0;

fn is_app_menu_selectable(items: &[ResolvedMenuItem], idx: usize) -> bool {
    let Some(item) = items.get(idx) else {
        return false;
    };
    if item.is_separator() || !item.enabled {
        return false;
    }
    !matches!(&item.kind, MenuEntryKind::Widget { widget_id } if widget_id == ZOOM_CONTROLS_WIDGET_ID)
}

fn estimate_app_menu_height(items: &[ResolvedMenuItem]) -> f32 {
    let mut height = 14.0;
    for item in items {
        height += if item.is_separator() {
            APP_MENU_SEP_HEIGHT
        } else {
            APP_MENU_ROW_HEIGHT
        };
    }
    height
}

fn row_y_offset(items: &[ResolvedMenuItem], target_index: usize) -> f32 {
    let mut y = 7.0;
    for (idx, item) in items.iter().enumerate() {
        if idx == target_index {
            return y;
        }
        y += if item.is_separator() {
            APP_MENU_SEP_HEIGHT
        } else {
            APP_MENU_ROW_HEIGHT
        };
    }
    y
}

fn app_menu_panel_bg(theme: &ResolvedTheme) -> Color {
    if theme.scheme.is_dark() {
        Color::from_rgb(0x1f, 0x1f, 0x1f)
    } else {
        Color::from_rgb(0xf4, 0xf4, 0xf4)
    }
}

fn app_menu_border_color(theme: &ResolvedTheme) -> Color {
    if theme.scheme.is_dark() {
        Color::from_rgb(0x36, 0x36, 0x36)
    } else {
        Color::from_rgb(0xd4, 0xd4, 0xd4)
    }
}

fn app_menu_separator_color(theme: &ResolvedTheme) -> Color {
    if theme.scheme.is_dark() {
        Color::from_argb(26, 255, 255, 255)
    } else {
        Color::from_argb(26, 0, 0, 0)
    }
}

fn app_menu_hover_bg(theme: &ResolvedTheme) -> Color {
    if theme.scheme.is_dark() {
        Color::from_argb(22, 255, 255, 255)
    } else {
        Color::from_argb(16, 0, 0, 0)
    }
}

/// Overlay host rendering the animated, anchored application menu when open in [`MenuViewState`].
#[derive(Clone, PartialEq)]
pub struct AppMenuOverlay;

impl Component for AppMenuOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let menu_state = use_menu_state();
        let key_table = use_hook(KeyTable::standard);
        let is_open = *menu_state.app_menu_open.read();
        let anchor = *menu_state.app_menu_anchor.read();
        let mut open_seq = use_state(|| 0u64);
        let mut was_open = use_state(|| is_open);
        if *was_open.peek() != is_open {
            was_open.set(is_open);
            if is_open {
                let next = open_seq.peek().wrapping_add(1);
                open_seq.set(next);
            }
        }

        let (items, zoom_level) = {
            let model = core.model().read();
            let resolved = resolve_contributed_menu(
                &model.contributions.menu_entries,
                MenuLocation::AppMenu,
                Some(&key_table),
                APP_MENU_GROUPS,
            );
            (resolved, model.zoom_level)
        };

        let root_size = *Platform::get().current_window().root_size.read();
        let window_size = (root_size.width.max(320.0), root_size.height.max(240.0));
        let max_h = max_popup_height(window_size.1, 12.0, APP_MENU_MAX_HEIGHT);
        let est_height = estimate_app_menu_height(&items).min(max_h);
        let origin = compute_app_menu_origin(anchor, (APP_MENU_WIDTH, est_height), window_size);
        let seq = *open_seq.read();

        PopupPresence::anchored(is_open, origin)
            .open_seq(seq)
            .on_backdrop_mouse_down(move |_| {
                menu_state.close_app_menu();
            })
            .child(
                AppMenuInteractive::new(items, zoom_level)
                    .origin(origin)
                    .window_size(window_size)
                    .stagger_trigger(seq)
                    .capture_global_keys(is_open)
                    .on_close(move |_| menu_state.close_app_menu()),
            )
    }
}

/// Interactive Zen-styled application menu panel with keyboard navigation and cascading submenu support.
///
/// Used both by [`AppMenuOverlay`] (anchored to the toolbar button) and by the gallery (inline or anchored preview).
#[derive(Clone, PartialEq)]
pub struct AppMenuInteractive {
    items: Vec<ResolvedMenuItem>,
    zoom_level: f32,
    origin: Option<(f32, f32)>,
    window_size: (f32, f32),
    initial_open_submenu: Option<String>,
    stagger_trigger: u64,
    capture_global_keys: bool,
    on_close: Option<EventHandler<()>>,
    on_toggle: Option<EventHandler<(String, bool)>>,
    key: DiffKey,
}

impl KeyExt for AppMenuInteractive {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl AppMenuInteractive {
    pub fn new(items: Vec<ResolvedMenuItem>, zoom_level: f32) -> Self {
        Self {
            items,
            zoom_level,
            origin: None,
            window_size: (1600.0, 1000.0),
            initial_open_submenu: None,
            stagger_trigger: 0,
            capture_global_keys: false,
            on_close: None,
            on_toggle: None,
            key: DiffKey::None,
        }
    }

    pub fn origin(mut self, origin: (f32, f32)) -> Self {
        self.origin = Some(origin);
        self
    }

    pub fn window_size(mut self, window_size: (f32, f32)) -> Self {
        self.window_size = window_size;
        self
    }

    pub fn initial_open_submenu(mut self, id: impl Into<String>) -> Self {
        self.initial_open_submenu = Some(id.into());
        self
    }

    pub fn stagger_trigger(mut self, trigger: u64) -> Self {
        self.stagger_trigger = trigger;
        self
    }

    pub fn capture_global_keys(mut self, capture: bool) -> Self {
        self.capture_global_keys = capture;
        self
    }

    pub fn on_close(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_close = Some(handler.into());
        self
    }

    pub fn on_toggle(mut self, handler: impl Into<EventHandler<(String, bool)>>) -> Self {
        self.on_toggle = Some(handler.into());
        self
    }
}

impl Component for AppMenuInteractive {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let core = use_core();

        let initial_submenu_idx = self
            .initial_open_submenu
            .as_deref()
            .and_then(|id| self.items.iter().position(|item| item.id.as_str() == id));

        let mut active_index = use_state(move || initial_submenu_idx);
        let mut open_submenu_index = use_state(move || initial_submenu_idx);
        let mut submenu_active_index = use_state(|| None::<usize>);
        let mut typeahead = use_state(TypeaheadState::default);
        let mut main_scroll = use_scroll_controller(ScrollConfig::default);
        let mut sub_scroll = use_scroll_controller(ScrollConfig::default);

        let max_h = max_popup_height(self.window_size.1, 12.0, APP_MENU_MAX_HEIGHT);
        let raw_main_height = estimate_app_menu_height(&self.items);
        let main_scrolls = raw_main_height > max_h;
        let main_viewport_h = (max_h - 12.0).max(80.0);

        let items_for_keys = self.items.clone();
        let core_for_keys = core.clone();
        let on_close_keys = self.on_close.clone();
        let on_toggle_keys = self.on_toggle.clone();
        let capture_keys = self.capture_global_keys;

        let handle_key = move |e: Event<KeyboardEventData>| {
            if !capture_keys {
                return;
            }
            let Some(action) = classify_list_key(&e, ListNavConfig::menu()) else {
                return;
            };
            e.stop_propagation();
            e.prevent_default();

            let active_sub = *open_submenu_index.peek();
            match action {
                ListNavAction::Dismiss => {
                    typeahead.write().clear();
                    if active_sub.is_some() {
                        open_submenu_index.set(None);
                        submenu_active_index.set(None);
                    } else if let Some(close) = &on_close_keys {
                        close.call(());
                    }
                }
                ListNavAction::MoveNext => {
                    if let Some(sub_idx) = active_sub
                        && let Some(parent) = items_for_keys.get(sub_idx)
                    {
                        let next = step_selectable(parent.children.len(), *submenu_active_index.peek(), 1, true, |i| {
                            is_app_menu_selectable(&parent.children, i)
                        });
                        submenu_active_index.set(next);
                        if let Some(i) = next {
                            let sub_content = estimate_app_menu_height(&parent.children) - 12.0;
                            scroll_item_into_view(
                                &mut sub_scroll,
                                row_y_offset(&parent.children, i) - 7.0,
                                APP_MENU_ROW_HEIGHT,
                                main_viewport_h,
                                sub_content,
                            );
                        }
                    } else {
                        let next = step_selectable(items_for_keys.len(), *active_index.peek(), 1, true, |i| {
                            is_app_menu_selectable(&items_for_keys, i)
                        });
                        active_index.set(next);
                        if let Some(i) = next {
                            scroll_item_into_view(
                                &mut main_scroll,
                                row_y_offset(&items_for_keys, i) - 7.0,
                                APP_MENU_ROW_HEIGHT,
                                main_viewport_h,
                                raw_main_height - 12.0,
                            );
                        }
                    }
                }
                ListNavAction::MovePrev => {
                    if let Some(sub_idx) = active_sub
                        && let Some(parent) = items_for_keys.get(sub_idx)
                    {
                        let next =
                            step_selectable(parent.children.len(), *submenu_active_index.peek(), -1, true, |i| {
                                is_app_menu_selectable(&parent.children, i)
                            });
                        submenu_active_index.set(next);
                        if let Some(i) = next {
                            let sub_content = estimate_app_menu_height(&parent.children) - 12.0;
                            scroll_item_into_view(
                                &mut sub_scroll,
                                row_y_offset(&parent.children, i) - 7.0,
                                APP_MENU_ROW_HEIGHT,
                                main_viewport_h,
                                sub_content,
                            );
                        }
                    } else {
                        let next = step_selectable(items_for_keys.len(), *active_index.peek(), -1, true, |i| {
                            is_app_menu_selectable(&items_for_keys, i)
                        });
                        active_index.set(next);
                        if let Some(i) = next {
                            scroll_item_into_view(
                                &mut main_scroll,
                                row_y_offset(&items_for_keys, i) - 7.0,
                                APP_MENU_ROW_HEIGHT,
                                main_viewport_h,
                                raw_main_height - 12.0,
                            );
                        }
                    }
                }
                ListNavAction::PageDown(page) | ListNavAction::PageUp(page) => {
                    let forward = matches!(action, ListNavAction::PageDown(_));
                    if let Some(sub_idx) = active_sub
                        && let Some(parent) = items_for_keys.get(sub_idx)
                    {
                        let next = page_step_selectable(
                            parent.children.len(),
                            *submenu_active_index.peek(),
                            page,
                            forward,
                            |i| is_app_menu_selectable(&parent.children, i),
                        );
                        submenu_active_index.set(next);
                        if let Some(i) = next {
                            let sub_content = estimate_app_menu_height(&parent.children) - 12.0;
                            scroll_item_into_view(
                                &mut sub_scroll,
                                row_y_offset(&parent.children, i) - 7.0,
                                APP_MENU_ROW_HEIGHT,
                                main_viewport_h,
                                sub_content,
                            );
                        }
                    } else {
                        let next =
                            page_step_selectable(items_for_keys.len(), *active_index.peek(), page, forward, |i| {
                                is_app_menu_selectable(&items_for_keys, i)
                            });
                        active_index.set(next);
                        if let Some(i) = next {
                            scroll_item_into_view(
                                &mut main_scroll,
                                row_y_offset(&items_for_keys, i) - 7.0,
                                APP_MENU_ROW_HEIGHT,
                                main_viewport_h,
                                raw_main_height - 12.0,
                            );
                        }
                    }
                }
                ListNavAction::First => {
                    if let Some(sub_idx) = active_sub
                        && let Some(parent) = items_for_keys.get(sub_idx)
                    {
                        let next =
                            first_selectable(parent.children.len(), |i| is_app_menu_selectable(&parent.children, i));
                        submenu_active_index.set(next);
                        sub_scroll.scroll_to_y(0);
                    } else {
                        let next =
                            first_selectable(items_for_keys.len(), |i| is_app_menu_selectable(&items_for_keys, i));
                        active_index.set(next);
                        main_scroll.scroll_to_y(0);
                    }
                }
                ListNavAction::Last => {
                    if let Some(sub_idx) = active_sub
                        && let Some(parent) = items_for_keys.get(sub_idx)
                    {
                        let next =
                            last_selectable(parent.children.len(), |i| is_app_menu_selectable(&parent.children, i));
                        submenu_active_index.set(next);
                        if let Some(i) = next {
                            let sub_content = estimate_app_menu_height(&parent.children) - 12.0;
                            scroll_item_into_view(
                                &mut sub_scroll,
                                row_y_offset(&parent.children, i) - 7.0,
                                APP_MENU_ROW_HEIGHT,
                                main_viewport_h,
                                sub_content,
                            );
                        }
                    } else {
                        let next =
                            last_selectable(items_for_keys.len(), |i| is_app_menu_selectable(&items_for_keys, i));
                        active_index.set(next);
                        if let Some(i) = next {
                            scroll_item_into_view(
                                &mut main_scroll,
                                row_y_offset(&items_for_keys, i) - 7.0,
                                APP_MENU_ROW_HEIGHT,
                                main_viewport_h,
                                raw_main_height - 12.0,
                            );
                        }
                    }
                }
                ListNavAction::OpenSubmenu => {
                    if active_sub.is_none()
                        && let Some(idx) = *active_index.peek()
                        && let Some(item) = items_for_keys.get(idx)
                        && item.is_submenu()
                    {
                        open_submenu_index.set(Some(idx));
                        submenu_active_index.set(first_selectable(item.children.len(), |i| {
                            is_app_menu_selectable(&item.children, i)
                        }));
                    }
                }
                ListNavAction::CloseSubmenu => {
                    if active_sub.is_some() {
                        open_submenu_index.set(None);
                        submenu_active_index.set(None);
                    }
                }
                ListNavAction::Typeahead(ch) => {
                    let now = Instant::now();
                    if let Some(sub_idx) = active_sub
                        && let Some(parent) = items_for_keys.get(sub_idx)
                    {
                        let found = typeahead.write().push_and_find(
                            ch,
                            now,
                            parent.children.len(),
                            *submenu_active_index.peek(),
                            |i| parent.children.get(i).map(|it| it.label.clone()),
                            |i| is_app_menu_selectable(&parent.children, i),
                        );
                        if let Some(i) = found {
                            submenu_active_index.set(Some(i));
                            let sub_content = estimate_app_menu_height(&parent.children) - 12.0;
                            scroll_item_into_view(
                                &mut sub_scroll,
                                row_y_offset(&parent.children, i) - 7.0,
                                APP_MENU_ROW_HEIGHT,
                                main_viewport_h,
                                sub_content,
                            );
                        }
                    } else {
                        let found = typeahead.write().push_and_find(
                            ch,
                            now,
                            items_for_keys.len(),
                            *active_index.peek(),
                            |i| items_for_keys.get(i).map(|it| it.label.clone()),
                            |i| is_app_menu_selectable(&items_for_keys, i),
                        );
                        if let Some(i) = found {
                            active_index.set(Some(i));
                            scroll_item_into_view(
                                &mut main_scroll,
                                row_y_offset(&items_for_keys, i) - 7.0,
                                APP_MENU_ROW_HEIGHT,
                                main_viewport_h,
                                raw_main_height - 12.0,
                            );
                        }
                    }
                }
                ListNavAction::Activate => {
                    if let Some(sub_idx) = active_sub
                        && let Some(parent) = items_for_keys.get(sub_idx)
                        && let Some(child_idx) = *submenu_active_index.peek()
                        && let Some(child) = parent.children.get(child_idx)
                        && child.enabled
                    {
                        invoke_menu_command(&core_for_keys, &child.command, &child.command_args);
                        if !child.keeps_menu_open
                            && let Some(close) = &on_close_keys
                        {
                            close.call(());
                        }
                    } else if let Some(idx) = *active_index.peek()
                        && let Some(item) = items_for_keys.get(idx)
                        && is_app_menu_selectable(&items_for_keys, idx)
                    {
                        if item.is_submenu() {
                            open_submenu_index.set(Some(idx));
                            submenu_active_index.set(first_selectable(item.children.len(), |i| {
                                is_app_menu_selectable(&item.children, i)
                            }));
                        } else {
                            invoke_menu_command(&core_for_keys, &item.command, &item.command_args);
                            if let MenuEntryKind::Toggle { checked } = &item.kind
                                && let Some(cb) = &on_toggle_keys
                            {
                                cb.call((item.command.clone(), !*checked));
                            }
                            if !item.keeps_menu_open
                                && let Some(close) = &on_close_keys
                            {
                                close.call(());
                            }
                        }
                    }
                }
            }
        };

        let current_active = *active_index.read();
        let current_submenu = *open_submenu_index.read();
        let current_sub_active = *submenu_active_index.read();
        let stagger_trigger = self.stagger_trigger;
        let back_fb = use_interactive_feedback(true);

        // APP-03: In-panel PanelMultiView slide transition between main view and active subview.
        let subview_slide = use_animation_with_dependencies(&current_submenu, move |conf, sub| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let from = if sub.is_some() { 18.0 } else { -12.0 };
            AnimNum::new(from, 0.0)
                .duration(motion::SUBMENU_HOVER_INTENT)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let slide_x = subview_slide.get().value();

        let active_sub_parent = current_submenu
            .and_then(|idx| self.items.get(idx))
            .filter(|parent| !parent.children.is_empty());

        let panel_body: Element = if let Some(parent) = active_sub_parent {
            let raw_sub_h = estimate_app_menu_height(&parent.children) + 42.0;
            let sub_scrolls = raw_sub_h > max_h;
            let sub_viewport_h = (max_h - 54.0).max(80.0);
            let sub_rows: Vec<Element> = parent
                .children
                .iter()
                .enumerate()
                .map(|(sub_idx, child)| {
                    if child.is_separator() {
                        return app_menu_separator(&theme, sub_idx);
                    }
                    let enabled = child.enabled;
                    let keeps_menu_open = child.keeps_menu_open;
                    let command = child.command.clone();
                    let command_args = child.command_args.clone();
                    let core_sub = core.clone();
                    let on_close_sub = self.on_close.clone();
                    let sub_highlighted = current_sub_active == Some(sub_idx);
                    app_menu_row(
                        &theme,
                        &core,
                        child,
                        sub_idx,
                        sub_highlighted,
                        None,
                        move |_| {
                            submenu_active_index.set(Some(sub_idx));
                        },
                        move |_| {
                            if !enabled {
                                return;
                            }
                            invoke_menu_command(&core_sub, &command, &command_args);
                            if !keeps_menu_open && let Some(close) = &on_close_sub {
                                close.call(());
                            }
                        },
                    )
                })
                .collect();

            let back_btn = rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Back to main menu")
                .width(Size::px(28.0))
                .height(Size::px(28.0))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(back_fb.background(
                    Color::TRANSPARENT,
                    theme.color(ThemeRole::Hover),
                    theme.color(ThemeRole::Hover),
                ))
                .scale(back_fb.scale())
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(back_fb.on_pointer_enter())
                .on_pointer_leave(back_fb.on_pointer_leave())
                .on_pointer_down(back_fb.on_pointer_down())
                .on_global_pointer_up(back_fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    open_submenu_index.set(None);
                    submenu_active_index.set(None);
                })
                .child(Icon::new("back").size(14.0).role(ThemeRole::Text));

            let sub_header = rect()
                .width(Size::fill())
                .height(Size::px(34.0))
                .padding(Gaps::new(0.0, 6.0, 0.0, 4.0))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .cross_align(Alignment::center())
                .spacing(6.0)
                .child(back_btn)
                .child(
                    label()
                        .text(parent.label.clone())
                        .width(Size::flex(1.0))
                        .text_align(TextAlign::Center)
                        .font_size(theme.font_sizes.md)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(theme.color(ThemeRole::Text))
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
                .child(rect().width(Size::px(28.0)));

            let sub_list: Element = if sub_scrolls {
                ScrollView::new_controlled(sub_scroll)
                    .width(Size::fill())
                    .height(Size::px(sub_viewport_h))
                    .show_scrollbar(false)
                    .children(sub_rows)
                    .into_element()
            } else {
                rect().width(Size::fill()).children(sub_rows).into_element()
            };

            rect()
                .width(Size::fill())
                .offset_x(slide_x)
                .child(sub_header)
                .child(app_menu_separator(&theme, 9999))
                .child(sub_list)
                .into_element()
        } else {
            let rows: Vec<Element> = self
                .items
                .iter()
                .enumerate()
                .map(|(idx, item)| {
                    let row_el = if item.is_separator() {
                        app_menu_separator(&theme, idx)
                    } else if let MenuEntryKind::Widget { widget_id } = &item.kind
                        && widget_id == ZOOM_CONTROLS_WIDGET_ID
                    {
                        app_menu_zoom_row(&theme, &core, self.zoom_level, idx)
                    } else {
                        let highlighted = current_active == Some(idx);
                        let is_submenu = item.is_submenu();
                        let enabled = item.enabled;
                        let keeps_menu_open = item.keeps_menu_open;
                        let command = item.command.clone();
                        let command_args = item.command_args.clone();
                        let toggle_checked = match &item.kind {
                            MenuEntryKind::Toggle { checked } => Some(*checked),
                            _ => None,
                        };
                        let core_press = core.clone();
                        let on_close_press = self.on_close.clone();
                        let on_toggle_row = self.on_toggle.clone();
                        let on_toggle_switch = self.on_toggle.clone();

                        app_menu_row(
                            &theme,
                            &core,
                            item,
                            idx,
                            highlighted,
                            on_toggle_switch,
                            move |_| {
                                active_index.set(Some(idx));
                            },
                            move |_| {
                                if !enabled {
                                    return;
                                }
                                if is_submenu {
                                    open_submenu_index.set(Some(idx));
                                    submenu_active_index.set(None);
                                } else {
                                    invoke_menu_command(&core_press, &command, &command_args);
                                    if let Some(was_checked) = toggle_checked
                                        && let Some(cb) = &on_toggle_row
                                    {
                                        cb.call((command.clone(), !was_checked));
                                    }
                                    if !keeps_menu_open && let Some(close) = &on_close_press {
                                        close.call(());
                                    }
                                }
                            },
                        )
                    };
                    if stagger_trigger > 0 {
                        StaggeredRow::new(idx)
                            .trigger(stagger_trigger)
                            .key(("stagger", idx))
                            .child(row_el)
                            .into_element()
                    } else {
                        row_el
                    }
                })
                .collect();

            let main_list: Element = if main_scrolls {
                ScrollView::new_controlled(main_scroll)
                    .width(Size::fill())
                    .height(Size::px(main_viewport_h))
                    .show_scrollbar(false)
                    .children(rows)
                    .into_element()
            } else {
                PopupContent::new(stagger_trigger).children(rows).into_element()
            };

            rect()
                .width(Size::fill())
                .offset_x(slide_x)
                .child(main_list)
                .into_element()
        };

        rect()
            .a11y_role(AccessibilityRole::Menu)
            .width(Size::px(APP_MENU_WIDTH))
            .max_height(Size::px(max_h))
            .overflow(Overflow::Clip)
            .padding(Gaps::new(6.0, 6.0, 6.0, 6.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(app_menu_panel_bg(&theme))
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(app_menu_border_color(&theme)),
            )
            .shadow(theme.shadows.float_drop.to_shadow())
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .on_global_key_down(handle_key)
            .child(panel_body)
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn app_menu_separator(theme: &ResolvedTheme, key: usize) -> Element {
    rect()
        .key(key)
        .width(Size::fill())
        .padding(Gaps::new(5.0, 6.0, 5.0, 6.0))
        .child(
            rect()
                .width(Size::fill())
                .height(Size::px(1.0))
                .background(app_menu_separator_color(theme)),
        )
        .into_element()
}

#[allow(clippy::too_many_arguments)]
fn app_menu_row(
    theme: &ResolvedTheme,
    core: &CoreView,
    item: &ResolvedMenuItem,
    key: usize,
    highlighted: bool,
    on_toggle: Option<EventHandler<(String, bool)>>,
    on_hover: impl FnMut(Event<PointerEventData>) + 'static,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let text_role = if item.enabled {
        ThemeRole::Text
    } else {
        ThemeRole::TextMuted
    };
    let bg = if highlighted && item.enabled {
        app_menu_hover_bg(theme)
    } else {
        Color::TRANSPARENT
    };

    let trailing: Option<Element> = match &item.kind {
        MenuEntryKind::Submenu => Some(
            Icon::new("arrow-right")
                .size(12.0)
                .role(ThemeRole::TextMuted)
                .into_element(),
        ),
        MenuEntryKind::Toggle { checked } => {
            let toggle_core = core.clone();
            let cmd = item.command.clone();
            Some(
                rect()
                    .on_press(|e: Event<PressEventData>| e.stop_propagation())
                    .child(
                        KitSwitch::new(*checked)
                            .disabled(!item.enabled)
                            .on_toggle(move |next: bool| {
                                invoke_menu_command(&toggle_core, &cmd, &[("enabled".to_string(), next.to_string())]);
                                if let Some(cb) = &on_toggle {
                                    cb.call((cmd.clone(), next));
                                }
                            }),
                    )
                    .into_element(),
            )
        }
        MenuEntryKind::Checkable { checked } => {
            if *checked {
                Some(Icon::new("check").size(13.0).role(ThemeRole::Accent).into_element())
            } else {
                None
            }
        }
        _ => item
            .shortcut_hint
            .as_ref()
            .filter(|_| item.id.as_str() != "zendia.app.settings")
            .map(|shortcut| {
                label()
                    .text(shortcut.clone())
                    .font_size(12.0)
                    .color(theme.color(ThemeRole::TextMuted))
                    .into_element()
            }),
    };

    let (a11y_role, a11y_alt, row_tooltip) = match &item.kind {
        MenuEntryKind::Toggle { checked } => {
            let tip = (item.command == "vim.set_enabled").then_some(if *checked {
                "Neovim-style keys drive the browser. Click to turn them off."
            } else {
                "Click to drive the browser with Neovim-style keys."
            });
            (
                AccessibilityRole::CheckBox,
                format!("{} ({})", item.label, if *checked { "on" } else { "off" }),
                tip,
            )
        }
        _ => (AccessibilityRole::MenuItem, item.label.clone(), None),
    };

    let row = rect()
        .a11y_role(a11y_role)
        .a11y_alt(a11y_alt)
        .width(Size::fill())
        .height(Size::px(APP_MENU_ROW_HEIGHT))
        .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
        .spacing(10.0)
        .direction(Direction::Horizontal)
        .content(Content::Flex)
        .cross_align(Alignment::center())
        .corner_radius(CornerRadius::new_all(6.0))
        .background(bg)
        .cursor(if item.enabled {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        })
        .on_pointer_enter(on_hover)
        .on_press(on_press)
        .child(
            rect()
                .width(Size::px(16.0))
                .height(Size::px(16.0))
                .center()
                .maybe_child(
                    item.icon
                        .as_ref()
                        .map(|icon| Icon::new(icon.clone()).size(15.0).role(text_role)),
                ),
        )
        .child(
            label()
                .text(item.label.clone())
                .width(Size::flex(1.0))
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .font_size(theme.font_sizes.md)
                .color(theme.color(text_role)),
        )
        .maybe_child(trailing);

    if let Some(tip) = row_tooltip {
        KitTooltip::new(tip).key(key).fill_width(true).child(row).into_element()
    } else {
        row.key(key).into_element()
    }
}

fn app_menu_zoom_row(theme: &ResolvedTheme, core: &CoreView, zoom_level: f32, key: usize) -> Element {
    let pct_text = format!("{}%", (zoom_level * 100.0).round() as i32);
    let out_core = core.clone();
    let reset_core = core.clone();
    let in_core = core.clone();
    let fs_core = core.clone();

    let zoom_btn = |text: &'static str, alt: &'static str, mut on_click: Box<dyn FnMut()>| {
        Button::new()
            .flat()
            .compact()
            .width(Size::px(26.0))
            .height(Size::px(26.0))
            .padding(Gaps::new_all(0.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(Color::TRANSPARENT)
            .hover_background(app_menu_hover_bg(theme))
            .border_fill(Color::TRANSPARENT)
            .color(theme.color(ThemeRole::Text))
            .cursor(CursorIcon::Pointer)
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_click();
            })
            .child(
                rect()
                    .a11y_alt(alt)
                    .center()
                    .child(label().text(text).font_size(15.0).font_weight(FontWeight::MEDIUM)),
            )
    };

    rect()
        .key(key)
        .width(Size::fill())
        .height(Size::px(34.0))
        .padding(Gaps::new(0.0, 6.0, 0.0, 36.0))
        .spacing(4.0)
        .direction(Direction::Horizontal)
        .content(Content::Flex)
        .cross_align(Alignment::center())
        .child(
            label()
                .text("Zoom")
                .width(Size::flex(1.0))
                .font_size(theme.font_sizes.md)
                .color(theme.color(ThemeRole::Text)),
        )
        .child(zoom_btn(
            "−",
            "Zoom out",
            Box::new(move || {
                out_core.invoke_or_log(Command::ZoomOut { tab_id: None });
            }),
        ))
        .child(
            Button::new()
                .flat()
                .compact()
                .width(Size::px(48.0))
                .height(Size::px(26.0))
                .padding(Gaps::new(0.0, 6.0, 0.0, 6.0))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(Color::TRANSPARENT)
                .hover_background(theme.color(ThemeRole::Hover))
                .border_fill(Color::TRANSPARENT)
                .color(theme.color(ThemeRole::Text))
                .cursor(CursorIcon::Pointer)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    reset_core.invoke_or_log(Command::ZoomReset { tab_id: None });
                })
                .child(
                    rect()
                        .a11y_alt("Reset zoom")
                        .center()
                        .child(label().text(pct_text).font_size(12.5).font_weight(FontWeight::MEDIUM)),
                ),
        )
        .child(zoom_btn(
            "+",
            "Zoom in",
            Box::new(move || {
                in_core.invoke_or_log(Command::ZoomIn { tab_id: None });
            }),
        ))
        .child(
            rect()
                .width(Size::px(1.0))
                .height(Size::px(18.0))
                .margin(Gaps::new(0.0, 4.0, 0.0, 4.0))
                .background(theme.color(ThemeRole::BorderSubtle)),
        )
        .child(
            Button::new()
                .flat()
                .compact()
                .width(Size::px(26.0))
                .height(Size::px(26.0))
                .padding(Gaps::new_all(0.0))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(Color::TRANSPARENT)
                .hover_background(theme.color(ThemeRole::Hover))
                .border_fill(Color::TRANSPARENT)
                .color(theme.color(ThemeRole::Text))
                .cursor(CursorIcon::Pointer)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    invoke_menu_command(&fs_core, "window.toggle_fullscreen", &[]);
                })
                .child(
                    rect()
                        .a11y_alt("Toggle fullscreen")
                        .center()
                        .child(Icon::new("fullscreen").size(14.0).role(ThemeRole::Text)),
                ),
        )
        .into_element()
}

/// Helper for callers (such as a future toolbar button) to compute a standard [`AnchorRect`] from a Freya [`Area`].
#[allow(dead_code)]
pub fn anchor_from_area(area: Area) -> AnchorRect {
    AnchorRect::from_area(area)
}
