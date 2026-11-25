/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Zendia Vim picker modal (list and grid layouts), built on the generalized [`PickerList`] and [`PickerGrid`] kit
//! components and driven by `BrowserCore` picker commands and events.

use crate::ui::chrome::sidebar::state::favicon_badge_colors;
use crate::ui::core_view::{CoreView, use_core};
use crate::ui::kit::{Icon, PickerGrid, PickerItem as KitPickerItem, PickerList};
use crate::ui::page::PageSurfaceContext;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, ResolvedTheme, Rgba, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::picker::{
    DEFAULT_GRID_COLUMNS, PickerItem as CorePickerItem, PickerLayout, PickerRequest, filter_picker_items,
    find_item_by_key_badge, step_picker_cursor,
};

/// View-local state for the window's active Zendia Vim picker modal.
#[derive(Clone, Copy, PartialEq)]
pub struct VimPickerViewState {
    pub request: State<Option<PickerRequest>>,
    pub query: State<String>,
    pub cursor: State<usize>,
    pub filter_active: State<bool>,
    pub open_seq: State<u64>,
    pub focus_a11y_id: AccessibilityId,
}

impl VimPickerViewState {
    /// Opens a picker modal from a core-published [`PickerRequest`].
    pub fn open(&self, request: PickerRequest) {
        let initial = request.initial_selected;
        let mut req_state = self.request;
        let mut query = self.query;
        let mut cursor = self.cursor;
        let mut filter_active = self.filter_active;
        let mut seq = self.open_seq;

        query.set(String::new());
        filter_active.set(false);
        cursor.set(initial);
        req_state.set(Some(request));
        let next_seq = seq.peek().wrapping_add(1);
        seq.set(next_seq);
        self.focus_a11y_id.request_focus();
    }

    /// Closes the picker modal and resets transient query/cursor state.
    pub fn close(&self) {
        let mut req_state = self.request;
        let mut query = self.query;
        let mut filter_active = self.filter_active;
        req_state.set_if_modified(None);
        query.set_if_modified(String::new());
        filter_active.set_if_modified(false);
        self.focus_a11y_id.request_unfocus();
    }

    /// Returns whether a Vim picker modal is currently open.
    pub fn is_open(&self) -> bool {
        self.request.peek().is_some()
    }
}

/// Installs [`VimPickerViewState`] into Freya context for the current window.
pub fn use_provide_vim_picker_state() -> VimPickerViewState {
    let request = use_state(|| None::<PickerRequest>);
    let query = use_state(String::new);
    let cursor = use_state(|| 0usize);
    let filter_active = use_state(|| false);
    let open_seq = use_state(|| 0u64);
    let focus_a11y_id = use_hook(AccessibilityId::new_unique);

    use_provide_context(|| VimPickerViewState {
        request,
        query,
        cursor,
        filter_active,
        open_seq,
        focus_a11y_id,
    })
}

/// Consumes the window's [`VimPickerViewState`] from Freya context.
pub fn use_vim_picker_state() -> VimPickerViewState {
    use_consume::<VimPickerViewState>()
}

/// Consumes [`VimPickerViewState`] if a provider is installed in this window.
pub fn try_use_vim_picker_state() -> Option<VimPickerViewState> {
    try_consume_context::<VimPickerViewState>()
}

