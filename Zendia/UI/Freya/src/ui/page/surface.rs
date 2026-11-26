/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! [`PageSurface`]: self-measuring, self-routing Freya component displaying one Hub web view.

use crate::input;
use crate::ui::core_view::use_core;
use crate::ui::page::keymap_context::use_keymap;
use crate::ui::page::surface_store::{SurfaceRect, use_page_surfaces};
use crate::ui::panels::try_use_dock_state;
use crate::ui::theme::{ThemeRole, use_theme};
use crate::ui::vim::try_use_vim_statusline_state;
use freya::elements::image::image;
use freya::prelude::*;
use hub_protocol::Web_DevicePixelPoint;
use std::cell::RefCell;
use std::rc::Rc;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::TabId;

/// Identifies which Hub view a [`PageSurface`] displays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceTarget {
    /// Resolves the live Hub `view_id` from a tab in [`BrowserCore`](zendia_browser_core::BrowserCore).
    Tab(TabId),
    /// Binds directly to a Hub `view_id` (used for non-tab web views such as Glance and web chat panels).
    #[allow(dead_code)]
    View(u64),
}

/// The command a clipboard key runs on a page: cut, copy, paste, paste as plain text and select all, with Ctrl, or
/// with Cmd on macOS.
fn clipboard_command(e: &KeyboardEventData, tab_id: TabId) -> Option<Command> {
    let accel = e.modifiers.contains(Modifiers::CONTROL) || e.modifiers.contains(Modifiers::META);
    if !accel || e.modifiers.contains(Modifiers::ALT) {
        return None;
    }
    let shift = e.modifiers.contains(Modifiers::SHIFT);
    let tab_id = Some(tab_id);
    match (e.code, shift) {
        (Code::KeyX, false) => Some(Command::PageCut { tab_id }),
        (Code::KeyC, false) => Some(Command::PageCopy { tab_id }),
        (Code::KeyV, false) => Some(Command::PagePaste {
            tab_id,
            match_style: false,
        }),
        (Code::KeyV, true) => Some(Command::PagePaste {
            tab_id,
            match_style: true,
        }),
        (Code::KeyA, false) => Some(Command::PageSelectAll { tab_id }),
        _ => None,
    }
}

fn device_point(location: CursorPoint, device_pixel_ratio: f64) -> Web_DevicePixelPoint {
    Web_DevicePixelPoint {
        x: (location.x * device_pixel_ratio).round() as i32,
        y: (location.y * device_pixel_ratio).round() as i32,
    }
}

/// Displays a single Hub web view at its own measured layout rect, manages its visibility, and routes pointer, wheel,
/// and keyboard events exclusively to that view.
#[derive(Clone, PartialEq)]
pub struct PageSurface {
    target: SurfaceTarget,
    a11y_id: Option<AccessibilityId>,
    width: Size,
    height: Size,
    show_focus_ring: bool,
    key: DiffKey,
}

impl KeyExt for PageSurface {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl PageSurface {
    pub fn for_tab(tab_id: TabId) -> Self {
        Self {
            target: SurfaceTarget::Tab(tab_id),
            a11y_id: None,
            width: Size::fill(),
            height: Size::fill(),
            show_focus_ring: false,
            key: DiffKey::None,
        }
    }

    #[allow(dead_code)]
    pub fn for_view(view_id: u64) -> Self {
        Self {
            target: SurfaceTarget::View(view_id),
            a11y_id: None,
            width: Size::fill(),
            height: Size::fill(),
            show_focus_ring: false,
            key: DiffKey::None,
        }
    }

    #[allow(dead_code)]
    pub fn a11y_id(mut self, a11y_id: AccessibilityId) -> Self {
        self.a11y_id = Some(a11y_id);
        self
    }

    pub fn width(mut self, width: Size) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: Size) -> Self {
        self.height = height;
        self
    }

    pub fn show_focus_ring(mut self, show: bool) -> Self {
        self.show_focus_ring = show;
        self
    }
}

impl Component for PageSurface {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let keymap = use_keymap();
        let page_ctx = use_page_surfaces();
        let dock_state = try_use_dock_state();
        let vim_state = try_use_vim_statusline_state();

        let fallback_a11y_id = use_hook(AccessibilityId::new_unique);
        let a11y_id = self.a11y_id.unwrap_or(fallback_a11y_id);
        page_ctx.register_surface_a11y(a11y_id);

        let view_id = match self.target {
            SurfaceTarget::Tab(tab_id) => core.model().read().tab(tab_id).and_then(|tab| tab.view_id),
            SurfaceTarget::View(view_id) => Some(view_id),
        };

