/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Per-view rendering data, Hub visibility/viewport/focus lifecycle tracking, and per-view mouse state.
//!
//! Each mounted [`PageSurface`](crate::ui::page::PageSurface) registers its `view_id` here so that:
//! - Mounted views become `Visible` and receive their own measured `ViewSetViewport`.
//! - Views not mounted on screen transition to `Hidden` so the Compositor stops presenting frames for them.
//! - Focus transitions update `ViewSetHasSystemFocus` and `ActiveViewChanged` for the focused surface only.

use crate::frames::{PageBackingStores, Presentation};
use crate::hub;
use crate::input::MouseState;
use freya::elements::image::ImageHandle;
use freya::prelude::*;
use hub_protocol::ipc_types::hub_server::messages as server;
use hub_protocol::{
    Gfx_Color, Gfx_IntSize, Gfx_SharedImage, Web_DevicePixelPoint, Web_DevicePixelSize, Web_HTML_VisibilityState,
    Web_KeyEvent, Web_MouseEvent, Web_UIEvents_KeyModifier as KeyModifier, Web_UIEvents_MouseButton as WebMouseButton,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// Measured layout rectangle of a [`PageSurface`](crate::ui::page::PageSurface) in logical window coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub device_pixel_ratio: f64,
}

impl SurfaceRect {
    pub fn from_area(area: Area, device_pixel_ratio: f64) -> Self {
        Self {
            x: area.min_x(),
            y: area.min_y(),
            width: area.width(),
            height: area.height(),
            device_pixel_ratio,
        }
    }

    pub fn device_size(&self) -> (i32, i32) {
        (
            (self.width as f64 * self.device_pixel_ratio).round() as i32,
            (self.height as f64 * self.device_pixel_ratio).round() as i32,
        )
    }
}

/// Reactive view-local rendering state for a single Hub view.
#[derive(Clone, PartialEq)]
pub struct PageSurfaceData {
    pub frame: Option<ImageHandle>,
    pub frame_seq: u64,
    pub frame_size: (i32, i32),
    pub frame_device_pixel_ratio: f64,
    pub background: (u8, u8, u8),
    pub cursor: CursorIcon,
    pub hovered_link: Option<String>,
}

impl Default for PageSurfaceData {
    fn default() -> Self {
        Self {
            frame: None,
            frame_seq: 0,
            frame_size: (0, 0),
            frame_device_pixel_ratio: 1.0,
            background: (255, 255, 255),
            cursor: CursorIcon::Default,
            hovered_link: None,
        }
    }
}

impl PageSurfaceData {
    pub fn set_background_from_color(&mut self, color: &Gfx_Color) {
        self.background = (color.red(), color.green(), color.blue());
    }
}

/// Sink for Hub IPC calls made by [`PageSurfaceTracker`], abstracted so lifecycle and routing are unit-testable.
pub trait PageHubSink {
    fn set_viewport(&self, view_id: u64, size: (i32, i32), device_pixel_ratio: f64);
    fn set_visible(&self, view_id: u64, visible: bool);
    fn set_focused(&self, view_id: u64, focused: bool);
    fn set_active_view(&self, view_id: Option<u64>);
    fn enqueue_mouse_event(&self, view_id: u64, event: Web_MouseEvent);
    fn enqueue_key_event(&self, view_id: u64, event: Web_KeyEvent);
}

/// Production [`PageHubSink`] backed by the live Hub IPC connection.
pub struct LiveHubSink;

impl PageHubSink for LiveHubSink {
    fn set_viewport(&self, view_id: u64, size: (i32, i32), device_pixel_ratio: f64) {
        hub::connection().post_or_log(server::ViewSetViewport {
            view_id,
            size: Web_DevicePixelSize {
                width: size.0,
                height: size.1,
            },
            device_pixel_ratio,
        });
    }

    fn set_visible(&self, view_id: u64, visible: bool) {
        let hub = hub::connection();
        hub.post_or_log(server::ViewSetSystemVisibilityState {
            view_id,
            visibility_state: if visible {
                Web_HTML_VisibilityState::Visible
            } else {
                Web_HTML_VisibilityState::Hidden
            },
        });
        hub.post_or_log(server::ViewSetCurrentTabOfWindow {
            view_id,
            current_tab_of_window: visible,
        });
    }

