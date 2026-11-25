/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Full-page crosshair region picker overlay for Zendia Vim mode.
//!
//! Covers the active page area with a soft dim scrim and floating instruction banner. Dragging draws
//! an accent-outlined selection cutout; pressing Enter captures either the drawn rectangle or the
//! whole page view via `region.capture`; pressing Escape (or right-clicking) cancels via `region.cancel`.

use crate::ui::core_view::use_core;
use crate::ui::page::PageSurfaceContext;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::TabId;
use zendia_browser_core::region::{REGION_PICKER_HINT, RegionRect};

/// View-local state for an active page region pick inside a window.
#[derive(Clone, Copy, PartialEq)]
pub struct RegionPickerViewState {
    pub active: State<bool>,
    pub tab_id: State<Option<TabId>>,
    pub drag_start: State<Option<(f32, f32)>>,
    pub selection: State<Option<RegionRect>>,
    pub dragging: State<bool>,
    pub focus_a11y_id: AccessibilityId,
}

impl RegionPickerViewState {
    /// Starts region selection over `tab_id` (resetting any previous box).
    pub fn start(&self, tab_id: Option<TabId>) {
        let mut active = self.active;
        let mut tab = self.tab_id;
        let mut start = self.drag_start;
        let mut selection = self.selection;
        let mut dragging = self.dragging;

        tab.set(tab_id);
        start.set(None);
        selection.set(None);
        dragging.set(false);
        active.set(true);
        self.focus_a11y_id.request_focus();
    }

    /// Ends region selection and clears all transient drag state.
    pub fn stop(&self) {
        let mut active = self.active;
        let mut start = self.drag_start;
        let mut selection = self.selection;
        let mut dragging = self.dragging;

        active.set_if_modified(false);
        start.set_if_modified(None);
        selection.set_if_modified(None);
        dragging.set_if_modified(false);
        self.focus_a11y_id.request_unfocus();
    }

    /// Returns whether the region picker overlay is currently active.
    pub fn is_active(&self) -> bool {
        *self.active.peek()
    }
}

/// Installs [`RegionPickerViewState`] into Freya context for the current window.
pub fn use_provide_region_picker_state() -> RegionPickerViewState {
    let active = use_state(|| false);
    let tab_id = use_state(|| None::<TabId>);
    let drag_start = use_state(|| None::<(f32, f32)>);
    let selection = use_state(|| None::<RegionRect>);
    let dragging = use_state(|| false);
    let focus_a11y_id = use_hook(AccessibilityId::new_unique);

    use_provide_context(|| RegionPickerViewState {
        active,
        tab_id,
        drag_start,
        selection,
        dragging,
        focus_a11y_id,
    })
}

/// Consumes the window's [`RegionPickerViewState`] from Freya context.
pub fn use_region_picker_state() -> RegionPickerViewState {
    use_consume::<RegionPickerViewState>()
}

/// Consumes [`RegionPickerViewState`] if a provider is installed in this window.
pub fn try_use_region_picker_state() -> Option<RegionPickerViewState> {
    try_consume_context::<RegionPickerViewState>()
}

/// Live overlay mounted inside [`PageArea`](crate::ui::page::PageArea) when `region.pick` is active.
///
/// Gated strictly on `model.vim_mode_enabled`.
#[derive(Clone, PartialEq)]
pub struct RegionPickerOverlay;

impl Component for RegionPickerOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let region_state = try_use_region_picker_state();
        let page_ctx = try_consume_context::<PageSurfaceContext>();

        let (vim_enabled, zoom_level) = {
            let model = core.model().read();
            (model.vim_mode_enabled, model.zoom_level)
        };

        let Some(state) = region_state else {
            return rect().into_element();
        };
        if !vim_enabled || !*state.active.read() {
            return rect().into_element();
        }

        let target_tab = *state.tab_id.read();
        let on_capture = {
            let core = core.clone();
            let page_ctx = page_ctx.clone();
            move |(rect, whole_page): (RegionRect, bool)| {
                state.stop();
                core.invoke_or_log(Command::RegionCapture {
                    tab_id: target_tab,
                    rect: Some(rect),
                    whole_page,
                });
                if let Some(ctx) = &page_ctx {
                    ctx.request_focus_active_surface();
                }
            }
        };
        let on_cancel = {
            let core = core.clone();
            let page_ctx = page_ctx;
            move |_: ()| {
                state.stop();
                core.invoke_or_log(Command::RegionCancel);
                if let Some(ctx) = &page_ctx {
                    ctx.request_focus_active_surface();
                }
            }
        };

        RegionPickerSurface::new(state.drag_start, state.selection, state.dragging)
            .focus_a11y_id(state.focus_a11y_id)
            .zoom_level(zoom_level)
            .capture_keys(true)
            .on_capture(on_capture)
            .on_cancel(on_cancel)
            .into_element()
    }
}