/// Clean vector emblems for the built-in Agents grid picker so every brand tile reads sharply at 1x and 2x.
pub fn agent_emblem_svg(icon_id: &str, dimmed: bool) -> Option<(String, Bytes)> {
    let suffix = if dimmed { "dim" } else { "full" };
    let svg = match icon_id {
        "agent-claude" => {
            let color = if dimmed { "#6e7280" } else { "#d97757" };
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="38" height="38" viewBox="0 0 38 38">
  <g stroke="{color}" stroke-width="3.2" stroke-linecap="round">
    <line x1="19" y1="5" x2="19" y2="33"/>
    <line x1="5" y1="19" x2="33" y2="19"/>
    <line x1="9" y1="9" x2="29" y2="29"/>
    <line x1="29" y1="9" x2="9" y2="29"/>
    <line x1="13" y1="6.2" x2="25" y2="31.8"/>
    <line x1="31.8" y1="13" x2="6.2" y2="25"/>
  </g>
</svg>"##
            )
        }
        "agent-codex" => {
            let bg = if dimmed { "#6e7280" } else { "#3b6df6" };
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="38" height="38" viewBox="0 0 38 38">
  <rect x="4" y="6" width="30" height="26" rx="9" fill="{bg}"/>
  <path d="M12 14.5L17.5 19L12 23.5" fill="none" stroke="#ffffff" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/>
  <line x1="19.5" y1="23.5" x2="26.5" y2="23.5" stroke="#ffffff" stroke-width="2.6" stroke-linecap="round"/>
</svg>"##
            )
        }
        "agent-cursor" => {
            let top = if dimmed { "#7b7f8c" } else { "#3f4658" };
            let left = if dimmed { "#585c68" } else { "#232836" };
            let right = if dimmed { "#9397a4" } else { "#5e667a" };
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="38" height="38" viewBox="0 0 38 38">
  <polygon points="19,4 32,11.5 19,19 6,11.5" fill="{top}"/>
  <polygon points="6,11.5 19,19 19,34 6,26.5" fill="{left}"/>
  <polygon points="32,11.5 19,19 19,34 32,26.5" fill="{right}"/>
</svg>"##
            )
        }
        "agent-antigravity" => {
            if dimmed {
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="38" height="38" viewBox="0 0 38 38">
  <path d="M8 30C12 16 15.5 8 19 8C22.5 8 26 16 30 30" fill="none" stroke="#6e7280" stroke-width="5.2" stroke-linecap="round"/>
  <path d="M12.5 22.5C15.5 20.5 22.5 20.5 25.5 22.5" fill="none" stroke="#8a8e9c" stroke-width="3.6" stroke-linecap="round"/>
</svg>"##
                    .to_string()
            } else {
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="38" height="38" viewBox="0 0 38 38">
  <defs>
    <linearGradient id="ag" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="#34a853"/>
      <stop offset="45%" stop-color="#4285f4"/>
      <stop offset="80%" stop-color="#7c5cff"/>
      <stop offset="100%" stop-color="#ea4335"/>
    </linearGradient>
  </defs>
  <path d="M8 30C12 16 15.5 8 19 8C22.5 8 26 16 30 30" fill="none" stroke="url(#ag)" stroke-width="5.2" stroke-linecap="round"/>
  <path d="M12.5 22.5C15.5 20.5 22.5 20.5 25.5 22.5" fill="none" stroke="#4285f4" stroke-width="3.4" stroke-linecap="round"/>
</svg>"##
                    .to_string()
            }
        }
        "agent-opencode" => {
            let outer = if dimmed { "#3c3f4a" } else { "#f5a742" };
            let inner = if dimmed { "#9fa3b0" } else { "#ffe2b8" };
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="38" height="38" viewBox="0 0 38 38">
  <rect x="7" y="5" width="24" height="28" rx="4" fill="{outer}"/>
  <rect x="13" y="11" width="12" height="16" rx="2" fill="{inner}"/>
</svg>"##
            )
        }
        "agent-kiro" => {
            let body = if dimmed { "#3c3f4a" } else { "#9046ff" };
            let eye = if dimmed { "#e2e5ee" } else { "#ffffff" };
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="38" height="38" viewBox="0 0 38 38">
  <path d="M10 31V18C10 11.4 14 7 19 7C24 7 28 11.4 28 18V31L24 27.5L21 31L19 28L17 31L14 27.5L10 31Z" fill="{body}"/>
  <circle cx="15.5" cy="17.5" r="2.1" fill="{eye}"/>
  <circle cx="22.5" cy="17.5" r="2.1" fill="{eye}"/>
</svg>"##
            )
        }
        _ => return None,
    };
    Some((
        format!("zendia-agent-emblem-{icon_id}-{suffix}"),
        Bytes::from(svg.into_bytes()),
    ))
}

fn core_item_to_kit_item(item: &CorePickerItem, layout: PickerLayout, theme: &ResolvedTheme) -> KitPickerItem {
    let mut kit = KitPickerItem::new(item.id.clone(), item.title.clone())
        .dimmed(item.dimmed)
        .disabled(!item.enabled);

    if let Some(section) = &item.section {
        kit = kit.section(section.clone());
    }
    if let Some(detail) = &item.detail {
        kit = kit.detail(detail.clone());
    }
    if let Some(key) = &item.key {
        kit = kit.key_badge(key.clone());
    }
    if let Some((r, g, b)) = item.accent_rgb {
        kit = kit.accent_color(Color::from_rgb(r, g, b));
    }

    if let Some(icon) = &item.icon {
        if let Some((cache_key, bytes)) = agent_emblem_svg(icon, item.dimmed) {
            kit = kit.custom_icon_svg(cache_key, bytes);
        } else {
            kit = kit.icon(icon.clone());
            if layout == PickerLayout::List {
                let (bg, fg) = if let Some((r, g, b)) = item.icon_badge_rgb {
                    (Color::from_rgb(r, g, b), Color::WHITE)
                } else {
                    favicon_badge_colors(Some(icon.as_str()), theme)
                };
                kit = kit.icon_badge(bg, fg);
            }
        }
    }

    kit
}