    fn set_focused(&self, view_id: u64, focused: bool) {
        hub::connection().post_or_log(server::ViewSetHasSystemFocus {
            view_id,
            has_system_focus: focused,
        });
    }

    fn set_active_view(&self, view_id: Option<u64>) {
        hub::connection().post_or_log(server::ActiveViewChanged { view_id });
    }

    fn enqueue_mouse_event(&self, view_id: u64, event: Web_MouseEvent) {
        hub::connection().post_or_log(server::ViewEnqueueMouseEvent {
            view_id,
            input_id: 0,
            event,
        });
    }

    fn enqueue_key_event(&self, view_id: u64, event: Web_KeyEvent) {
        hub::connection().post_or_log(server::ViewEnqueueKeyEvent {
            view_id,
            input_id: 0,
            event,
        });
    }
}

/// Tracks per-view backing stores, measured rects, mount counts (visibility), focus, and mouse states.
pub struct PageSurfaceTracker {
    sink: Rc<dyn PageHubSink>,
    backing_stores: HashMap<u64, PageBackingStores>,
    mouse_states: HashMap<u64, MouseState>,
    modifiers: KeyModifier,
    mounted_counts: HashMap<u64, usize>,
    rects: HashMap<u64, SurfaceRect>,
    sent_viewports: HashMap<u64, ((i32, i32), f64)>,
    focused_view: Option<u64>,
}

impl PageSurfaceTracker {
    pub fn new(sink: Rc<dyn PageHubSink>) -> Self {
        Self {
            sink,
            backing_stores: HashMap::new(),
            mouse_states: HashMap::new(),
            modifiers: KeyModifier::Mod_None,
            mounted_counts: HashMap::new(),
            rects: HashMap::new(),
            sent_viewports: HashMap::new(),
            focused_view: None,
        }
    }

    /// Marks `view_id` as mounted on screen, making it `Visible` on its first mount.
    pub fn mount_surface(&mut self, view_id: u64) {
        let count = self.mounted_counts.entry(view_id).or_insert(0);
        let became_visible = *count == 0;
        *count += 1;
        if became_visible {
            self.sink.set_visible(view_id, true);
        }
    }

    /// Unmounts one surface showing `view_id`; when no surface shows it, hides it so frames stop arriving.
    pub fn unmount_surface(&mut self, view_id: u64) {
        let Some(count) = self.mounted_counts.get_mut(&view_id) else {
            return;
        };
        *count = count.saturating_sub(1);
        if *count == 0 {
            self.mounted_counts.remove(&view_id);
            self.rects.remove(&view_id);
            if self.focused_view == Some(view_id) {
                self.focused_view = None;
                self.sink.set_focused(view_id, false);
            }
            self.sink.set_visible(view_id, false);
        }
    }

    /// Records the measured layout rect of `view_id` and sends `ViewSetViewport` for that view only when changed.
    pub fn set_surface_rect(&mut self, view_id: u64, rect: SurfaceRect) -> bool {
        self.rects.insert(view_id, rect);
        let size = rect.device_size();
        if size.0 <= 0 || size.1 <= 0 {
            return false;
        }
        let next = (size, rect.device_pixel_ratio);
        if self.sent_viewports.get(&view_id) == Some(&next) {
            return false;
        }
        self.sent_viewports.insert(view_id, next);
        self.sink.set_viewport(view_id, size, rect.device_pixel_ratio);
        true
    }

    /// Returns the last measured logical rect of a mounted view (for overlays and hint positioning).
    #[allow(dead_code)]
    pub fn surface_rect(&self, view_id: u64) -> Option<SurfaceRect> {
        self.rects.get(&view_id).copied()
    }

    /// Finds the mounted view whose measured rect contains logical window coordinate `(x, y)`.
    pub fn hit_test_view(&self, x: f32, y: f32) -> Option<(u64, SurfaceRect)> {
        self.rects.iter().find_map(|(&view_id, &rect)| {
            let hit = x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height;
            hit.then_some((view_id, rect))
        })
    }