/// Interactive crosshair region selection surface covering its parent page container.
///
/// Used both live by [`RegionPickerOverlay`] and in the `vim-pickers` gallery section.
#[derive(Clone, PartialEq)]
pub struct RegionPickerSurface {
    drag_start: State<Option<(f32, f32)>>,
    selection: State<Option<RegionRect>>,
    dragging: State<bool>,
    focus_a11y_id: Option<AccessibilityId>,
    zoom_level: f32,
    capture_keys: bool,
    on_capture: Option<EventHandler<(RegionRect, bool)>>,
    on_cancel: Option<EventHandler<()>>,
    key: DiffKey,
}

impl KeyExt for RegionPickerSurface {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl RegionPickerSurface {
    pub fn new(
        drag_start: State<Option<(f32, f32)>>,
        selection: State<Option<RegionRect>>,
        dragging: State<bool>,
    ) -> Self {
        Self {
            drag_start,
            selection,
            dragging,
            focus_a11y_id: None,
            zoom_level: 1.0,
            capture_keys: false,
            on_capture: None,
            on_cancel: None,
            key: DiffKey::None,
        }
    }

    pub fn focus_a11y_id(mut self, id: AccessibilityId) -> Self {
        self.focus_a11y_id = Some(id);
        self
    }

    pub fn zoom_level(mut self, zoom: f32) -> Self {
        self.zoom_level = zoom;
        self
    }

    pub fn capture_keys(mut self, capture: bool) -> Self {
        self.capture_keys = capture;
        self
    }

    pub fn on_capture(mut self, handler: impl Into<EventHandler<(RegionRect, bool)>>) -> Self {
        self.on_capture = Some(handler.into());
        self
    }

    pub fn on_cancel(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_cancel = Some(handler.into());
        self
    }
}

impl Component for RegionPickerSurface {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut area = use_state(|| (0.0f32, 0.0f32, 0.0f32, 0.0f32));

        let mut drag_start = self.drag_start;
        let mut selection = self.selection;
        let mut dragging = self.dragging;
        let current_selection = *selection.read();
        let (_, _, area_w, area_h) = *area.read();

        let on_cancel_mouse = self.on_cancel.clone();
        let on_mouse_down = move |e: Event<MouseEventData>| {
            e.stop_propagation();
            if e.button != Some(MouseButton::Left) {
                dragging.set(false);
                drag_start.set(None);
                selection.set(None);
                if let Some(cancel) = &on_cancel_mouse {
                    cancel.call(());
                }
                return;
            }
            let (ox, oy, w, h) = *area.peek();
            let lx = (e.global_location.x as f32 - ox).clamp(0.0, w.max(0.0));
            let ly = (e.global_location.y as f32 - oy).clamp(0.0, h.max(0.0));
            drag_start.set(Some((lx, ly)));
            selection.set(None);
            dragging.set(true);
        };

        let on_global_pointer_move = move |e: Event<PointerEventData>| {
            if !*dragging.peek() {
                return;
            }
            let Some(start) = *drag_start.peek() else {
                return;
            };
            let (ox, oy, w, h) = *area.peek();
            let lx = e.global_location().x as f32 - ox;
            let ly = e.global_location().y as f32 - oy;
            let rect = RegionRect::from_drag_points(start, (lx, ly), w, h);
            selection.set(Some(rect));
        };

        let on_global_pointer_up = move |_: Event<PointerEventData>| {
            if !*dragging.peek() {
                return;
            }
            dragging.set(false);
            drag_start.set(None);
            let keep = selection.peek().filter(|r| r.is_valid_drag());
            selection.set(keep);
        };

        let capture_keys = self.capture_keys;
        let zoom_level = self.zoom_level;
        let on_capture_key = self.on_capture.clone();
        let on_cancel_key = self.on_cancel.clone();
        let on_global_key_down = move |e: Event<KeyboardEventData>| {
            if !capture_keys {
                return;
            }
            match &e.key {
                Key::Named(NamedKey::Escape) => {
                    e.stop_propagation();
                    dragging.set(false);
                    drag_start.set(None);
                    selection.set(None);
                    if let Some(cancel) = &on_cancel_key {
                        cancel.call(());
                    }
                }
                Key::Named(NamedKey::Enter) => {
                    e.stop_propagation();
                    let (_, _, w, h) = *area.peek();
                    let chosen = *selection.peek();
                    dragging.set(false);
                    drag_start.set(None);
                    if let Some(r) = chosen.filter(|r| r.is_valid_drag()) {
                        if let Some(capture) = &on_capture_key {
                            capture.call((r.to_page_coords(zoom_level), false));
                        }
                    } else if let Some(capture) = &on_capture_key {
                        capture.call((RegionRect::whole_page(w, h, zoom_level), true));
                    }
                }
                _ => {}
            }
        };