        let mounted_view: Rc<RefCell<Option<u64>>> = use_hook(|| Rc::new(RefCell::new(None)));
        let last_rect: Rc<RefCell<Option<SurfaceRect>>> = use_hook(|| Rc::new(RefCell::new(None)));

        // Sync visibility/viewport lifecycle when this component mounts or switches to a different view_id.
        {
            let mut current_mounted = mounted_view.borrow_mut();
            if *current_mounted != view_id {
                let mut tracker = page_ctx.tracker().borrow_mut();
                if let Some(old_id) = *current_mounted {
                    tracker.unmount_surface(old_id);
                }
                if let Some(new_id) = view_id {
                    tracker.mount_surface(new_id);
                    if let Some(rect) = *last_rect.borrow() {
                        tracker.set_surface_rect(new_id, rect);
                    }
                }
                *current_mounted = view_id;
            }
        }

        // Unmount on component drop so off-screen views transition to Hidden and stop producing frames.
        use_drop({
            let mounted_view = Rc::clone(&mounted_view);
            let tracker = Rc::clone(page_ctx.tracker());
            move || {
                if let Some(old_id) = mounted_view.borrow_mut().take() {
                    tracker.borrow_mut().unmount_surface(old_id);
                }
            }
        });

        // Keep Hub system focus aligned with Freya focus for this surface.
        let focus = use_focus(a11y_id);
        let is_focused = focus() != Focus::Not;
        if let Some(view_id) = view_id {
            if is_focused {
                page_ctx.record_focused_surface(a11y_id, view_id);
            } else {
                page_ctx.clear_focused_view_if(view_id);
            }
        }

        let on_sized = {
            let tracker = Rc::clone(page_ctx.tracker());
            let last_rect = Rc::clone(&last_rect);
            move |e: Event<SizedEventData>| {
                let dpr = *Platform::get().current_window().scale_factor.peek();
                let measured = SurfaceRect::from_area(e.area, dpr);
                *last_rect.borrow_mut() = Some(measured);
                if let Some(view_id) = view_id {
                    tracker.borrow_mut().set_surface_rect(view_id, measured);
                }
            }
        };

        let target = self.target;
        let on_mouse_down = {
            let page_ctx = page_ctx.clone();
            let core = core.clone();
            move |e: Event<MouseEventData>| {
                if dock_state.is_some_and(|ds| ds.is_any_panel_resizing()) {
                    return;
                }
                a11y_id.request_focus();
                if let Some(view_id) = view_id {
                    page_ctx.record_focused_surface(a11y_id, view_id);
                }
                if let SurfaceTarget::Tab(tab_id) = target
                    && core.model().peek().active_tab != Some(tab_id)
                {
                    core.invoke_or_log(Command::TabActivate { tab_id });
                }
                // A click ends a jump, or its labels would go on taking the keys.
                if core.model().peek().page_hints.is_some() {
                    core.invoke_or_log(Command::Page(
                        zendia_browser_core::page_actions::PageAction::PageHintCancel,
                    ));
                }
                let (Some(view_id), Some(button)) = (view_id, e.button) else {
                    return;
                };
                let dpr = *Platform::get().current_window().scale_factor.peek();
                page_ctx.tracker().borrow_mut().send_mouse_down(
                    view_id,
                    device_point(e.element_location, dpr),
                    device_point(e.global_location, dpr),
                    input::mouse_button(button),
                );
            }
        };

        let on_mouse_up = {
            let tracker = Rc::clone(page_ctx.tracker());
            move |e: Event<MouseEventData>| {
                if dock_state.is_some_and(|ds| ds.is_any_panel_resizing()) {
                    return;
                }
                let (Some(view_id), Some(button)) = (view_id, e.button) else {
                    return;
                };
                let dpr = *Platform::get().current_window().scale_factor.peek();
                tracker.borrow_mut().send_mouse_up(
                    view_id,
                    device_point(e.element_location, dpr),
                    device_point(e.global_location, dpr),
                    input::mouse_button(button),
                );
            }
        };

        let on_mouse_move = {
            let tracker = Rc::clone(page_ctx.tracker());
            move |e: Event<MouseEventData>| {
                if dock_state.is_some_and(|ds| ds.is_any_panel_resizing()) {
                    return;
                }
                let Some(view_id) = view_id else { return };
                let dpr = *Platform::get().current_window().scale_factor.peek();
                tracker.borrow_mut().send_mouse_move(
                    view_id,
                    device_point(e.element_location, dpr),
                    device_point(e.global_location, dpr),
                );
            }
        };

        let on_pointer_leave = {
            let tracker = Rc::clone(page_ctx.tracker());
            move |_: Event<PointerEventData>| {
                let Some(view_id) = view_id else { return };
                tracker.borrow_mut().send_mouse_leave(view_id);
            }
        };

