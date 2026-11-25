/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Ladybird-styled native context menu component used for page, link, image, media, selected-text, and tab menus.

use crate::ui::chrome::menus::state::{
    clamp_popup_origin, compute_submenu_origin, invoke_menu_command, next_selectable_index, use_menu_state,
};
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::theme::POPUP_OVERLAY_LEVEL;
use crate::ui::theme::{ResolvedTheme, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::contributions::MenuEntryKind;
use zendia_browser_core::menus::ResolvedMenuItem;

pub(crate) const CTX_MENU_MIN_WIDTH: f32 = 216.0;
pub(crate) const CTX_MENU_MAX_WIDTH: f32 = 340.0;
pub(crate) const CTX_MENU_ROW_HEIGHT: f32 = 26.0;
pub(crate) const CTX_MENU_SEP_HEIGHT: f32 = 9.0;

pub(crate) fn native_menu_bg(theme: &ResolvedTheme) -> Color {
    if theme.scheme.is_dark() {
        theme
            .rgba(ThemeRole::Surface)
            .mix(theme.rgba(ThemeRole::SurfaceRaised), 0.5)
            .to_color()
    } else {
        theme.color(ThemeRole::Surface)
    }
}

pub(crate) fn native_menu_hover_bg(theme: &ResolvedTheme) -> Color {
    if theme.scheme.is_dark() {
        theme.rgba(ThemeRole::Accent).with_alpha(0.24).to_color()
    } else {
        theme.rgba(ThemeRole::Accent).with_alpha(0.16).to_color()
    }
}

fn estimate_menu_size(items: &[ResolvedMenuItem]) -> (f32, f32) {
    let mut height = 10.0;
    let mut max_chars = 18usize;
    for item in items {
        if item.is_separator() {
            height += CTX_MENU_SEP_HEIGHT;
        } else {
            height += CTX_MENU_ROW_HEIGHT;
            let extra = item.shortcut_hint.as_ref().map_or(0, |s| s.len() + 4)
                + item.badge.as_ref().map_or(0, |b| b.len() + 4)
                + usize::from(item.is_submenu()) * 3;
            max_chars = max_chars.max(item.label.chars().count() + extra);
        }
    }
    let width = ((max_chars as f32) * 7.2 + 44.0).clamp(CTX_MENU_MIN_WIDTH, CTX_MENU_MAX_WIDTH);
    (width, height)
}

fn row_offset(items: &[ResolvedMenuItem], target: usize) -> f32 {
    let mut y = 5.0;
    for (idx, item) in items.iter().enumerate() {
        if idx == target {
            return y;
        }
        y += if item.is_separator() {
            CTX_MENU_SEP_HEIGHT
        } else {
            CTX_MENU_ROW_HEIGHT
        };
    }
    y
}

fn menu_needs_gutter(items: &[ResolvedMenuItem]) -> bool {
    items
        .iter()
        .any(|item| matches!(item.kind, MenuEntryKind::Checkable { .. }) || item.icon.is_some())
}

/// Overlay host rendering the active context menu in [`MenuViewState`].
#[derive(Clone, PartialEq)]
pub struct ContextMenuOverlay;

impl Component for ContextMenuOverlay {
    fn render(&self) -> impl IntoElement {
        let menu_state = use_menu_state();
        let Some(open) = menu_state.context_menu.read().clone() else {
            return rect().into_element();
        };

        let root_size = *Platform::get().current_window().root_size.read();
        let window_size = (root_size.width.max(320.0), root_size.height.max(240.0));
        let est_size = estimate_menu_size(&open.items);
        let origin = clamp_popup_origin(open.position, est_size, window_size, 6.0);

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
                    .on_mouse_down(move |_| {
                        menu_state.close_context_menu();
                    }),
            )
            .child(
                ContextMenuCard::new(open.items)
                    .origin(origin)
                    .window_size(window_size)
                    .capture_global_keys(true)
                    .on_close(move |_| menu_state.close_context_menu()),
            )
            .into_element()
    }
}