    /// Returns whether `view_id` is currently mounted on screen in at least one surface.
    #[allow(dead_code)]
    pub fn is_mounted(&self, view_id: u64) -> bool {
        self.mounted_counts.get(&view_id).copied().unwrap_or(0) > 0
    }

    /// Moves system keyboard focus to `view_id` (or clears page focus when `None`).
    pub fn set_focused_view(&mut self, view_id: Option<u64>) {
        if self.focused_view == view_id {
            return;
        }
        if let Some(previous) = self.focused_view.take() {
            self.sink.set_focused(previous, false);
        }
        if let Some(next) = view_id {
            self.focused_view = Some(next);
            self.sink.set_focused(next, true);
            self.sink.set_active_view(Some(next));
        }
    }

    /// Clears system focus only if `view_id` is currently the focused view.
    pub fn clear_focused_view_if(&mut self, view_id: u64) {
        if self.focused_view == Some(view_id) {
            self.set_focused_view(None);
        }
    }

    #[allow(dead_code)]
    pub fn focused_view(&self) -> Option<u64> {
        self.focused_view
    }

    /// Removes all resources for a closed Hub view without posting messages to the destroyed view.
    pub fn remove_closed_view(&mut self, view_id: u64) {
        self.backing_stores.remove(&view_id);
        self.mouse_states.remove(&view_id);
        self.mounted_counts.remove(&view_id);
        self.rects.remove(&view_id);
        self.sent_viewports.remove(&view_id);
        if self.focused_view == Some(view_id) {
            self.focused_view = None;
        }
    }

    pub fn set_modifiers(&mut self, modifiers: KeyModifier) {
        self.modifiers = modifiers;
        for mouse in self.mouse_states.values_mut() {
            mouse.modifiers = modifiers;
        }
    }

    fn mouse_for(&mut self, view_id: u64) -> &mut MouseState {
        let modifiers = self.modifiers;
        let mouse = self.mouse_states.entry(view_id).or_default();
        mouse.modifiers = modifiers;
        mouse
    }

    pub fn send_mouse_down(
        &mut self,
        view_id: u64,
        position: Web_DevicePixelPoint,
        screen_position: Web_DevicePixelPoint,
        button: WebMouseButton,
    ) {
        let event = self.mouse_for(view_id).down(position, screen_position, button);
        self.sink.enqueue_mouse_event(view_id, event);
    }

    pub fn send_mouse_up(
        &mut self,
        view_id: u64,
        position: Web_DevicePixelPoint,
        screen_position: Web_DevicePixelPoint,
        button: WebMouseButton,
    ) {
        let event = self.mouse_for(view_id).up(position, screen_position, button);
        self.sink.enqueue_mouse_event(view_id, event);
    }

    pub fn send_mouse_move(
        &mut self,
        view_id: u64,
        position: Web_DevicePixelPoint,
        screen_position: Web_DevicePixelPoint,
    ) {
        let event = self.mouse_for(view_id).moved(position, screen_position);
        self.sink.enqueue_mouse_event(view_id, event);
    }

    pub fn send_mouse_leave(&mut self, view_id: u64) {
        let event = self.mouse_for(view_id).left();
        self.sink.enqueue_mouse_event(view_id, event);
    }

    pub fn send_wheel(
        &mut self,
        view_id: u64,
        position: Web_DevicePixelPoint,
        screen_position: Web_DevicePixelPoint,
        delta: (f64, f64),
        source: WheelSource,
        device_pixel_ratio: f64,
    ) {
        let event =
            self.mouse_for(view_id)
                .wheel(position, screen_position, delta.0, delta.1, source, device_pixel_ratio);
        self.sink.enqueue_mouse_event(view_id, event);
    }

    pub fn send_key(&self, view_id: u64, event: Web_KeyEvent) {
        self.sink.enqueue_key_event(view_id, event);
    }

    pub fn install_backing_stores(
        &mut self,
        view_id: u64,
        page_id: u64,
        bitmap_ids: Vec<i32>,
        backing_stores: Vec<Gfx_SharedImage>,
    ) {
        self.backing_stores
            .entry(view_id)
            .or_default()
            .install(page_id, bitmap_ids, backing_stores);
    }