fn invoke_choose(core: &CoreView, request: &PickerRequest, item: &CorePickerItem) {
    if !item.enabled {
        return;
    }
    core.invoke_or_log(Command::PickerChoose {
        request_id: Some(request.id.clone()),
        item_id: item.id.clone(),
    });
}

/// Computes the modal's `(width, top)` inside a window or stage of `(viewport_w, viewport_h)`.
pub fn compute_picker_geometry(layout: PickerLayout, viewport_w: f32, viewport_h: f32) -> (f32, f32) {
    let max_w = match layout {
        PickerLayout::List => 560.0,
        PickerLayout::Grid => 500.0,
    };
    let width = (viewport_w * 0.80).min(max_w).max(280.0);
    let top = (viewport_h * 0.20).clamp(32.0, (viewport_h - 180.0).max(32.0));
    (width, top)
}

/// Window-root overlay rendering the active Zendia Vim picker modal above all chrome and pages.
///
/// Only shows when `model.vim_mode_enabled` is true.
#[derive(Clone, PartialEq)]
pub struct VimPickerOverlay;

impl Component for VimPickerOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let state = use_vim_picker_state();
        let page_ctx = try_consume_context::<PageSurfaceContext>();

        let vim_enabled = core.model().read().vim_mode_enabled;
        let current_request = state.request.read().clone();
        let is_open = vim_enabled && current_request.is_some();
        let open_seq = *state.open_seq.read();

        let mut last_request = use_state(|| None::<PickerRequest>);
        if let Some(req) = &current_request
            && last_request.peek().as_ref() != Some(req)
        {
            last_request.set(Some(req.clone()));
        }

        let entrance = use_animation_with_dependencies(&(is_open, open_seq), move |conf, (open, _)| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *open {
                AnimNum::new(0.0, 1.0)
                    .duration(motion::FLOAT_IN)
                    .ease(Ease::Out)
                    .function(motion::spring_fn())
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(motion::FLOAT_OUT)
                    .ease(Ease::In)
                    .function(motion::ease_in_fn())
            }
        });

        let progress = entrance.get().value().clamp(0.0, 1.1);
        let opacity = progress.clamp(0.0, 1.0);
        if !is_open && opacity <= 0.01 {
            return rect().into_element();
        }

        let Some(request) = current_request.or_else(|| last_request.read().clone()) else {
            return rect().into_element();
        };

        let root_size = *Platform::get().current_window().root_size.read();
        let window_w = root_size.width.max(480.0);
        let window_h = root_size.height.max(360.0);
        let (card_w, base_top) = compute_picker_geometry(request.layout, window_w, window_h);
        let left = ((window_w - card_w) * 0.5).max(16.0);
        let y_offset = motion::FLOAT_IN_Y * (1.0 - progress);
        let top = (base_top + y_offset).max(20.0);

        let close_picker = {
            let core = core.clone();
            let page_ctx = page_ctx.clone();
            let request_id = request.id.clone();
            move || {
                state.close();
                core.invoke_or_log(Command::PickerClose {
                    request_id: Some(request_id.clone()),
                });
                if let Some(ctx) = &page_ctx {
                    ctx.request_focus_active_surface();
                }
            }
        };
        let close_backdrop = close_picker.clone();
        let close_card = close_picker;

        rect()
            .a11y_id(state.focus_a11y_id)
            .a11y_focusable(true)
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .on_mouse_down(move |_| {
                close_backdrop();
            })
            .child(
                rect()
                    .position(Position::new_global().left(left).top(top))
                    .opacity(opacity)
                    .child(
                        VimPickerCard::new(request, state.query, state.cursor, state.filter_active)
                            .width(card_w)
                            .capture_keys(is_open)
                            .on_close(move |_| close_card())
                            .key(open_seq),
                    ),
            )
            .into_element()
    }
}

