/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Anchored floating popover hosting a WebExtension's live HTML popup page via a Hub auxiliary [`PageSurface`],
//! including automatic normal-mode keyboard hint badges (`adfewocmxzbqt`, `EXT-01`, `TB-05`).

use crate::ui::core_view::use_core;
use crate::ui::kit::{PopupPresence, use_last_some};
use crate::ui::page::PageSurface;
use crate::ui::theme::{ThemeRole, use_theme};
use crate::ui::vim::page_hints::PageHintsOverlay;
use freya::prelude::*;
use std::rc::Rc;
use std::time::Duration;
use zendia_automation::PageTarget;
use zendia_browser_core::model::{BrowserModel, InputMode, TabId};
use zendia_browser_core::modules::ExtensionId;

const DEFAULT_POPUP_WIDTH: f32 = 340.0;
const DEFAULT_POPUP_HEIGHT: f32 = 280.0;
const MIN_POPUP_SIZE: f32 = 25.0;
const MAX_POPUP_WIDTH: f32 = 800.0;
const MAX_POPUP_HEIGHT: f32 = 600.0;

/// Unmapped normal-mode letters used on keyboard-opened extension popups (`ZendiaPopupHints.sys.mjs`, `EXT-01`).
pub const POPUP_HINT_LETTERS: &str = "adfewocmxzbqt";

const BADGE_BG: Color = Color::from_rgb(0xa2, 0x57, 0x72);
const BADGE_FG: Color = Color::from_rgb(0xee, 0xf5, 0xff);
const BADGE_TYPED_BG: Color = Color::from_rgb(0xff, 0xc7, 0x77);
const BADGE_TYPED_FG: Color = Color::from_rgb(0x1b, 0x1d, 0x2b);
const BADGE_SHADOW: Color = Color::from_argb(102, 0, 0, 0);

/// Assigns up to 169 popup hint labels from [`POPUP_HINT_LETTERS`]:
/// 1 letter when `count <= 13`, 2 letters when `count > 13` (`EXT-01`).
pub fn assign_popup_hint_labels(count: usize) -> Vec<String> {
    let chars: Vec<char> = POPUP_HINT_LETTERS.chars().collect();
    let base = chars.len();
    if count == 0 || base == 0 {
        return Vec::new();
    }
    if count <= base {
        return chars.into_iter().take(count).map(|c| c.to_string()).collect();
    }
    let cap = count.min(base * base);
    let mut out = Vec::with_capacity(cap);
    'outer: for &first in &chars {
        for &second in &chars {
            if out.len() >= cap {
                break 'outer;
            }
            let mut s = String::with_capacity(2);
            s.push(first);
            s.push(second);
            out.push(s);
        }
    }
    out
}

/// Active extension HTML popup currently displayed by the window.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenExtensionPopup {
    pub extension_id: ExtensionId,
    pub view_id: u64,
    pub width: f32,
    pub height: f32,
    pub ready: bool,
    pub anchor: Option<(f32, f32)>,
    /// True when opened from the keyboard (`<leader>x`, Toolbar mode, or `ublock.popup`, `EXT-01`).
    pub keyboard_hints: bool,
    /// Scanned top-document interactive controls and fields (`EXT-01`).
    pub targets: Vec<PageTarget>,
    /// First letter of a two-letter popup hint typed so far (`EXT-01`).
    pub typed: String,
}

type PopupCloseCallback = Rc<dyn Fn(u64)>;
type PopupScanCallback = Rc<dyn Fn(u64)>;
type PopupActivateCallback = Rc<dyn Fn(u64, &PageTarget)>;

/// Reactive state for the window's active extension popup, shared via Freya context.
#[derive(Clone, Copy, PartialEq)]
pub struct ExtensionPopupState {
    pub open: State<Option<OpenExtensionPopup>>,
    pub close_callback: State<Option<PopupCloseCallback>>,
    pub scan_callback: State<Option<PopupScanCallback>>,
    pub activate_callback: State<Option<PopupActivateCallback>>,
}

impl ExtensionPopupState {
    pub fn is_open(&self) -> bool {
        self.open.peek().is_some()
    }