    pub fn add_backing_stores(&mut self, view_id: u64, bitmap_ids: Vec<i32>, backing_stores: Vec<Gfx_SharedImage>) {
        self.backing_stores
            .entry(view_id)
            .or_default()
            .add(bitmap_ids, backing_stores);
    }

    pub fn retire_backing_stores(&mut self, view_id: u64, bitmap_ids: &[i32]) {
        if let Some(stores) = self.backing_stores.get_mut(&view_id) {
            stores.retire(bitmap_ids);
        }
    }

    pub fn clear_backing_stores(&mut self, view_id: u64) {
        if let Some(stores) = self.backing_stores.get_mut(&view_id) {
            stores.clear();
        }
    }

    pub fn present_backing_store(
        &mut self,
        view_id: u64,
        page_id: u64,
        bitmap_id: i32,
        size: Gfx_IntSize,
    ) -> Presentation {
        match self.backing_stores.get_mut(&view_id) {
            Some(stores) if stores.page_id() == page_id => stores.present(bitmap_id, size),
            _ => Presentation {
                frame: None,
                release: Some(bitmap_id),
            },
        }
    }
}

/// Coordinates `RefCell`-backed surface focus state without holding any borrow across nested mutations or Freya calls.
#[derive(Clone)]
struct SurfaceFocusCoordinator<Id: Copy + PartialEq = AccessibilityId> {
    tracker: Rc<RefCell<PageSurfaceTracker>>,
    surface_a11y: Rc<RefCell<HashMap<u64, Id>>>,
    preferred_view: Rc<RefCell<Option<u64>>>,
    pending_focus_view: Rc<RefCell<Option<u64>>>,
    core: Rc<RefCell<Option<Arc<dyn zendia_browser_core::BrowserCore>>>>,
}

impl<Id: Copy + PartialEq> SurfaceFocusCoordinator<Id> {
    fn new(sink: Rc<dyn PageHubSink>) -> Self {
        Self {
            tracker: Rc::new(RefCell::new(PageSurfaceTracker::new(sink))),
            surface_a11y: Rc::new(RefCell::new(HashMap::new())),
            preferred_view: Rc::new(RefCell::new(None)),
            pending_focus_view: Rc::new(RefCell::new(None)),
            core: Rc::new(RefCell::new(None)),
        }
    }

    fn set_core(&self, core: Arc<dyn zendia_browser_core::BrowserCore>) {
        *self.core.borrow_mut() = Some(core);
    }

    fn record_focused_surface(&self, view_id: u64) {
        *self.preferred_view.borrow_mut() = Some(view_id);
        self.tracker.borrow_mut().set_focused_view(Some(view_id));
    }

    fn register_view_surface(&self, view_id: u64, a11y_id: Id, has_last_focused: bool) -> bool {
        self.surface_a11y.borrow_mut().insert(view_id, a11y_id);
        let pending_matches = *self.pending_focus_view.borrow() == Some(view_id);
        let none_focused = !has_last_focused && self.tracker.borrow().focused_view().is_none();
        if pending_matches || none_focused {
            *self.pending_focus_view.borrow_mut() = None;
            self.record_focused_surface(view_id);
            true
        } else {
            false
        }
    }

    fn unregister_view_surface(&self, view_id: u64, a11y_id: Id) {
        let mut map = self.surface_a11y.borrow_mut();
        if map.get(&view_id) == Some(&a11y_id) {
            map.remove(&view_id);
        }
    }

    fn focus_view(&self, view_id: u64) -> Option<Id> {
        *self.preferred_view.borrow_mut() = Some(view_id);
        let existing_a11y = self.surface_a11y.borrow().get(&view_id).copied();
        if let Some(a11y_id) = existing_a11y {
            *self.pending_focus_view.borrow_mut() = None;
            self.record_focused_surface(view_id);
            Some(a11y_id)
        } else {
            *self.pending_focus_view.borrow_mut() = Some(view_id);
            None
        }
    }

    fn active_model_view_id(&self) -> Option<u64> {
        let core = self.core.borrow().clone()?;
        let model = core.model();
        let tid = model.effective_keyed_tab()?;
        Some(model.tab(tid).and_then(|t| t.view_id).unwrap_or(tid.0))
    }