/// Reusable Ladybird-styled native context menu with submenus and keyboard navigation.
#[derive(Clone, PartialEq)]
pub struct ContextMenuCard {
    items: Vec<ResolvedMenuItem>,
    origin: Option<(f32, f32)>,
    window_size: (f32, f32),
    initial_open_submenu: Option<String>,
    capture_global_keys: bool,
    on_close: Option<EventHandler<()>>,
    key: DiffKey,
}

impl KeyExt for ContextMenuCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ContextMenuCard {
    pub fn new(items: Vec<ResolvedMenuItem>) -> Self {
        Self {
            items,
            origin: None,
            window_size: (1600.0, 1000.0),
            initial_open_submenu: None,
            capture_global_keys: false,
            on_close: None,
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

    pub fn capture_global_keys(mut self, capture: bool) -> Self {
        self.capture_global_keys = capture;
        self
    }

    pub fn on_close(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_close = Some(handler.into());
        self
    }
}

impl Component for ContextMenuCard {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let core = use_core();

        let initial_sub = self
            .initial_open_submenu
            .as_deref()
            .and_then(|id| self.items.iter().position(|item| item.id.as_str() == id));

        let mut active_index = use_state(move || initial_sub);
        let mut open_submenu_index = use_state(move || initial_sub);
        let mut sub_active_index = use_state(|| None::<usize>);

        let items_for_keys = self.items.clone();
        let core_for_keys = core.clone();
        let on_close_keys = self.on_close.clone();
        let capture = self.capture_global_keys;

        let handle_key = move |e: Event<KeyboardEventData>| {
            if !capture {
                return;
            }
            match &e.key {
                Key::Named(NamedKey::Escape) => {
                    e.stop_propagation();
                    if open_submenu_index.peek().is_some() {
                        open_submenu_index.set(None);
                        sub_active_index.set(None);
                    } else if let Some(close) = &on_close_keys {
                        close.call(());
                    }
                }
                Key::Named(NamedKey::ArrowDown) => {
                    e.stop_propagation();
                    if let Some(sub_idx) = *open_submenu_index.peek()
                        && let Some(parent) = items_for_keys.get(sub_idx)
                    {
                        let next = next_selectable_index(&parent.children, *sub_active_index.peek(), 1);
                        sub_active_index.set(next);
                    } else {
                        let next = next_selectable_index(&items_for_keys, *active_index.peek(), 1);
                        active_index.set(next);
                    }
                }
                Key::Named(NamedKey::ArrowUp) => {
                    e.stop_propagation();
                    if let Some(sub_idx) = *open_submenu_index.peek()
                        && let Some(parent) = items_for_keys.get(sub_idx)
                    {
                        let next = next_selectable_index(&parent.children, *sub_active_index.peek(), -1);
                        sub_active_index.set(next);
                    } else {
                        let next = next_selectable_index(&items_for_keys, *active_index.peek(), -1);
                        active_index.set(next);
                    }
                }
                Key::Named(NamedKey::ArrowRight) => {
                    e.stop_propagation();
                    if open_submenu_index.peek().is_none()
                        && let Some(idx) = *active_index.peek()
                        && let Some(item) = items_for_keys.get(idx)
                        && item.is_submenu()
                    {
                        open_submenu_index.set(Some(idx));
                        sub_active_index.set(next_selectable_index(&item.children, None, 1));
                    }
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    e.stop_propagation();
                    if open_submenu_index.peek().is_some() {
                        open_submenu_index.set(None);
                        sub_active_index.set(None);
                    }
                }
                Key::Named(NamedKey::Enter) => {
                    e.stop_propagation();
                    if let Some(sub_idx) = *open_submenu_index.peek()
                        && let Some(parent) = items_for_keys.get(sub_idx)
                        && let Some(child_idx) = *sub_active_index.peek()
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
                        && item.enabled
                    {
                        if item.is_submenu() {
                            open_submenu_index.set(Some(idx));
                            sub_active_index.set(next_selectable_index(&item.children, None, 1));
                        } else {
                            invoke_menu_command(&core_for_keys, &item.command, &item.command_args);
                            if !item.keeps_menu_open
                                && let Some(close) = &on_close_keys
                            {
                                close.call(());
                            }
                        }
                    }
                }
                _ => {}
            }
        };

        let (est_width, _) = estimate_menu_size(&self.items);
        let has_gutter = menu_needs_gutter(&self.items);
        let cur_active = *active_index.read();
        let cur_sub = *open_submenu_index.read();
        let cur_sub_active = *sub_active_index.read();

        let rows = self.items.iter().enumerate().map(|(idx, item)| {
            if item.is_separator() {
                return context_menu_separator(&theme, idx);
            }
            let highlighted = cur_active == Some(idx) || cur_sub == Some(idx);
            let is_sub = item.is_submenu();
            let item_cmd = item.command.clone();
            let item_args = item.command_args.clone();
            let item_enabled = item.enabled;
            let keeps_open = item.keeps_menu_open;
            let core_press = core.clone();
            let on_close_press = self.on_close.clone();

            context_menu_row(
                &theme,
                item,
                idx,
                has_gutter,
                highlighted,
                move |_| {
                    active_index.set(Some(idx));
                    if is_sub {
                        open_submenu_index.set(Some(idx));
                        sub_active_index.set(None);
                    } else {
                        open_submenu_index.set(None);
                        sub_active_index.set(None);
                    }
                },
                move |_| {
                    if !item_enabled {
                        return;
                    }
                    if is_sub {
                        let next = if *open_submenu_index.peek() == Some(idx) {
                            None
                        } else {
                            Some(idx)
                        };
                        open_submenu_index.set(next);
                        sub_active_index.set(None);
                    } else {
                        invoke_menu_command(&core_press, &item_cmd, &item_args);
                        if !keeps_open && let Some(close) = &on_close_press {
                            close.call(());
                        }
                    }
                },
            )
        });

        let main_card = rect()
            .a11y_role(AccessibilityRole::Menu)
            .width(Size::px(est_width))
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
            .children(rows);

        let submenu = cur_sub
            .and_then(|idx| self.items.get(idx).map(|parent| (idx, parent)))
            .filter(|(_, parent)| !parent.children.is_empty())
            .map(|(parent_idx, parent)| {
                let (sub_w, sub_h) = estimate_menu_size(&parent.children);
                let sub_gutter = menu_needs_gutter(&parent.children);
                let sub_rows = parent.children.iter().enumerate().map(|(sub_idx, child)| {
                    if child.is_separator() {
                        return context_menu_separator(&theme, sub_idx);
                    }
                    let cmd = child.command.clone();
                    let args = child.command_args.clone();
                    let enabled = child.enabled;
                    let keeps_open = child.keeps_menu_open;
                    let core_sub = core.clone();
                    let on_close_sub = self.on_close.clone();

                    context_menu_row(
                        &theme,
                        child,
                        sub_idx,
                        sub_gutter,
                        cur_sub_active == Some(sub_idx),
                        move |_| {
                            sub_active_index.set(Some(sub_idx));
                        },
                        move |_| {
                            if !enabled {
                                return;
                            }
                            invoke_menu_command(&core_sub, &cmd, &args);
                            if !keeps_open && let Some(close) = &on_close_sub {
                                close.call(());
                            }
                        },
                    )
                });

                let card = rect()
                    .a11y_role(AccessibilityRole::Menu)
                    .width(Size::px(sub_w))
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
                    .children(sub_rows);

                let y_off = row_offset(&self.items, parent_idx);
                (card, y_off, (sub_w, sub_h))
            });

        if let Some((ox, oy)) = self.origin {
            let positioned_sub = submenu.map(|(card, y_off, sub_size)| {
                let (sx, sy) = compute_submenu_origin((ox, oy), est_width, y_off, sub_size, self.window_size);
                rect().position(Position::new_global().left(sx).top(sy)).child(card)
            });
            rect()
                .on_global_key_down(handle_key)
                .child(
                    rect()
                        .position(Position::new_global().left(ox).top(oy))
                        .child(main_card),
                )
                .maybe_child(positioned_sub)
                .into_element()
        } else {
            rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::Start)
                .on_global_key_down(handle_key)
                .child(main_card)
                .maybe_child(submenu.map(|(card, y_off, _)| {
                    rect()
                        .margin(Gaps::new((y_off - 4.0).max(0.0), 0.0, 0.0, 2.0))
                        .child(card)
                }))
                .into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

pub(crate) fn context_menu_separator(theme: &ResolvedTheme, key: usize) -> Element {
    rect()
        .key(key)
        .width(Size::fill())
        .padding(Gaps::new(4.0, 4.0, 4.0, 4.0))
        .child(
            rect()
                .width(Size::fill())
                .height(Size::px(1.0))
                .background(theme.color(ThemeRole::BorderSubtle)),
        )
        .into_element()
}

fn context_menu_row(
    theme: &ResolvedTheme,
    item: &ResolvedMenuItem,
    key: usize,
    show_gutter: bool,
    highlighted: bool,
    on_hover: impl FnMut(Event<PointerEventData>) + 'static,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let text_color = if item.enabled {
        theme.color(ThemeRole::Text)
    } else {
        theme.rgba(ThemeRole::TextMuted).with_alpha(0.55).to_color()
    };
    let bg = if highlighted && item.enabled {
        native_menu_hover_bg(theme)
    } else {
        Color::TRANSPARENT
    };

    let gutter_el: Option<Element> = if show_gutter {
        let inner: Option<Element> = match &item.kind {
            MenuEntryKind::Checkable { checked: true } => Some(
                Icon::new("check")
                    .size(13.0)
                    .role(if item.enabled {
                        ThemeRole::Text
                    } else {
                        ThemeRole::TextMuted
                    })
                    .into_element(),
            ),
            _ => item.icon.as_ref().map(|icon| {
                Icon::new(icon.clone())
                    .size(14.0)
                    .role(if item.enabled {
                        ThemeRole::Text
                    } else {
                        ThemeRole::TextMuted
                    })
                    .into_element()
            }),
        };
        Some(
            rect()
                .width(Size::px(16.0))
                .height(Size::px(16.0))
                .center()
                .maybe_child(inner)
                .into_element(),
        )
    } else {
        None
    };

    let badge_el = item.badge.as_ref().map(|badge| {
        rect()
            .padding(Gaps::new(1.0, 6.0, 1.0, 6.0))
            .corner_radius(CornerRadius::new_all(4.0))
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::Border)),
            )
            .child(
                label()
                    .text(badge.clone())
                    .font_size(11.0)
                    .color(theme.color(ThemeRole::TextMuted)),
            )
    });

    let shortcut_el = item.shortcut_hint.as_ref().map(|shortcut| {
        label()
            .text(shortcut.clone())
            .font_size(11.5)
            .color(theme.color(ThemeRole::TextMuted))
    });

    let submenu_arrow = item
        .is_submenu()
        .then(|| Icon::new("arrow-right").size(11.0).role(ThemeRole::TextMuted));

    rect()
        .key(key)
        .a11y_role(AccessibilityRole::MenuItem)
        .a11y_alt(item.label.clone())
        .width(Size::fill())
        .height(Size::px(CTX_MENU_ROW_HEIGHT))
        .padding(Gaps::new(0.0, 10.0, 0.0, 8.0))
        .spacing(8.0)
        .direction(Direction::Horizontal)
        .content(Content::Flex)
        .cross_align(Alignment::center())
        .corner_radius(CornerRadius::new_all(4.0))
        .background(bg)
        .cursor(if item.enabled {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        })
        .on_pointer_enter(on_hover)
        .on_press(on_press)
        .maybe_child(gutter_el)
        .child(
            label()
                .text(item.label.clone())
                .width(Size::flex(1.0))
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .font_size(12.5)
                .color(text_color),
        )
        .maybe_child(badge_el)
        .maybe_child(shortcut_el)
        .maybe_child(submenu_arrow)
        .into_element()
}
