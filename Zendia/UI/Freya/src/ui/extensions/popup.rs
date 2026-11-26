/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Anchored floating popover hosting a WebExtension's live HTML popup page via a Hub auxiliary [`PageSurface`].

use crate::ui::page::PageSurface;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ThemeRole, use_theme};
use freya::prelude::*;
use std::rc::Rc;
use zendia_browser_core::modules::ExtensionId;

const DEFAULT_POPUP_WIDTH: f32 = 340.0;
const DEFAULT_POPUP_HEIGHT: f32 = 280.0;
const MIN_POPUP_SIZE: f32 = 25.0;
const MAX_POPUP_WIDTH: f32 = 800.0;
const MAX_POPUP_HEIGHT: f32 = 600.0;

/// Active extension HTML popup currently displayed by the window.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenExtensionPopup {
    pub extension_id: ExtensionId,
    pub view_id: u64,
    pub width: f32,
    pub height: f32,
    pub ready: bool,
    pub anchor: Option<(f32, f32)>,
}

type PopupCloseCallback = Rc<dyn Fn(u64)>;

/// Reactive state for the window's active extension popup, shared via Freya context.
#[derive(Clone, Copy, PartialEq)]
pub struct ExtensionPopupState {
    pub open: State<Option<OpenExtensionPopup>>,
    pub close_callback: State<Option<PopupCloseCallback>>,
}

impl ExtensionPopupState {
    pub fn is_open(&self) -> bool {
        self.open.peek().is_some()
    }

    pub fn open_popup(&self, extension_id: ExtensionId, view_id: u64, anchor: Option<(f32, f32)>) {
        let mut open = self.open;
        open.set(Some(OpenExtensionPopup {
            extension_id,
            view_id,
            width: DEFAULT_POPUP_WIDTH,
            height: DEFAULT_POPUP_HEIGHT,
            ready: false,
            anchor,
        }));
    }

    pub fn resize_popup(&self, view_id: u64, width: f32, height: f32) {
        let mut open = self.open;
        let current = open.peek().clone();
        if let Some(current) = current
            && current.view_id == view_id
        {
            open.set(Some(OpenExtensionPopup {
                width: width.clamp(MIN_POPUP_SIZE, MAX_POPUP_WIDTH),
                height: height.clamp(MIN_POPUP_SIZE, MAX_POPUP_HEIGHT),
                ..current
            }));
        }
    }

    pub fn mark_ready(&self, view_id: u64) {
        let mut open = self.open;
        let current = open.peek().clone();
        if let Some(current) = current
            && current.view_id == view_id
            && !current.ready
        {
            open.set(Some(OpenExtensionPopup { ready: true, ..current }));
        }
    }

    /// Closes the active popup (if any) and returns the Hub `view_id` to destroy.
    pub fn take_closed_view(&self) -> Option<u64> {
        let mut open = self.open;
        let current = open.peek().clone()?;
        open.set(None);
        Some(current.view_id)
    }

    /// Closes the popup and invokes the registered Hub cleanup callback.
    pub fn request_close(&self) {
        if let Some(view_id) = self.take_closed_view() {
            let cb = self.close_callback.peek().clone();
            if let Some(cb) = cb {
                cb(view_id);
            }
        }
    }

    pub fn set_close_callback(&self, cb: impl Fn(u64) + 'static) {
        let mut slot = self.close_callback;
        slot.set(Some(Rc::new(cb)));
    }
}

pub fn use_provide_extension_popup_state() -> ExtensionPopupState {
    let open = use_state(|| None::<OpenExtensionPopup>);
    let close_callback = use_state(|| None::<PopupCloseCallback>);
    let state = ExtensionPopupState { open, close_callback };
    use_provide_context(move || state);
    state
}

pub fn use_extension_popup_state() -> ExtensionPopupState {
    consume_context::<ExtensionPopupState>()
}

pub fn try_use_extension_popup_state() -> Option<ExtensionPopupState> {
    try_consume_context::<ExtensionPopupState>()
}

/// Window-root overlay rendering the active extension's HTML popup inside an anchored card.
#[derive(Clone, PartialEq)]
pub struct ExtensionPopupOverlay;

impl Component for ExtensionPopupOverlay {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let popup_state = try_use_extension_popup_state();
        let root_size = *Platform::get().current_window().root_size.read();

        let Some(popup_state) = popup_state else {
            return rect().into_element();
        };
        let Some(popup) = popup_state.open.read().clone() else {
            return rect().into_element();
        };

        let win_w = root_size.width.max(360.0);
        let win_h = root_size.height.max(280.0);
        let card_w = popup.width.clamp(MIN_POPUP_SIZE, (win_w - 24.0).min(MAX_POPUP_WIDTH));
        let card_h = popup.height.clamp(MIN_POPUP_SIZE, (win_h - 64.0).min(MAX_POPUP_HEIGHT));

        let (left, top) = if let Some((ax, ay)) = popup.anchor {
            let x = if ax < 320.0 {
                (ax - 16.0).clamp(12.0, (win_w - card_w - 12.0).max(12.0))
            } else {
                (ax - card_w + 28.0).clamp(12.0, (win_w - card_w - 12.0).max(12.0))
            };
            let base_y = if ay < 46.0 { 44.0 } else { ay + 16.0 };
            let y = base_y.clamp(12.0, (win_h - card_h - 12.0).max(12.0));
            (x, y)
        } else {
            ((win_w - card_w - 16.0).max(12.0), 44.0)
        };

        let on_backdrop_down = move |e: Event<MouseEventData>| {
            e.stop_propagation();
            popup_state.request_close();
        };
        let on_key_down = move |e: Event<KeyboardEventData>| {
            if e.code == Code::Escape {
                e.stop_propagation();
                e.prevent_default();
                popup_state.request_close();
            }
        };

        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .on_mouse_down(on_backdrop_down)
            .on_global_key_down(on_key_down)
            .child(
                rect()
                    .position(Position::new_global().left(left).top(top))
                    .width(Size::px(card_w))
                    .height(Size::px(card_h))
                    .corner_radius(CornerRadius::new_all(10.0))
                    .overflow(Overflow::Clip)
                    .background(theme.color(ThemeRole::Surface))
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.color(ThemeRole::Border)),
                    )
                    .shadow(theme.shadows.float_drop.to_shadow())
                    .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
                    .child(
                        PageSurface::for_view(popup.view_id)
                            .width(Size::px(card_w))
                            .height(Size::px(card_h))
                            .key(format!("ext-popup-{}", popup.view_id)),
                    ),
            )
            .into_element()
    }
}