    fn request_focus_active_surface(&self) -> Option<(u64, Id)> {
        if let Some(vid) = self.active_model_view_id() {
            return self.focus_view(vid).map(|a11y_id| (vid, a11y_id));
        }
        let preferred = {
            let vid = *self.preferred_view.borrow();
            vid.and_then(|v| self.surface_a11y.borrow().get(&v).copied().map(|id| (v, id)))
        };
        if let Some((vid, a11y_id)) = preferred {
            self.record_focused_surface(vid);
            return Some((vid, a11y_id));
        }
        None
    }

    fn remove_closed_view(&self, view_id: u64) {
        self.surface_a11y.borrow_mut().remove(&view_id);
        let clear_pref = *self.preferred_view.borrow() == Some(view_id);
        if clear_pref {
            *self.preferred_view.borrow_mut() = None;
        }
        let clear_pend = *self.pending_focus_view.borrow() == Some(view_id);
        if clear_pend {
            *self.pending_focus_view.borrow_mut() = None;
        }
        self.tracker.borrow_mut().remove_closed_view(view_id);
    }
}

/// Freya context shared by all [`PageSurface`](crate::ui::page::PageSurface) instances and the Hub message handler.
#[derive(Clone)]
pub struct PageSurfaceContext {
    surfaces: State<HashMap<u64, PageSurfaceData>>,
    last_focused_a11y: State<Option<AccessibilityId>>,
    focus_coord: SurfaceFocusCoordinator,
}

impl PartialEq for PageSurfaceContext {
    fn eq(&self, other: &Self) -> bool {
        self.surfaces == other.surfaces
            && Rc::ptr_eq(&self.focus_coord.tracker, &other.focus_coord.tracker)
            && self.last_focused_a11y == other.last_focused_a11y
    }
}

impl PageSurfaceContext {
    pub fn set_core(&self, core: Arc<dyn zendia_browser_core::BrowserCore>) {
        self.focus_coord.set_core(core);
    }

    pub fn surfaces(&self) -> State<HashMap<u64, PageSurfaceData>> {
        self.surfaces
    }

    pub fn tracker(&self) -> &Rc<RefCell<PageSurfaceTracker>> {
        &self.focus_coord.tracker
    }

    pub fn record_focused_surface(&self, a11y_id: AccessibilityId, view_id: u64) {
        let mut last = self.last_focused_a11y;
        last.set_if_modified(Some(a11y_id));
        self.focus_coord.record_focused_surface(view_id);
    }

    pub fn clear_focused_view_if(&self, view_id: u64) {
        self.focus_coord.tracker.borrow_mut().clear_focused_view_if(view_id);
    }

    /// Registers a mounted [`PageSurface`]'s `a11y_id` for `view_id`, granting focus immediately if `view_id` was
    /// queued for focus or if no surface in the window has been focused yet (`INS-04`, `EXT-03`).
    pub fn register_view_surface(&self, view_id: u64, a11y_id: AccessibilityId) {
        let has_last = self.last_focused_a11y.peek().is_some();
        if self.focus_coord.register_view_surface(view_id, a11y_id, has_last) {
            a11y_id.request_focus();
            let mut last = self.last_focused_a11y;
            last.set_if_modified(Some(a11y_id));
        }
    }

    pub fn unregister_view_surface(&self, view_id: u64, a11y_id: AccessibilityId) {
        self.focus_coord.unregister_view_surface(view_id, a11y_id);
    }

    /// Moves keyboard focus to `view_id`'s [`PageSurface`] (or queues it if the surface mounts on the next render).
    pub fn focus_view(&self, view_id: u64) {
        if let Some(a11y_id) = self.focus_coord.focus_view(view_id) {
            a11y_id.request_focus();
            let mut last = self.last_focused_a11y;
            last.set_if_modified(Some(a11y_id));
        }
    }