    pub fn open_popup(&self, extension_id: ExtensionId, view_id: u64, anchor: Option<(f32, f32)>) {
        self.open_popup_with_hints(extension_id, view_id, anchor, false);
    }

    pub fn open_popup_with_hints(
        &self,
        extension_id: ExtensionId,
        view_id: u64,
        anchor: Option<(f32, f32)>,
        keyboard_hints: bool,
    ) {
        let mut open = self.open;
        let preserved = open
            .peek()
            .as_ref()
            .is_some_and(|cur| cur.view_id == view_id && cur.keyboard_hints);
        let enable_hints = keyboard_hints || preserved;
        open.set(Some(OpenExtensionPopup {
            extension_id,
            view_id,
            width: DEFAULT_POPUP_WIDTH,
            height: DEFAULT_POPUP_HEIGHT,
            ready: false,
            anchor,
            keyboard_hints: enable_hints,
            targets: Vec::new(),
            typed: String::new(),
        }));
        if enable_hints {
            self.request_scan_burst(view_id);
        }
    }

    pub fn resize_popup(&self, view_id: u64, width: f32, height: f32) {
        let mut open = self.open;
        let current = open.peek().clone();
        if let Some(current) = current
            && current.view_id == view_id
        {
            let hints = current.keyboard_hints;
            open.set(Some(OpenExtensionPopup {
                width: width.clamp(MIN_POPUP_SIZE, MAX_POPUP_WIDTH),
                height: height.clamp(MIN_POPUP_SIZE, MAX_POPUP_HEIGHT),
                ..current
            }));
            if hints {
                self.request_scan(view_id);
            }
        }
    }

    pub fn mark_ready(&self, view_id: u64) {
        let mut open = self.open;
        let current = open.peek().clone();
        if let Some(current) = current
            && current.view_id == view_id
        {
            let hints = current.keyboard_hints;
            if !current.ready {
                open.set(Some(OpenExtensionPopup { ready: true, ..current }));
            }
            if hints {
                self.request_scan_burst(view_id);
            }
        }
    }

    /// Re-scans popup targets when DOM mutations, scroll, or load-finish occur inside `view_id` (`EXT-01`).
    pub fn on_view_updated(&self, view_id: u64) {
        let should_scan = self
            .open
            .peek()
            .as_ref()
            .is_some_and(|p| p.view_id == view_id && p.keyboard_hints && p.typed.is_empty());
        if should_scan {
            self.request_scan(view_id);
        }
    }

    /// Stores scanned popup targets unless a two-letter label is currently half-typed (`EXT-01`).
    pub fn set_targets(&self, view_id: u64, targets: Vec<PageTarget>) {
        let mut open = self.open;
        let current = open.peek().clone();
        if let Some(mut current) = current
            && current.view_id == view_id
            && current.keyboard_hints
            && current.typed.is_empty()
        {
            current.targets = targets;
            open.set(Some(current));
        }
    }

    /// Triggers one scan of `view_id` if it is still open with keyboard hints and no half-typed prefix.
    pub fn request_scan(&self, view_id: u64) {
        let active = self
            .open
            .peek()
            .as_ref()
            .is_some_and(|p| p.view_id == view_id && p.keyboard_hints && p.typed.is_empty());
        if active && let Some(cb) = self.scan_callback.peek().clone() {
            cb(view_id);
        }
    }

