/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Ladybird-styled native context menu component used for page, link, image, media, selected-text, and tab menus.
//!
//! Supports arbitrary-depth nested submenus, max-height scrolling when taller than the viewport, and standard keyboard
//! navigation (`ArrowUp`/`ArrowDown`/`ArrowLeft`/`ArrowRight`, `Home`/`End`, `PageUp`/`PageDown`, `Enter`, `Escape`,
//! and type-to-jump).

use crate::ui::chrome::menus::state::{
    clamp_popup_origin, compute_submenu_origin, invoke_menu_command, use_menu_state,
};
use crate::ui::core_view::use_core;
use crate::ui::kit::{
    Icon, ListNavAction, ListNavConfig, TypeaheadState, classify_list_key, first_selectable, max_popup_height,
    moved_index, scroll_item_into_view,
};
use crate::ui::page::PageSurfaceContext;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedTheme, ThemeRole, motion, use_theme};
use crate::ui::webchat::chat_site_logo;
use freya::animation::*;
use freya::prelude::*;
use hub_protocol::{Web_DevicePixelPoint, Web_UIEvents_MouseButton as WebMouseButton};
use zendia_browser_core::contributions::MenuEntryKind;
use zendia_browser_core::menus::{
    ASK_ALL_SITE_ID, ResolvedMenuItem, ask_language_pick, ask_site_pick, relabel_ask_language, retarget_ask_menu,
    retarget_ask_menu_all,
};
use zendia_browser_core::model::ChatSiteId;

pub(crate) const CTX_MENU_MIN_WIDTH: f32 = 216.0;
pub(crate) const CTX_MENU_MAX_WIDTH: f32 = 340.0;
pub(crate) const CTX_MENU_ROW_HEIGHT: f32 = 26.0;
pub(crate) const CTX_MENU_SEP_HEIGHT: f32 = 9.0;
const CTX_MENU_MAX_HEIGHT: f32 = 520.0;
const CTX_MENU_CHROME_HEIGHT: f32 = 10.0;
/// Height of the row of chat site logos at the top of an Ask submenu.
const ASK_SITE_ROW_HEIGHT: f32 = 34.0;
/// Width each logo takes in that row, spacing included.
const ASK_SITE_BUTTON_STRIDE: f32 = 28.0;

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

fn is_menu_item_selectable(items: &[ResolvedMenuItem], idx: usize) -> bool {
    items.get(idx).is_some_and(|item| !item.is_separator() && item.enabled)
}