    /// Moves keyboard focus to the surface displaying `model.effective_keyed_tab()` (`INS-04`, `PG-02`, `EXT-03`).
    pub fn focus_effective_model_surface(&self, model: &zendia_browser_core::model::BrowserModel) {
        let target_view = model
            .effective_keyed_tab()
            .and_then(|tid| model.tab(tid).and_then(|t| t.view_id).or(Some(tid.0)));
        if let Some(view_id) = target_view {
            self.focus_view(view_id);
        }
    }

    /// Requests Freya focus on the active mounted page surface (e.g. after closing a picker, panel, library, or URL bar).
    pub fn request_focus_active_surface(&self) {
        if let Some((_vid, a11y_id)) = self.focus_coord.request_focus_active_surface() {
            a11y_id.request_focus();
            let mut last = self.last_focused_a11y;
            last.set_if_modified(Some(a11y_id));
            return;
        }
        if self.focus_coord.active_model_view_id().is_some() {
            return;
        }
        let fallback = *self.last_focused_a11y.peek();
        if let Some(a11y_id) = fallback {
            a11y_id.request_focus();
        }
    }

    pub fn register_surface_a11y(&self, a11y_id: AccessibilityId) {
        let mut last = self.last_focused_a11y;
        if last.peek().is_none() {
            last.set(Some(a11y_id));
        }
    }

    pub fn remove_closed_view(&self, view_id: u64) {
        self.focus_coord.remove_closed_view(view_id);
        let mut surfaces = self.surfaces;
        surfaces.write().remove(&view_id);
    }
}

/// Installs [`PageSurfaceContext`] into Freya context backed by [`LiveHubSink`].
pub fn use_provide_page_surfaces() -> PageSurfaceContext {
    let surfaces = use_state(HashMap::<u64, PageSurfaceData>::new);
    let last_focused_a11y = use_state(|| None::<AccessibilityId>);
    let focus_coord = use_hook(|| SurfaceFocusCoordinator::new(Rc::new(LiveHubSink)));

    use_provide_context(|| PageSurfaceContext {
        surfaces,
        last_focused_a11y,
        focus_coord,
    })
}