        let on_wheel = {
            let tracker = Rc::clone(page_ctx.tracker());
            let core = core.clone();
            move |e: Event<WheelEventData>| {
                if let Some(state) = vim_state {
                    state.notify_page_scroll();
                }
                crate::ui::vim::follow_page_scroll(&core);
                let Some(view_id) = view_id else { return };
                let dpr = *Platform::get().current_window().scale_factor.peek();
                tracker.borrow_mut().send_wheel(
                    view_id,
                    device_point(e.element_location, dpr),
                    device_point(e.global_location, dpr),
                    (e.delta_x, e.delta_y),
                    e.source,
                    dpr,
                );
            }
        };

        let on_key_down = {
            let keymap = keymap.clone();
            let core = core.clone();
            let tracker = Rc::clone(page_ctx.tracker());
            move |e: Event<KeyboardEventData>| {
                let model = core.model().peek();
                if crate::ui::vim::surface_keeps_keys(&model) {
                    return;
                }
                if keymap.will_consume_key_down(&e, &model) {
                    return;
                }
                drop(model);
                let Some(view_id) = view_id else { return };
                // The page leaves the clipboard keys to the browser, as Ladybird's own windows bind them to their Edit
                // actions: they act on the page that has the keys, in any mode a keymap leaves them to.
                if let Some(command) = clipboard_command(&e, TabId(view_id)) {
                    keymap.consume_key_up(e.code);
                    core.invoke_or_log(command);
                    return;
                }
                let event = input::key_event(true, &e.key, e.code, e.modifiers);
                tracker.borrow().send_key(view_id, event);
            }
        };

        let on_key_up = {
            let keymap = keymap.clone();
            let tracker = Rc::clone(page_ctx.tracker());
            move |e: Event<KeyboardEventData>| {
                if keymap.take_consumed_key_up(e.code) {
                    return;
                }
                let Some(view_id) = view_id else { return };
                let event = input::key_event(false, &e.key, e.code, e.modifiers);
                tracker.borrow().send_key(view_id, event);
            }
        };

        let (background, cursor, frame, hovered_link) = {
            let surfaces = page_ctx.surfaces();
            let surfaces = surfaces.read();
            match view_id.and_then(|id| surfaces.get(&id)) {
                Some(surface) => (
                    surface.background,
                    surface.cursor,
                    surface
                        .frame
                        .clone()
                        .map(|frame| (frame, surface.frame_size, surface.frame_device_pixel_ratio)),
                    surface.hovered_link.clone(),
                ),
                None => ((255, 255, 255), CursorIcon::Default, None, None),
            }
        };

        // The page's viewport in logical pixels, which jump labels stay inside.
        let viewport = frame.as_ref().map_or((0.0, 0.0), |(_, (width, height), dpr)| {
            ((*width as f64 / dpr) as f32, (*height as f64 / dpr) as f32)
        });

        let panel_resizing = dock_state.is_some_and(|ds| !ds.slide_resizing.read().is_empty());
        let effective_cursor = if panel_resizing { CursorIcon::ColResize } else { cursor };

        let surface_rect = rect()
            .width(self.width.clone())
            .height(self.height.clone())
            .overflow(Overflow::Clip)
            .background(background)
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Document)
            .cursor(effective_cursor)
            .on_sized(on_sized)
            .on_mouse_down(on_mouse_down)
            .on_mouse_up(on_mouse_up)
            .on_mouse_move(on_mouse_move)
            .on_pointer_leave(on_pointer_leave)
            .on_wheel(on_wheel)
            .on_key_down(on_key_down)
            .on_key_up(on_key_up)
            .maybe_child(frame.map(|(frame, (width, height), frame_dpr)| {
                image(frame)
                    .width(Size::px((width as f64 / frame_dpr) as f32))
                    .height(Size::px((height as f64 / frame_dpr) as f32))
            }))
            .child(crate::ui::vim::page_hints::PageHintsOverlay::new(
                view_id.map(zendia_browser_core::model::TabId),
                viewport,
            ))
            .maybe_child(hovered_link.map(|link| {
                rect()
                    .position(Position::new_absolute().left(0.).bottom(0.))
                    .padding(Gaps::new(3., 8., 3., 8.))
                    .background(theme.color(ThemeRole::SurfaceRaised))
                    .color(theme.color(ThemeRole::Text))
                    .corner_radius(CornerRadius::new_all(4.))
                    .child(label().text(link).font_size(12.).max_lines(1))
            }));

        if self.show_focus_ring && is_focused {
            surface_rect.border(
                Border::new()
                    .width(2.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::FocusRing)),
            )
        } else {
            surface_rect
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