    /// Schedules scans at `100 ms`, `300 ms`, and `800 ms` matching `ZendiaPopupHints.sys.mjs` (`EXT-01`).
    pub fn request_scan_burst(&self, view_id: u64) {
        for delay_ms in [100_u64, 300, 800] {
            let state = *self;
            spawn(async move {
                timer(Duration::from_millis(delay_ms)).await;
                state.request_scan(view_id);
            });
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

    pub fn set_scan_callback(&self, cb: impl Fn(u64) + 'static) {
        let mut slot = self.scan_callback;
        slot.set(Some(Rc::new(cb)));
    }

    pub fn set_activate_callback(&self, cb: impl Fn(u64, &PageTarget) + 'static) {
        let mut slot = self.activate_callback;
        slot.set(Some(Rc::new(cb)));
    }

    fn hints_active_in_model(popup: &OpenExtensionPopup, model: &BrowserModel) -> bool {
        popup.keyboard_hints
            && model.vim_mode_enabled
            && model.keyed_page == Some(TabId(popup.view_id))
            && model.input_mode == InputMode::Normal
            && !model.page_editing
            && model.page_hints.is_none()
            && !popup.targets.is_empty()
    }

    /// Handles popup hint label keys and `<Esc>` before the popup's vim engine (`EXT-01`).
    /// Returns `true` when consumed; `false` lets the key proceed to the popup's vim engine (`j`, `k`, `s`, `v`, `i`, etc.).
    pub fn handle_key_down(&self, e: &KeyboardEventData, model: &BrowserModel) -> bool {
        let current = self.open.peek().clone();
        let Some(popup) = current else {
            return false;
        };
        if !Self::hints_active_in_model(&popup, model) {
            return false;
        }
        if crate::input::key_stroke_from_event(&e.key, e.code, e.modifiers).is_none() {
            return false;
        }
        if e.modifiers
            .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
        {
            if !popup.typed.is_empty() {
                let mut open = self.open;
                open.set(Some(OpenExtensionPopup {
                    typed: String::new(),
                    ..popup
                }));
            }
            return false;
        }
        if e.key == Key::Named(NamedKey::Escape) {
            if !popup.typed.is_empty() {
                let mut open = self.open;
                open.set(Some(OpenExtensionPopup {
                    typed: String::new(),
                    ..popup
                }));
            } else {
                self.request_close();
            }
            return true;
        }
        if let Key::Character(ch) = &e.key
            && ch.chars().count() == 1
        {
            let labels = assign_popup_hint_labels(popup.targets.len());
            let candidate = format!("{}{}", popup.typed, ch);
            if let Some(idx) = labels.iter().position(|lbl| lbl == &candidate) {
                let target = popup.targets[idx].clone();
                let vid = popup.view_id;
                if !popup.typed.is_empty() {
                    let mut open = self.open;
                    open.set(Some(OpenExtensionPopup {
                        typed: String::new(),
                        ..popup
                    }));
                }
                if let Some(cb) = self.activate_callback.peek().clone() {
                    cb(vid, &target);
                }
                self.request_scan_burst(vid);
                return true;
            }
            if labels.iter().any(|lbl| lbl.starts_with(&candidate)) {
                let mut open = self.open;
                open.set(Some(OpenExtensionPopup {
                    typed: candidate,
                    ..popup
                }));
                return true;
            }
        }
        if !popup.typed.is_empty() {
            let mut open = self.open;
            open.set(Some(OpenExtensionPopup {
                typed: String::new(),
                ..popup
            }));
        }
        false
    }
}

/// Non-mutating check used by [`PageSurface`] so popup hint keys are not forwarded to WebContent (`EXT-01`).
pub fn popup_hints_will_consume(e: &KeyboardEventData, model: &BrowserModel) -> bool {
    let Some(state) = try_use_extension_popup_state() else {
        return false;
    };
    let current = state.open.peek().clone();
    let Some(popup) = current else {
        return false;
    };
    if !ExtensionPopupState::hints_active_in_model(&popup, model) {
        return false;
    }
    if e.modifiers
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
    {
        return false;
    }
    if e.key == Key::Named(NamedKey::Escape) {
        return true;
    }
    if let Key::Character(ch) = &e.key
        && ch.chars().count() == 1
    {
        let labels = assign_popup_hint_labels(popup.targets.len());
        let candidate = format!("{}{}", popup.typed, ch);
        return labels
            .iter()
            .any(|lbl| lbl == &candidate || lbl.starts_with(&candidate));
    }
    false
}

pub fn use_provide_extension_popup_state() -> ExtensionPopupState {
    let open = use_state(|| None::<OpenExtensionPopup>);
    let close_callback = use_state(|| None::<PopupCloseCallback>);
    let scan_callback = use_state(|| None::<PopupScanCallback>);
    let activate_callback = use_state(|| None::<PopupActivateCallback>);
    let state = ExtensionPopupState {
        open,
        close_callback,
        scan_callback,
        activate_callback,
    };
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
        let core = use_core();
        let theme = use_theme();
        let popup_state = try_use_extension_popup_state();
        let root_size = *Platform::get().current_window().root_size.read();

        let open_popup = popup_state.and_then(|state| state.open.read().clone());
        let is_open = open_popup.is_some();
        // A closed popup stays drawn while it fades out.
        let shown = use_last_some(&open_popup);
        let (Some(popup_state), Some(popup)) = (popup_state, shown) else {
            return rect().into_element();
        };

        let show_auto_hints = {
            let model = core.model().read();
            ExtensionPopupState::hints_active_in_model(&popup, &model)
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

        let auto_badges: Option<Element> = show_auto_hints.then(|| {
            let labels = assign_popup_hint_labels(popup.targets.len());
            let max_bx = (card_w - 24.0).max(0.0);
            let max_by = (card_h - 18.0).max(0.0);
            let mut badges = Vec::new();
            for (target, full_label) in popup.targets.iter().zip(labels) {
                if !full_label.starts_with(&popup.typed) {
                    continue;
                }
                let bx = (target.x + 2.0).clamp(0.0, max_bx);
                let by = (target.y + 2.0).clamp(0.0, max_by);
                let typed_len = popup.typed.len();
                let typed_part = &full_label[..typed_len];
                let rest_part = &full_label[typed_len..];
                let badge = rect()
                    .key(format!("popup-hint-{}-{full_label}", target.id))
                    .position(Position::new_absolute().left(bx).top(by))
                    .height(Size::px(16.0))
                    .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
                    .corner_radius(CornerRadius::new_all(4.0))
                    .background(BADGE_BG)
                    .shadow(Shadow::new().x(0.0).y(1.0).blur(3.0).color(BADGE_SHADOW))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .interactive(Interactive::No)
                    .maybe_child((!typed_part.is_empty()).then(|| {
                        rect()
                            .background(BADGE_TYPED_BG)
                            .corner_radius(CornerRadius::new_all(2.0))
                            .child(
                                label()
                                    .text(typed_part.to_string())
                                    .font_size(11.0)
                                    .font_weight(FontWeight::BOLD)
                                    .font_family("monospace")
                                    .color(BADGE_TYPED_FG),
                            )
                    }))
                    .child(
                        label()
                            .text(rest_part.to_string())
                            .font_size(11.0)
                            .font_weight(FontWeight::BOLD)
                            .font_family("monospace")
                            .color(BADGE_FG),
                    )
                    .into_element();
                badges.push(badge);
            }
            rect()
                .position(Position::new_absolute().left(0.0).top(0.0))
                .width(Size::px(card_w))
                .height(Size::px(card_h))
                .interactive(Interactive::No)
                .children(badges)
                .into_element()
        });

        let on_backdrop_down = move |e: Event<MouseEventData>| {
            e.stop_propagation();
            popup_state.request_close();
        };

        // It rises and fades in like the browser's own popups. The page inside is not scaled.
        PopupPresence::anchored(is_open, (left, top))
            .open_seq(popup.view_id)
            .animate_scale(false)
            .on_backdrop_mouse_down(on_backdrop_down)
            .child(
                rect()
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
                    )
                    .child(PageHintsOverlay::new(Some(TabId(popup.view_id)), (card_w, card_h)))
                    .maybe_child(auto_badges),
            )
            .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_hint_labels_use_single_letter_up_to_13_and_two_letters_up_to_169() {
        assert!(assign_popup_hint_labels(0).is_empty());
        let l13 = assign_popup_hint_labels(13);
        assert_eq!(l13.len(), 13);
        assert_eq!(l13[0], "a");
        assert_eq!(l13[1], "d");
        assert_eq!(l13[12], "t");

        let l14 = assign_popup_hint_labels(14);
        assert_eq!(l14.len(), 14);
        assert_eq!(l14[0], "aa");
        assert_eq!(l14[1], "ad");
        assert_eq!(l14[13], "da");

        let l200 = assign_popup_hint_labels(200);
        assert_eq!(l200.len(), 169);
        assert_eq!(l200[168], "tt");
    }
}