        let idle_scrim = Color::from_argb(56, 0, 0, 0);
        let cutout_scrim = Color::from_argb(77, 0, 0, 0);
        let accent = theme.color(ThemeRole::Accent);

        let overlay_content: Element = if let Some(box_rect) = current_selection {
            let bx = box_rect.x.clamp(0.0, area_w.max(box_rect.x + box_rect.width));
            let by = box_rect.y.clamp(0.0, area_h.max(box_rect.y + box_rect.height));
            let bw = box_rect.width.max(0.0);
            let bh = box_rect.height.max(0.0);
            let right_x = bx + bw;
            let bottom_y = by + bh;
            let right_w = (area_w - right_x).max(0.0);
            let bottom_h = (area_h - bottom_y).max(0.0);

            rect()
                .width(Size::percent(100.0))
                .height(Size::percent(100.0))
                // Top scrim strip above the selection cutout.
                .child(
                    rect()
                        .position(Position::new_absolute().left(0.0).top(0.0))
                        .width(Size::percent(100.0))
                        .height(Size::px(by))
                        .background(cutout_scrim),
                )
                // Bottom scrim strip below the selection cutout.
                .child(
                    rect()
                        .position(Position::new_absolute().left(0.0).top(bottom_y))
                        .width(Size::percent(100.0))
                        .height(Size::px(bottom_h))
                        .background(cutout_scrim),
                )
                // Left scrim strip beside the selection cutout.
                .child(
                    rect()
                        .position(Position::new_absolute().left(0.0).top(by))
                        .width(Size::px(bx))
                        .height(Size::px(bh))
                        .background(cutout_scrim),
                )
                // Right scrim strip beside the selection cutout.
                .child(
                    rect()
                        .position(Position::new_absolute().left(right_x).top(by))
                        .width(Size::px(right_w))
                        .height(Size::px(bh))
                        .background(cutout_scrim),
                )
                // Clear cutout with 2px accent border.
                .child(
                    rect()
                        .position(Position::new_absolute().left(bx).top(by))
                        .width(Size::px(bw))
                        .height(Size::px(bh))
                        .corner_radius(CornerRadius::new_all(3.0))
                        .background(Color::TRANSPARENT)
                        .border(Border::new().width(2.0).alignment(BorderAlignment::Outer).fill(accent)),
                )
                .into_element()
        } else {
            let banner_shadow = Shadow::new()
                .x(0.0)
                .y(2.0)
                .blur(10.0)
                .color(Color::from_argb(76, 0, 0, 0));

            rect()
                .width(Size::percent(100.0))
                .height(Size::percent(100.0))
                .background(idle_scrim)
                .child(
                    rect()
                        .position(Position::new_absolute().left(0.0).top(16.0))
                        .width(Size::percent(100.0))
                        .direction(Direction::Horizontal)
                        .main_align(Alignment::center())
                        .child(
                            rect()
                                .padding(Gaps::new(6.0, 14.0, 6.0, 14.0))
                                .corner_radius(CornerRadius::new_all(999.0))
                                .background(theme.color(ThemeRole::Surface))
                                .border(
                                    Border::new()
                                        .width(1.0)
                                        .alignment(BorderAlignment::Inner)
                                        .fill(theme.color(ThemeRole::BorderSubtle)),
                                )
                                .shadow(banner_shadow)
                                .child(
                                    label()
                                        .text(REGION_PICKER_HINT)
                                        .font_size(12.0)
                                        .color(theme.color(ThemeRole::Text))
                                        .max_lines(1),
                                ),
                        ),
                )
                .into_element()
        };

        let fallback_a11y = use_hook(AccessibilityId::new_unique);
        let a11y_id = self.focus_a11y_id.unwrap_or(fallback_a11y);
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(self.capture_keys)
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_absolute().left(0.0).top(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .overflow(Overflow::Clip)
            .cursor(CursorIcon::Crosshair)
            .a11y_role(AccessibilityRole::Application)
            .a11y_alt("Drag over what to ask about. Enter takes the whole view, Escape cancels.")
            .on_sized(move |e: Event<SizedEventData>| {
                let next = (e.area.min_x(), e.area.min_y(), e.area.width(), e.area.height());
                if *area.peek() != next {
                    area.set(next);
                }
            })
            .on_mouse_down(on_mouse_down)
            .on_global_pointer_move(on_global_pointer_move)
            .on_global_pointer_up(on_global_pointer_up)
            .on_global_key_down(on_global_key_down)
            .child(overlay_content)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