/// Reusable Zendia Vim picker modal card used by [`VimPickerOverlay`] and by the `vim-pickers` gallery section.
#[derive(Clone, PartialEq)]
pub struct VimPickerCard {
    request: PickerRequest,
    query: State<String>,
    cursor: State<usize>,
    filter_active: State<bool>,
    width: Option<f32>,
    max_list_height: f32,
    capture_keys: bool,
    on_choose: Option<EventHandler<CorePickerItem>>,
    on_close: Option<EventHandler<()>>,
    key: DiffKey,
}

impl KeyExt for VimPickerCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl VimPickerCard {
    pub fn new(request: PickerRequest, query: State<String>, cursor: State<usize>, filter_active: State<bool>) -> Self {
        Self {
            request,
            query,
            cursor,
            filter_active,
            width: None,
            max_list_height: 360.0,
            capture_keys: false,
            on_choose: None,
            on_close: None,
            key: DiffKey::None,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn max_list_height(mut self, height: f32) -> Self {
        self.max_list_height = height;
        self
    }

    pub fn capture_keys(mut self, capture: bool) -> Self {
        self.capture_keys = capture;
        self
    }

    pub fn on_choose(mut self, handler: impl Into<EventHandler<CorePickerItem>>) -> Self {
        self.on_choose = Some(handler.into());
        self
    }

    pub fn on_close(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_close = Some(handler.into());
        self
    }
}

impl Component for VimPickerCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();

        let query_text = self.query.read().clone();
        let is_filtering = *self.filter_active.read() || !query_text.is_empty();
        let filtered = filter_picker_items(&self.request, &query_text);
        let item_count = filtered.len();

        let mut cursor = self.cursor;
        let current_cursor = if item_count == 0 {
            None
        } else {
            let clamped = (*cursor.read()).min(item_count - 1);
            if *cursor.peek() != clamped {
                cursor.set(clamped);
            }
            Some(clamped)
        };

        let kit_items: Vec<KitPickerItem> = filtered
            .iter()
            .map(|item| core_item_to_kit_item(item, self.request.layout, &theme))
            .collect();

        let choose_item = {
            let core = core.clone();
            let request = self.request.clone();
            let on_choose = self.on_choose.clone();
            let on_close = self.on_close.clone();
            move |item: &CorePickerItem| {
                if !item.enabled {
                    return;
                }
                if let Some(handler) = &on_choose {
                    handler.call(item.clone());
                } else {
                    invoke_choose(&core, &request, item);
                    if let Some(close) = &on_close {
                        close.call(());
                    }
                }
            }
        };

        let choose_on_click = {
            let filtered = filtered.clone();
            let choose_item = choose_item.clone();
            move |idx: usize| {
                if let Some(item) = filtered.get(idx) {
                    choose_item(item);
                }
            }
        };

        let handle_global_key = {
            let filtered = filtered.clone();
            let layout = self.request.layout;
            let filterable = self.request.filterable;
            let capture = self.capture_keys;
            let mut query_state = self.query;
            let mut filter_state = self.filter_active;
            let choose_item = choose_item;
            let on_close = self.on_close.clone();
            move |e: Event<KeyboardEventData>| {
                if !capture {
                    return;
                }
                let ctrl = e.modifiers.contains(Modifiers::CONTROL);
                let alt = e.modifiers.contains(Modifiers::ALT);
                let meta = e.modifiers.contains(Modifiers::META);
                let filtering_now = *filter_state.peek() || !query_state.peek().is_empty();

                let mut move_cursor = |dx: isize, dy: isize| {
                    let current = *cursor.peek();
                    if let Some(next) =
                        step_picker_cursor(current, filtered.len(), layout, DEFAULT_GRID_COLUMNS, dx, dy)
                    {
                        cursor.set(next);
                    }
                };

                match &e.key {
                    Key::Named(NamedKey::Escape) => {
                        e.stop_propagation();
                        if let Some(close) = &on_close {
                            close.call(());
                        }
                    }
                    Key::Character(ch) if ctrl && !alt && !meta => {
                        let lower = ch.to_ascii_lowercase();
                        if lower == "c" || lower == "[" || e.code == Code::KeyC || e.code == Code::BracketLeft {
                            e.stop_propagation();
                            if let Some(close) = &on_close {
                                close.call(());
                            }
                        } else if lower == "n" || e.code == Code::KeyN {
                            e.stop_propagation();
                            move_cursor(0, 1);
                        } else if lower == "p" || e.code == Code::KeyP {
                            e.stop_propagation();
                            move_cursor(0, -1);
                        }
                    }
                    Key::Named(NamedKey::Enter) => {
                        e.stop_propagation();
                        let idx = (*cursor.peek()).min(filtered.len().saturating_sub(1));
                        if let Some(item) = filtered.get(idx) {
                            choose_item(item);
                        }
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        e.stop_propagation();
                        move_cursor(0, 1);
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        e.stop_propagation();
                        move_cursor(0, -1);
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        e.stop_propagation();
                        if layout == PickerLayout::Grid {
                            move_cursor(1, 0);
                        } else {
                            move_cursor(0, 1);
                        }
                    }
                    Key::Named(NamedKey::ArrowLeft) => {
                        e.stop_propagation();
                        if layout == PickerLayout::Grid {
                            move_cursor(-1, 0);
                        } else {
                            move_cursor(0, -1);
                        }
                    }
                    Key::Named(NamedKey::Backspace) if filtering_now => {
                        e.stop_propagation();
                        let mut text = query_state.peek().clone();
                        if text.pop().is_some() {
                            query_state.set(text);
                            cursor.set(0);
                        } else {
                            filter_state.set(false);
                        }
                    }
                    Key::Character(ch) if !ctrl && !alt && !meta => {
                        e.stop_propagation();
                        if filtering_now {
                            if !ch.chars().any(char::is_control) {
                                let mut text = query_state.peek().clone();
                                text.push_str(ch);
                                query_state.set(text);
                                cursor.set(0);
                            }
                        } else if ch == "/" && filterable {
                            filter_state.set(true);
                        } else if ch == "j" {
                            move_cursor(0, 1);
                        } else if ch == "k" {
                            move_cursor(0, -1);
                        } else if ch == "h" && layout == PickerLayout::Grid {
                            move_cursor(-1, 0);
                        } else if ch == "l" && layout == PickerLayout::Grid {
                            move_cursor(1, 0);
                        } else if let Some(jump_idx) = find_item_by_key_badge(&filtered, ch) {
                            cursor.set(jump_idx);
                            if let Some(item) = filtered.get(jump_idx) {
                                choose_item(item);
                            }
                        }
                    }
                    _ => {}
                }
            }
        };

        let card_width = self.width.unwrap_or(match self.request.layout {
            PickerLayout::List => 560.0,
            PickerLayout::Grid => 500.0,
        });

        let (glass_bg, outline_color, text_muted, divider_color) = match theme.scheme {
            ResolvedScheme::Dark => (
                Rgba::from_rgb_alpha(30, 30, 34, 0.90).to_color(),
                Color::from_argb(38, 255, 255, 255),
                Color::from_argb(196, 251, 251, 254),
                Color::from_argb(32, 255, 255, 255),
            ),
            ResolvedScheme::Light => (
                Rgba::from_rgb_alpha(232, 232, 236, 0.90).to_color(),
                Color::from_argb(36, 0, 0, 0),
                Color::from_argb(190, 21, 20, 26),
                Color::from_argb(32, 0, 0, 0),
            ),
        };

        let filter_row = is_filtering.then(|| {
            let mut clear_query = self.query;
            let mut clear_filter = self.filter_active;
            let filter_bg = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(24, 255, 255, 255),
                ResolvedScheme::Light => Color::from_argb(18, 0, 0, 0),
            };
            let filter_text = match theme.scheme {
                ResolvedScheme::Dark => Color::from_argb(238, 251, 251, 254),
                ResolvedScheme::Light => Color::from_argb(230, 21, 20, 26),
            };
            let count_label = format!("{item_count}/{}", self.request.items.len());
            rect()
                .width(Size::fill())
                .height(Size::px(30.0))
                .margin(Gaps::new(0.0, 4.0, 6.0, 4.0))
                .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
                .spacing(8.0)
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .cross_align(Alignment::center())
                .corner_radius(CornerRadius::new_all(9.0))
                .background(filter_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.rgba(ThemeRole::Accent).with_alpha(0.55).to_color()),
                )
                .child(Icon::new("search-glass").size(13.0).role(ThemeRole::Accent))
                .child(
                    rect().width(Size::flex(1.0)).overflow(Overflow::Clip).child(
                        label()
                            .text(if query_text.is_empty() {
                                "Type to filter...".to_string()
                            } else {
                                query_text.clone()
                            })
                            .font_size(12.5)
                            .color(if query_text.is_empty() { text_muted } else { filter_text })
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    ),
                )
                .child(
                    label()
                        .text(count_label)
                        .font_size(11.0)
                        .font_family("monospace")
                        .color(text_muted),
                )
                .child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("Clear filter")
                        .width(Size::px(18.0))
                        .height(Size::px(18.0))
                        .center()
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            clear_query.set(String::new());
                            clear_filter.set(false);
                        })
                        .child(Icon::new("close").size(11.0).role(ThemeRole::TextMuted)),
                )
        });

        let body: Element = match self.request.layout {
            PickerLayout::List => PickerList::new(kit_items)
                .vim_style(true)
                .highlighted(current_cursor)
                .max_height(self.max_list_height)
                .empty_message("No matching items")
                .on_hover(move |idx: usize| {
                    cursor.set_if_modified(idx);
                })
                .on_select(choose_on_click)
                .into_element(),
            PickerLayout::Grid => PickerGrid::new(kit_items)
                .columns(DEFAULT_GRID_COLUMNS)
                .highlighted(current_cursor)
                .max_height(self.max_list_height)
                .empty_message("No matching items")
                .on_hover(move |idx: usize| {
                    cursor.set_if_modified(idx);
                })
                .on_select(choose_on_click)
                .into_element(),
        };

        let hint_text = if is_filtering {
            "Type to filter · Arrows move · Enter pick · Backspace clear · Esc cancel".to_string()
        } else {
            self.request.hint.clone()
        };

        let card = rect()
            .a11y_role(AccessibilityRole::Dialog)
            .a11y_alt(self.request.title.clone())
            .width(Size::px(card_width))
            .padding(Gaps::new_all(7.0))
            .corner_radius(CornerRadius::new_all(theme.radii.float))
            .overflow(Overflow::Clip)
            .background(glass_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(outline_color),
            )
            .shadow(theme.shadows.float_highlight.to_shadow())
            .shadow(theme.shadows.float_ambient.to_shadow())
            .shadow(theme.shadows.float_drop.to_shadow())
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .on_global_key_down(handle_global_key)
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(6.0, 10.0, 8.0, 10.0))
                    .child(
                        label()
                            .text(self.request.title.clone())
                            .font_size(13.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(text_muted)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    ),
            )
            .maybe_child(filter_row)
            .child(body)
            .child(
                rect()
                    .width(Size::fill())
                    .margin(Gaps::new(6.0, 0.0, 0.0, 0.0))
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::px(1.0))
                            .background(divider_color),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(7.0, 10.0, 3.0, 10.0))
                            .child(
                                label()
                                    .text(hint_text)
                                    .font_size(11.0)
                                    .color(text_muted)
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis),
                            ),
                    ),
            );

        if theme.glass.blur > 0.0 {
            card.backdrop_blur(theme.glass.blur)
        } else {
            card
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_geometry_respects_list_and_grid_widths() {
        let (list_w, list_top) = compute_picker_geometry(PickerLayout::List, 1600.0, 1000.0);
        assert_eq!(list_w, 560.0);
        assert_eq!(list_top, 200.0);

        let (grid_w, grid_top) = compute_picker_geometry(PickerLayout::Grid, 1600.0, 1000.0);
        assert_eq!(grid_w, 500.0);
        assert_eq!(grid_top, 200.0);

        let (narrow_w, _) = compute_picker_geometry(PickerLayout::List, 500.0, 600.0);
        assert_eq!(narrow_w, 400.0);
    }

    #[test]
    fn agent_emblems_parse_in_skia_for_active_and_dimmed_variants() {
        use freya::engine::prelude::{FontMgr, svg};

        for id in [
            "agent-claude",
            "agent-codex",
            "agent-cursor",
            "agent-antigravity",
            "agent-opencode",
            "agent-kiro",
        ] {
            for dimmed in [false, true] {
                let (_, bytes) = agent_emblem_svg(id, dimmed).expect("emblem should exist");
                let dom = svg::Dom::from_bytes(&bytes, FontMgr::empty());
                assert!(
                    dom.is_ok(),
                    "Skia failed to parse {id} (dimmed={dimmed}): {:?}",
                    dom.err()
                );
            }
        }
    }
}