fn estimate_menu_size(items: &[ResolvedMenuItem]) -> (f32, f32) {
    let mut height = CTX_MENU_CHROME_HEIGHT;
    let mut max_chars = 18usize;
    let site_row_len = ask_site_row_len(items);
    if site_row_len > 0 {
        height += ASK_SITE_ROW_HEIGHT;
    }
    for item in &items[site_row_len..] {
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
    let site_row_width = site_row_len as f32 * ASK_SITE_BUTTON_STRIDE + 16.0;
    let width = ((max_chars as f32) * 7.2 + 44.0)
        .max(site_row_width)
        .clamp(CTX_MENU_MIN_WIDTH, CTX_MENU_MAX_WIDTH);
    (width, height)
}

/// Number of leading items that form an Ask submenu's row of chat sites.
fn ask_site_row_len(items: &[ResolvedMenuItem]) -> usize {
    items.iter().take_while(|item| ask_site_pick(item).is_some()).count()
}

fn row_offset(items: &[ResolvedMenuItem], target: usize) -> f32 {
    let site_row_len = ask_site_row_len(items);
    if site_row_len > 0 && target < site_row_len {
        return 5.0;
    }
    let mut y = 5.0 + if site_row_len > 0 { ASK_SITE_ROW_HEIGHT } else { 0.0 };
    for (idx, item) in items.iter().enumerate().skip(site_row_len) {
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

/// Steps focus across a context menu level while treating a leading Ask chat-site row (`0..site_row_len`) as a
/// single vertical stop navigated horizontally with Left/Right (`CTX-04`).
fn step_context_menu_index(
    items: &[ResolvedMenuItem],
    current: Option<usize>,
    action: &ListNavAction,
    typeahead: &mut TypeaheadState,
) -> Option<usize> {
    let site_len = ask_site_row_len(items);
    if site_len == 0 {
        return moved_index(
            action,
            typeahead,
            items.len(),
            current,
            true,
            |i| items.get(i).map(|it| it.label.clone()),
            |i| is_menu_item_selectable(items, i),
        );
    }

    let in_site_row = current.is_some_and(|i| i < site_len);
    match action {
        ListNavAction::OpenSubmenu if in_site_row => Some((current.unwrap_or(0) + 1) % site_len),
        ListNavAction::CloseSubmenu if in_site_row => {
            let idx = current.unwrap_or(0);
            if idx > 0 { Some(idx - 1) } else { None }
        }
        ListNavAction::MoveNext => {
            if in_site_row {
                (site_len..items.len())
                    .find(|&i| is_menu_item_selectable(items, i))
                    .or(Some(0))
            } else {
                let start = current.map_or(0, |i| i + 1);
                if start == 0 {
                    Some(0)
                } else {
                    (start..items.len())
                        .find(|&i| is_menu_item_selectable(items, i))
                        .or(Some(0))
                }
            }
        }
        ListNavAction::MovePrev => {
            if in_site_row || current.is_none() {
                (site_len..items.len())
                    .rev()
                    .find(|&i| is_menu_item_selectable(items, i))
                    .or(Some(0))
            } else {
                let idx = current.unwrap_or(site_len);
                (site_len..idx)
                    .rev()
                    .find(|&i| is_menu_item_selectable(items, i))
                    .or(Some(0))
            }
        }
        _ => moved_index(
            action,
            typeahead,
            items.len(),
            current,
            true,
            |i| items.get(i).map(|it| it.label.clone()),
            |i| is_menu_item_selectable(items, i),
        ),
    }
}
fn items_at_depth<'a>(root: &'a [ResolvedMenuItem], path: &[usize], depth: usize) -> Option<&'a [ResolvedMenuItem]> {
    let mut current = root;
    for &idx in path.iter().take(depth) {
        let parent = current.get(idx)?;
        if parent.children.is_empty() {
            return None;
        }
        current = &parent.children;
    }
    Some(current)
}

/// Overlay host rendering the active context menu in [`MenuViewState`].
#[derive(Clone, PartialEq)]
pub struct ContextMenuOverlay;

impl Component for ContextMenuOverlay {
    fn render(&self) -> impl IntoElement {
        let menu_state = use_menu_state();
        let core = use_core();
        let page_ctx = try_consume_context::<PageSurfaceContext>();
        let open = menu_state.context_menu.read().clone();
        let is_open = open.is_some();
        let fade = use_animation_with_dependencies(&is_open, move |conf, shown| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *shown { (0.0, 1.0) } else { (1.0, 1.0) };
            AnimNum::new(from, to)
                .duration(motion::CONTEXT_MENU_FADE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let Some(open) = open else {
            return rect().into_element();
        };
        let opacity = fade.get().value().clamp(0.0, 1.0);

        let root_size = *Platform::get().current_window().root_size.read();
        let window_size = (root_size.width.max(320.0), root_size.height.max(240.0));
        let max_h = max_popup_height(window_size.1, 12.0, CTX_MENU_MAX_HEIGHT);
        let (est_w, est_h) = estimate_menu_size(&open.items);
        let origin = clamp_popup_origin(open.position, (est_w, est_h.min(max_h)), window_size, 6.0);

        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .opacity(opacity)
            .child(
                rect()
                    .position(Position::new_global().left(0.0).top(0.0))
                    .width(Size::window_percent(100.0))
                    .height(Size::window_percent(100.0))
                    .on_mouse_down(move |e: Event<MouseEventData>| {
                        menu_state.close_context_menu();
                        // CTX-02: Right-clicking elsewhere on a page dismisses and immediately reopens in one click.
                        if e.button == Some(MouseButton::Right)
                            && let Some(ref ctx) = page_ctx
                        {
                            let gx = e.global_location.x as f32;
                            let gy = e.global_location.y as f32;
                            let mut tracker = ctx.tracker().borrow_mut();
                            if let Some((view_id, surface)) = tracker.hit_test_view(gx, gy) {
                                let dpr = surface.device_pixel_ratio.max(0.5);
                                let local_pt = Web_DevicePixelPoint {
                                    x: (((gx - surface.x) as f64) * dpr).round() as i32,
                                    y: (((gy - surface.y) as f64) * dpr).round() as i32,
                                };
                                let screen_pt = Web_DevicePixelPoint {
                                    x: (e.global_location.x * dpr).round() as i32,
                                    y: (e.global_location.y * dpr).round() as i32,
                                };
                                tracker.send_mouse_down(
                                    view_id,
                                    local_pt.clone(),
                                    screen_pt.clone(),
                                    WebMouseButton::Secondary,
                                );
                                tracker.send_mouse_up(view_id, local_pt, screen_pt, WebMouseButton::Secondary);
                            }
                        }
                    }),
            )
            .child({
                let mut card = ContextMenuCard::new(open.items)
                    .origin(origin)
                    .window_size(window_size)
                    .capture_global_keys(true)
                    .on_close(move |_| menu_state.close_context_menu())
                    .on_pick_site(move |site_id: ChatSiteId| {
                        let mut context_menu = menu_state.context_menu;
                        if site_id.0 == ASK_ALL_SITE_ID {
                            if let Some(open) = context_menu.write().as_mut() {
                                retarget_ask_menu_all(&mut open.items);
                            }
                            return;
                        }
                        let site = core
                            .model()
                            .peek()
                            .chat_sites
                            .iter()
                            .find(|site| site.id == site_id)
                            .cloned();
                        if let Some(site) = site
                            && let Some(open) = context_menu.write().as_mut()
                        {
                            retarget_ask_menu(&mut open.items, &site);
                        }
                    })
                    .on_pick_language(move |lang_name: String| {
                        let mut context_menu = menu_state.context_menu;
                        if let Some(open) = context_menu.write().as_mut() {
                            relabel_ask_language(&mut open.items, &lang_name);
                        }
                    });
                if let Some(submenu) = open.open_submenu {
                    card = card.initial_open_submenu(submenu);
                }
                card
            })
            .into_element()
    }
}

/// Reusable Ladybird-styled native context menu with nested submenus, scrolling, and keyboard navigation.
#[derive(Clone, PartialEq)]
pub struct ContextMenuCard {
    items: Vec<ResolvedMenuItem>,
    origin: Option<(f32, f32)>,
    window_size: (f32, f32),
    initial_open_submenu: Option<String>,
    capture_global_keys: bool,
    on_close: Option<EventHandler<()>>,
    on_pick_site: Option<EventHandler<ChatSiteId>>,
    on_pick_language: Option<EventHandler<String>>,
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
            on_pick_site: None,
            on_pick_language: None,
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

    /// Called when a site in an Ask submenu's row of sites is picked. The menu stays open.
    pub fn on_pick_site(mut self, handler: impl Into<EventHandler<ChatSiteId>>) -> Self {
        self.on_pick_site = Some(handler.into());
        self
    }

    pub fn on_pick_language(mut self, handler: impl Into<EventHandler<String>>) -> Self {
        self.on_pick_language = Some(handler.into());
        self
    }
}

impl Component for ContextMenuCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();

        let initial_sub = self
            .initial_open_submenu
            .as_deref()
            .and_then(|id| self.items.iter().position(|item| item.id.as_str() == id));

        // Stack of opened submenu indices (length = number of nested submenus open).
        let mut submenu_path = use_state(move || initial_sub.into_iter().collect::<Vec<usize>>());
        // Highlighted item index per level (always at least length `submenu_path.len() + 1`).
        let mut active_stack = use_state(move || {
            if let Some(sub) = initial_sub {
                vec![Some(sub), None]
            } else {
                vec![None]
            }
        });
        let mut typeahead = use_state(TypeaheadState::default);

        let items_for_keys = self.items.clone();
        let core_for_keys = core.clone();
        let on_close_keys = self.on_close.clone();
        let on_pick_keys = self.on_pick_site.clone();
        let on_lang_keys = self.on_pick_language.clone();
        let capture = self.capture_global_keys;

        let handle_key = move |e: Event<KeyboardEventData>| {
            if !capture {
                return;
            }
            let vim_enabled = core_for_keys.model().peek().vim_mode_enabled;
            let Some(action) = classify_list_key(&e, ListNavConfig::menu().with_vim_keys(vim_enabled)) else {
                if vim_enabled {
                    e.stop_propagation();
                    e.prevent_default();
                }
                return;
            };
            e.stop_propagation();
            e.prevent_default();

            let path = submenu_path.peek().clone();
            let depth = path.len();
            let Some(level_items) = items_at_depth(&items_for_keys, &path, depth) else {
                return;
            };
            let current_active = active_stack.peek().get(depth).copied().flatten();

            match action {
                ListNavAction::Dismiss => {
                    typeahead.write().clear();
                    if depth > 0 {
                        submenu_path.write().pop();
                        active_stack.write().truncate(depth);
                    } else if let Some(close) = &on_close_keys {
                        close.call(());
                    }
                }
                ListNavAction::CloseSubmenu => {
                    typeahead.write().clear();
                    let site_len = ask_site_row_len(level_items);
                    if site_len > 0 && current_active.is_some_and(|i| i > 0 && i < site_len) {
                        let mut stack = active_stack.write();
                        if stack.len() <= depth {
                            stack.resize(depth + 1, None);
                        }
                        stack[depth] = current_active.map(|i| i - 1);
                    } else if depth > 0 {
                        submenu_path.write().pop();
                        active_stack.write().truncate(depth);
                    }
                }
                ListNavAction::OpenSubmenu => {
                    typeahead.write().clear();
                    let site_len = ask_site_row_len(level_items);
                    if site_len > 0 && current_active.is_some_and(|i| i < site_len) {
                        let next_site = (current_active.unwrap_or(0) + 1) % site_len;
                        let mut stack = active_stack.write();
                        if stack.len() <= depth {
                            stack.resize(depth + 1, None);
                        }
                        stack[depth] = Some(next_site);
                    } else if let Some(idx) = current_active
                        && let Some(item) = level_items.get(idx)
                        && item.is_submenu()
                        && !item.children.is_empty()
                    {
                        let first =
                            first_selectable(item.children.len(), |i| is_menu_item_selectable(&item.children, i));
                        submenu_path.write().push(idx);
                        let mut stack = active_stack.write();
                        stack.resize(depth + 1, None);
                        stack[depth] = Some(idx);
                        stack.push(first);
                    }
                }
                ListNavAction::Activate => {
                    if let Some(idx) = current_active
                        && let Some(item) = level_items.get(idx)
                        && item.enabled
                    {
                        if let Some(site_id) = ask_site_pick(item) {
                            if let Some(pick) = &on_pick_keys {
                                pick.call(site_id);
                            }
                            return;
                        }
                        if let Some(lang) = ask_language_pick(item) {
                            if let Some(pick_lang) = &on_lang_keys {
                                pick_lang.call(lang.to_string());
                            }
                            return;
                        }
                        if item.is_submenu() && !item.children.is_empty() {
                            let first =
                                first_selectable(item.children.len(), |i| is_menu_item_selectable(&item.children, i));
                            submenu_path.write().push(idx);
                            let mut stack = active_stack.write();
                            stack.resize(depth + 1, None);
                            stack[depth] = Some(idx);
                            stack.push(first);
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
                _ => {
                    let next = step_context_menu_index(level_items, current_active, &action, &mut typeahead.write());
                    if let Some(idx) = next {
                        let mut stack = active_stack.write();
                        if stack.len() <= depth {
                            stack.resize(depth + 1, None);
                        }
                        stack[depth] = Some(idx);
                    }
                }
            }
        };

        let path = submenu_path.read().clone();
        let stack = active_stack.read().clone();
        let max_h = max_popup_height(self.window_size.1, 12.0, CTX_MENU_MAX_HEIGHT);

        struct MenuLevelLayout {
            items: Vec<ResolvedMenuItem>,
            active: Option<usize>,
            opened_child: Option<usize>,
            y_offset: f32,
            clamped_size: (f32, f32),
        }

        // Build each open menu level (depth 0 = root menu, depth 1.. = nested submenus).
        let mut levels: Vec<MenuLevelLayout> = Vec::new();
        for depth in 0..=path.len() {
            let Some(level_items) = items_at_depth(&self.items, &path, depth) else {
                break;
            };
            let active = stack.get(depth).copied().flatten();
            let opened_child = path.get(depth).copied();
            let (w, raw_h) = estimate_menu_size(level_items);
            let y_offset = if depth == 0 {
                0.0
            } else {
                let parent_items = items_at_depth(&self.items, &path, depth - 1).unwrap_or(&[]);
                row_offset(parent_items, path[depth - 1])
            };
            levels.push(MenuLevelLayout {
                items: level_items.to_vec(),
                active,
                opened_child,
                y_offset,
                clamped_size: (w, raw_h.min(max_h)),
            });
        }

        if let Some(root_origin) = self.origin {
            let mut prev_origin = root_origin;
            let mut prev_width = 0.0f32;
            let mut positioned = Vec::with_capacity(levels.len());

            for (depth, level) in levels.into_iter().enumerate() {
                let origin = if depth == 0 {
                    root_origin
                } else {
                    compute_submenu_origin(
                        prev_origin,
                        prev_width,
                        level.y_offset,
                        level.clamped_size,
                        self.window_size,
                    )
                };
                prev_origin = origin;
                prev_width = level.clamped_size.0;

                positioned.push(
                    rect()
                        .key(("ctx-pos", depth))
                        .position(Position::new_global().left(origin.0).top(origin.1))
                        .child(ContextMenuLevelCard {
                            depth,
                            items: level.items,
                            active: level.active,
                            opened_child: level.opened_child,
                            max_height: max_h,
                            submenu_path,
                            active_stack,
                            on_close: self.on_close.clone(),
                            on_pick_site: self.on_pick_site.clone(),
                            on_pick_language: self.on_pick_language.clone(),
                            key: DiffKey::from(&format!("ctx-level-{depth}")),
                        }),
                );
            }

            rect()
                .on_global_key_down(handle_key)
                .children(positioned)
                .into_element()
        } else {
            let inline_cards: Vec<Element> = levels
                .into_iter()
                .enumerate()
                .map(|(depth, level)| {
                    let y_off = level.y_offset;
                    let card = ContextMenuLevelCard {
                        depth,
                        items: level.items,
                        active: level.active,
                        opened_child: level.opened_child,
                        max_height: max_h,
                        submenu_path,
                        active_stack,
                        on_close: self.on_close.clone(),
                        on_pick_site: self.on_pick_site.clone(),
                        on_pick_language: self.on_pick_language.clone(),
                        key: DiffKey::from(&format!("ctx-level-{depth}")),
                    };
                    if depth == 0 {
                        card.into_element()
                    } else {
                        rect()
                            .key(("ctx-inline", depth))
                            .margin(Gaps::new((y_off - 4.0).max(0.0), 0.0, 0.0, 2.0))
                            .child(card)
                            .into_element()
                    }
                })
                .collect();

            rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::Start)
                .on_global_key_down(handle_key)
                .children(inline_cards)
                .into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// One level of a [`ContextMenuCard`] (root menu or any nested submenu), with its own scroll controller.
#[derive(Clone, PartialEq)]
struct ContextMenuLevelCard {
    depth: usize,
    items: Vec<ResolvedMenuItem>,
    active: Option<usize>,
    opened_child: Option<usize>,
    max_height: f32,
    submenu_path: State<Vec<usize>>,
    active_stack: State<Vec<Option<usize>>>,
    on_close: Option<EventHandler<()>>,
    on_pick_site: Option<EventHandler<ChatSiteId>>,
    on_pick_language: Option<EventHandler<String>>,
    key: DiffKey,
}

impl KeyExt for ContextMenuLevelCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ContextMenuLevelCard {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let core = use_core();
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let mut last_scrolled = use_state(|| self.active);

        let (est_width, raw_height) = estimate_menu_size(&self.items);
        let site_row_len = ask_site_row_len(&self.items);
        let has_gutter = menu_needs_gutter(&self.items[site_row_len..]);
        let viewport_height = (self.max_height - CTX_MENU_CHROME_HEIGHT).max(CTX_MENU_ROW_HEIGHT);
        let content_height = (raw_height - CTX_MENU_CHROME_HEIGHT).max(0.0);
        let scrolls = content_height > viewport_height;

        if scrolls && *last_scrolled.peek() != self.active {
            last_scrolled.set(self.active);
            if let Some(idx) = self.active {
                let top = (row_offset(&self.items, idx) - 5.0).max(0.0);
                let item_h = if site_row_len > 0 && idx < site_row_len {
                    ASK_SITE_ROW_HEIGHT
                } else {
                    CTX_MENU_ROW_HEIGHT
                };
                scroll_item_into_view(&mut scroll, top, item_h, viewport_height, content_height);
            }
        }

        let depth = self.depth;
        let mut submenu_path = self.submenu_path;
        let mut active_stack = self.active_stack;

        let site_row = (site_row_len > 0).then(|| {
            ask_site_row(
                &theme,
                &self.items[..site_row_len],
                self.active,
                depth,
                active_stack,
                self.on_pick_site.clone(),
            )
        });

        let rows: Vec<Element> = self
            .items
            .iter()
            .enumerate()
            .skip(site_row_len)
            .map(|(idx, item)| {
                if item.is_separator() {
                    return context_menu_separator(&theme, idx);
                }
                let highlighted = self.active == Some(idx) || self.opened_child == Some(idx);
                let is_sub = item.is_submenu() && !item.children.is_empty();
                let item_cmd = item.command.clone();
                let item_args = item.command_args.clone();
                let item_enabled = item.enabled;
                let keeps_open = item.keeps_menu_open;
                let lang_pick = ask_language_pick(item).map(str::to_string);
                let core_press = core.clone();
                let on_close_press = self.on_close.clone();
                let on_lang_press = self.on_pick_language.clone();

                context_menu_row(
                    &theme,
                    item,
                    idx,
                    has_gutter,
                    highlighted,
                    move |_| {
                        {
                            let mut stack = active_stack.write();
                            if stack.len() <= depth {
                                stack.resize(depth + 1, None);
                            }
                            stack[depth] = Some(idx);
                            stack.truncate(depth + 1);
                            if is_sub {
                                stack.push(None);
                            }
                        }
                        {
                            let mut path = submenu_path.write();
                            path.truncate(depth);
                            if is_sub {
                                path.push(idx);
                            }
                        }
                    },
                    move |_| {
                        if !item_enabled {
                            return;
                        }
                        if is_sub {
                            let currently_open = submenu_path.peek().get(depth).copied() == Some(idx);
                            let mut path = submenu_path.write();
                            path.truncate(depth);
                            let mut stack = active_stack.write();
                            stack.truncate(depth + 1);
                            if !currently_open {
                                path.push(idx);
                                stack.push(None);
                            }
                        } else if let Some(lang) = &lang_pick {
                            if let Some(pick_lang) = &on_lang_press {
                                pick_lang.call(lang.clone());
                            }
                        } else {
                            invoke_menu_command(&core_press, &item_cmd, &item_args);
                            if !keeps_open && let Some(close) = &on_close_press {
                                close.call(());
                            }
                        }
                    },
                )
            })
            .collect();

        let body: Element = if scrolls {
            ScrollView::new_controlled(scroll)
                .width(Size::fill())
                .height(Size::px(viewport_height))
                .maybe_child(site_row)
                .children(rows)
                .into_element()
        } else {
            rect()
                .width(Size::fill())
                .maybe_child(site_row)
                .children(rows)
                .into_element()
        };

        rect()
            .a11y_role(AccessibilityRole::Menu)
            .width(Size::px(est_width))
            .max_height(Size::px(self.max_height))
            .overflow(Overflow::Clip)
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
            .child(body)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// The row of chat site logos at the top of an Ask submenu. The checked site is the one the menu asks; picking another
/// retargets the menu and keeps it open.
fn ask_site_row(
    theme: &ResolvedTheme,
    picks: &[ResolvedMenuItem],
    active: Option<usize>,
    depth: usize,
    mut active_stack: State<Vec<Option<usize>>>,
    on_pick: Option<EventHandler<ChatSiteId>>,
) -> Element {
    let buttons = picks.iter().enumerate().filter_map(|(idx, item)| {
        let site_id = ask_site_pick(item)?;
        let checked = matches!(item.kind, MenuEntryKind::Checkable { checked: true });
        let background = if checked || active == Some(idx) {
            native_menu_hover_bg(theme)
        } else {
            Color::TRANSPARENT
        };
        let border = if checked {
            theme.color(ThemeRole::Accent)
        } else {
            Color::TRANSPARENT
        };
        let is_all = site_id.0 == ASK_ALL_SITE_ID;
        let content: Element = if is_all {
            label()
                .text("All")
                .font_size(11.0)
                .font_weight(FontWeight::MEDIUM)
                .color(theme.color(ThemeRole::Text))
                .into_element()
        } else {
            chat_site_logo(
                item.icon.as_deref().unwrap_or(site_id.0.as_str()),
                15.0,
                theme.color(ThemeRole::Text),
            )
        };
        let on_pick = on_pick.clone();
        let mut btn = rect()
            .key(idx)
            .a11y_role(AccessibilityRole::MenuItemRadio)
            .a11y_alt(if checked {
                format!("{}, asked", item.label)
            } else {
                item.label.clone()
            })
            .height(Size::px(ASK_SITE_BUTTON_STRIDE - 2.0))
            .center()
            .corner_radius(CornerRadius::new_all(5.0))
            .background(background)
            .border(Border::new().width(1.0).alignment(BorderAlignment::Inner).fill(border))
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| {
                let mut stack = active_stack.write();
                if stack.len() <= depth {
                    stack.resize(depth + 1, None);
                }
                stack[depth] = Some(idx);
            })
            .on_press(move |_| {
                if let Some(pick) = &on_pick {
                    pick.call(site_id.clone());
                }
            })
            .child(content);
        if is_all {
            btn = btn.padding(Gaps::new(0.0, 6.0, 0.0, 6.0));
        } else {
            btn = btn.width(Size::px(ASK_SITE_BUTTON_STRIDE - 2.0));
        }
        Some(btn.into_element())
    });
    rect()
        .key("ask-sites")
        .width(Size::fill())
        .height(Size::px(ASK_SITE_ROW_HEIGHT))
        .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
        .spacing(2.0)
        .direction(Direction::Horizontal)
        .cross_align(Alignment::center())
        .children(buttons)
        .into_element()
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
                if let Some(site_icon) = icon.strip_prefix("chat-site:") {
                    chat_site_logo(site_icon, 15.0, text_color)
                } else {
                    Icon::new(icon.clone())
                        .size(14.0)
                        .role(if item.enabled {
                            ThemeRole::Text
                        } else {
                            ThemeRole::TextMuted
                        })
                        .into_element()
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_at_depth_traverses_nested_submenus() {
        let leaf = ResolvedMenuItem::action("leaf", "Leaf", "cmd.leaf");
        let level2 = ResolvedMenuItem {
            kind: MenuEntryKind::Submenu,
            children: vec![leaf],
            ..ResolvedMenuItem::action("sub2", "Sub 2", "")
        };
        let level1 = ResolvedMenuItem {
            kind: MenuEntryKind::Submenu,
            children: vec![level2],
            ..ResolvedMenuItem::action("sub1", "Sub 1", "")
        };
        let root = vec![level1];

        assert_eq!(items_at_depth(&root, &[], 0).map(<[_]>::len), Some(1));
        assert_eq!(items_at_depth(&root, &[0], 1).map(|s| s[0].id.as_str()), Some("sub2"));
        assert_eq!(
            items_at_depth(&root, &[0, 0], 2).map(|s| s[0].id.as_str()),
            Some("leaf")
        );
        assert!(items_at_depth(&root, &[0, 0, 0], 3).is_none());
    }

    #[test]
    fn ask_site_row_steps_as_one_vertical_stop_and_horizontally_with_arrows() {
        let mut s1 = ResolvedMenuItem::action("zendia.tab.ask.site.chatgpt", "ChatGPT", "");
        s1.command_args.push(("site_id".to_string(), "chatgpt".to_string()));
        let mut s2 = ResolvedMenuItem::action("zendia.tab.ask.site.claude", "Claude", "");
        s2.command_args.push(("site_id".to_string(), "claude".to_string()));
        let prompt = ResolvedMenuItem::action("p1", "Summarize Tab", "webchat.ask");
        let items = vec![s1, s2, ResolvedMenuItem::separator("sep"), prompt];

        let mut ta = TypeaheadState::default();
        assert_eq!(
            step_context_menu_index(&items, None, &ListNavAction::MoveNext, &mut ta),
            Some(0)
        );
        assert_eq!(
            step_context_menu_index(&items, Some(0), &ListNavAction::OpenSubmenu, &mut ta),
            Some(1)
        );
        assert_eq!(
            step_context_menu_index(&items, Some(1), &ListNavAction::CloseSubmenu, &mut ta),
            Some(0)
        );
        assert_eq!(
            step_context_menu_index(&items, Some(1), &ListNavAction::MoveNext, &mut ta),
            Some(3)
        );
        assert_eq!(
            step_context_menu_index(&items, Some(3), &ListNavAction::MovePrev, &mut ta),
            Some(0)
        );
    }
}