/// Consumes [`PageSurfaceContext`] from Freya context.
pub fn use_page_surfaces() -> PageSurfaceContext {
    use_consume::<PageSurfaceContext>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MockHubSink {
        calls: RefCell<Vec<String>>,
    }

    impl MockHubSink {
        fn take_calls(&self) -> Vec<String> {
            std::mem::take(&mut *self.calls.borrow_mut())
        }
    }

    impl PageHubSink for MockHubSink {
        fn set_viewport(&self, view_id: u64, size: (i32, i32), device_pixel_ratio: f64) {
            self.calls
                .borrow_mut()
                .push(format!("viewport:{view_id}:{}x{}@{device_pixel_ratio}", size.0, size.1));
        }

        fn set_visible(&self, view_id: u64, visible: bool) {
            self.calls.borrow_mut().push(format!("visible:{view_id}:{visible}"));
        }

        fn set_focused(&self, view_id: u64, focused: bool) {
            self.calls.borrow_mut().push(format!("focus:{view_id}:{focused}"));
        }

        fn set_active_view(&self, view_id: Option<u64>) {
            self.calls.borrow_mut().push(format!("active:{view_id:?}"));
        }

        fn enqueue_mouse_event(&self, view_id: u64, event: Web_MouseEvent) {
            self.calls
                .borrow_mut()
                .push(format!("mouse:{view_id}:{},{}", event.position.x, event.position.y));
        }

        fn enqueue_key_event(&self, view_id: u64, _event: Web_KeyEvent) {
            self.calls.borrow_mut().push(format!("key:{view_id}"));
        }
    }

    #[test]
    fn mounts_two_views_independently_and_hides_unmounted_view() {
        let sink = Rc::new(MockHubSink::default());
        let mut tracker = PageSurfaceTracker::new(Rc::clone(&sink) as Rc<dyn PageHubSink>);

        tracker.mount_surface(1);
        tracker.mount_surface(2);
        assert!(tracker.is_mounted(1));
        assert!(tracker.is_mounted(2));
        assert_eq!(sink.take_calls(), vec!["visible:1:true", "visible:2:true"]);

        let left_rect = SurfaceRect {
            x: 0.0,
            y: 82.0,
            width: 640.0,
            height: 778.0,
            device_pixel_ratio: 1.0,
        };
        let right_rect = SurfaceRect {
            x: 642.0,
            y: 82.0,
            width: 638.0,
            height: 778.0,
            device_pixel_ratio: 2.0,
        };

        assert!(tracker.set_surface_rect(1, left_rect));
        assert!(!tracker.set_surface_rect(1, left_rect));
        assert!(tracker.set_surface_rect(2, right_rect));
        assert_eq!(
            sink.take_calls(),
            vec!["viewport:1:640x778@1", "viewport:2:1276x1556@2"]
        );

        // Unmounting view 1 hides only view 1; view 2 stays visible on screen.
        tracker.unmount_surface(1);
        assert!(!tracker.is_mounted(1));
        assert!(tracker.is_mounted(2));
        assert_eq!(sink.take_calls(), vec!["visible:1:false"]);
    }

    #[test]
    fn routes_focus_and_mouse_events_per_view() {
        let sink = Rc::new(MockHubSink::default());
        let mut tracker = PageSurfaceTracker::new(Rc::clone(&sink) as Rc<dyn PageHubSink>);

        tracker.mount_surface(10);
        tracker.mount_surface(20);
        let _ = sink.take_calls();

        tracker.set_focused_view(Some(10));
        tracker.set_focused_view(Some(20));
        assert_eq!(
            sink.take_calls(),
            vec![
                "focus:10:true",
                "active:Some(10)",
                "focus:10:false",
                "focus:20:true",
                "active:Some(20)",
            ]
        );

        tracker.send_mouse_down(
            10,
            Web_DevicePixelPoint { x: 12, y: 34 },
            Web_DevicePixelPoint { x: 12, y: 116 },
            WebMouseButton::Primary,
        );
        tracker.send_mouse_move(
            20,
            Web_DevicePixelPoint { x: 50, y: 60 },
            Web_DevicePixelPoint { x: 690, y: 142 },
        );
        assert_eq!(sink.take_calls(), vec!["mouse:10:12,34", "mouse:20:50,60"]);

        // Closing a focused view clears focus without emitting IPC for the destroyed view.
        tracker.remove_closed_view(20);
        assert_eq!(tracker.focused_view(), None);
        assert!(sink.take_calls().is_empty());
    }

    #[test]
    fn focus_coordinator_never_double_borrows_on_panel_close_refocus() {
        let sink = Rc::new(MockHubSink::default());
        let coord = SurfaceFocusCoordinator::<u64>::new(Rc::clone(&sink) as Rc<dyn PageHubSink>);
        coord.tracker.borrow_mut().mount_surface(1);
        let _ = sink.take_calls();

        let a11y_1 = 101u64;
        assert!(coord.register_view_surface(1, a11y_1, false));
        assert_eq!(sink.take_calls(), vec!["focus:1:true", "active:Some(1)"]);

        // Without `core` attached (hits the `preferred_view` fallback path that previously panicked on panel close).
        coord.tracker.borrow_mut().clear_focused_view_if(1);
        let _ = sink.take_calls();
        assert_eq!(coord.request_focus_active_surface(), Some((1, a11y_1)));
        assert_eq!(sink.take_calls(), vec!["focus:1:true", "active:Some(1)"]);

        // With `StubBackend` attached as `core` (resolves the active tab's view and focuses it cleanly).
        use zendia_browser_core::BrowserCore;
        let stub = Arc::new(zendia_browser_core::stub::StubBackend::new());
        let active_vid = {
            let model = stub.model();
            let tid = model.effective_keyed_tab().expect("active tab");
            model.tab(tid).and_then(|t| t.view_id).unwrap_or(tid.0)
        };
        coord.tracker.borrow_mut().mount_surface(active_vid);
        let a11y_active = 102u64;
        let _ = coord.register_view_surface(active_vid, a11y_active, true);
        coord.set_core(stub);
        let _ = sink.take_calls();

        assert_eq!(coord.request_focus_active_surface(), Some((active_vid, a11y_active)));
        coord.remove_closed_view(active_vid);
        assert_eq!(*coord.preferred_view.borrow(), None);
    }
}
