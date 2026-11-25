/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! View-local state for the floating address bar, command palette (Actions mode), and Site Data Panel.
//!
//! Only view state lives here (open/closed flags, typed query text, highlighted row index, and panel anchor).
//! Result computation, palette entries, search engine settings, and site info all belong to `BrowserCore`.

use crate::ui::chrome::menus::AnchorRect;
use freya::prelude::*;

/// Operating mode of the floating address bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressBarMode {
    /// Standard address bar searching URLs, default search engine, open tabs, bookmarks, history, and inline actions.
    Normal { new_tab: bool },
    /// Command palette / Zen Actions mode (`Actions` chip active).
    Actions,
}

impl Default for AddressBarMode {
    fn default() -> Self {
        Self::Normal { new_tab: false }
    }
}

/// View-local context holding floating address bar and site data panel state for one window.
#[derive(Clone, Copy, PartialEq)]
pub struct AddressBarViewState {
    pub open: State<bool>,
    pub mode: State<AddressBarMode>,
    pub query: State<String>,
    pub highlighted: State<Option<usize>>,
    pub open_seq: State<u64>,
    pub input_a11y_id: AccessibilityId,
    pub site_panel_open: State<bool>,
    pub site_panel_anchor: State<AnchorRect>,
    /// The page URL the bar opened with, while it stands unedited. Like selected text, it gives way to what the user
    /// types (Freya's input cannot select text from outside).
    pub prefill: State<Option<String>>,
}

impl AddressBarViewState {
    /// Opens the floating address bar in normal mode, or toggles it closed if already open for a new tab
    /// (matching Zen's `Ctrl+T` toggle behavior).
    pub fn open_floating(&self, new_tab: bool, initial_query: Option<String>) {
        let mut open = self.open;
        let mut mode = self.mode;
        let mut query = self.query;
        let mut highlighted = self.highlighted;
        let mut seq = self.open_seq;

        let already_open = *open.peek();
        if already_open
            && new_tab
            && matches!(*mode.peek(), AddressBarMode::Normal { new_tab: true })
            && initial_query.is_none()
        {
            self.close();
            return;
        }

        self.close_site_panel();
        let text = initial_query.unwrap_or_default();
        let initial_highlight = if text.trim().is_empty() { None } else { Some(0) };
        let mut prefill = self.prefill;
        prefill.set((!new_tab && !text.trim().is_empty()).then(|| text.clone()));
        mode.set(AddressBarMode::Normal { new_tab });
        query.set(text);
        highlighted.set(initial_highlight);
        let next_seq = seq.peek().wrapping_add(1);
        seq.set(next_seq);
        open.set(true);
        self.input_a11y_id.request_focus();
    }

    /// Opens the floating address bar directly in Actions mode (command palette).
    pub fn open_actions(&self, initial_query: Option<String>) {
        let mut open = self.open;
        let mut mode = self.mode;
        let mut query = self.query;
        let mut highlighted = self.highlighted;
        let mut seq = self.open_seq;

        let already_open = *open.peek();
        if already_open && *mode.peek() == AddressBarMode::Actions && initial_query.is_none() {
            self.close();
            return;
        }

        self.close_site_panel();
        let text = initial_query.unwrap_or_default();
        let mut prefill = self.prefill;
        prefill.set_if_modified(None);
        mode.set(AddressBarMode::Actions);
        query.set(text);
        highlighted.set(Some(0));
        let next_seq = seq.peek().wrapping_add(1);
        seq.set(next_seq);
        open.set(true);
        self.input_a11y_id.request_focus();
    }

    /// Switches an already-open floating address bar into Actions mode (triggered by Tab on an empty query).
    pub fn enter_actions_mode(&self) {
        let mut mode = self.mode;
        let mut highlighted = self.highlighted;
        mode.set(AddressBarMode::Actions);
        highlighted.set(Some(0));
    }

    /// Leaves Actions mode back to normal address bar mode (triggered by Backspace on an empty query).
    pub fn exit_actions_mode(&self) {
        let mut mode = self.mode;
        let mut highlighted = self.highlighted;
        mode.set(AddressBarMode::Normal { new_tab: false });
        highlighted.set(None);
    }

    /// Closes the floating address bar.
    pub fn close(&self) {
        let mut open = self.open;
        let mut highlighted = self.highlighted;
        open.set_if_modified(false);
        highlighted.set(None);
        let mut prefill = self.prefill;
        prefill.set_if_modified(None);
        self.input_a11y_id.request_unfocus();
    }

    /// Replaces an unedited prefilled URL with what was typed into it. The input inserts the first character in front
    /// of the URL, so a query that is the URL with text before it becomes just that text.
    pub fn settle_prefill(&self) {
        let Some(original) = self.prefill.peek().clone() else {
            return;
        };
        let current = self.query.peek().clone();
        if current == original {
            return;
        }
        let mut prefill = self.prefill;
        prefill.set(None);
        if current.len() > original.len() && current.ends_with(&original) {
            let mut query = self.query;
            query.set(current[..current.len() - original.len()].to_string());
        }
    }

    /// Clears an unedited prefilled URL, as Backspace clears selected text. Returns whether it did.
    pub fn clear_prefill(&self) -> bool {
        if self.prefill.peek().is_none() {
            return false;
        }
        let mut prefill = self.prefill;
        prefill.set(None);
        let mut query = self.query;
        query.set(String::new());
        true
    }

    /// Keeps the prefilled URL as ordinary text, once the user moves the cursor into it.
    pub fn keep_prefill(&self) {
        let mut prefill = self.prefill;
        prefill.set_if_modified(None);
    }

    /// Returns whether the floating address bar is currently open.
    pub fn is_open(&self) -> bool {
        *self.open.peek()
    }

    /// Toggles the anchored Site Data Panel popover, updating its anchor when provided.
    pub fn toggle_site_panel(&self, anchor: Option<AnchorRect>) {
        let mut open = self.site_panel_open;
        let next = !*open.peek();
        if next {
            self.close();
            if let Some(rect) = anchor {
                let mut anchor_state = self.site_panel_anchor;
                anchor_state.set(rect);
            }
        }
        open.set(next);
    }

    /// Closes the Site Data Panel popover.
    pub fn close_site_panel(&self) {
        let mut open = self.site_panel_open;
        open.set_if_modified(false);
    }

    /// Returns whether either the floating address bar or the site panel is open.
    #[allow(dead_code)]
    pub fn has_open_overlay(&self) -> bool {
        *self.open.peek() || *self.site_panel_open.peek()
    }
}

/// Installs [`AddressBarViewState`] into Freya context for the current window.
pub fn use_provide_address_bar_state() -> AddressBarViewState {
    let open = use_state(|| false);
    let mode = use_state(AddressBarMode::default);
    let query = use_state(String::new);
    let highlighted = use_state(|| None::<usize>);
    let open_seq = use_state(|| 0u64);
    let input_a11y_id = use_hook(AccessibilityId::new_unique);
    let site_panel_open = use_state(|| false);
    let prefill = use_state(|| None::<String>);
    let site_panel_anchor = use_state(|| AnchorRect {
        x: 152.0,
        y: 44.0,
        width: 24.0,
        height: 24.0,
    });

    use_provide_context(|| AddressBarViewState {
        open,
        mode,
        query,
        highlighted,
        open_seq,
        input_a11y_id,
        site_panel_open,
        site_panel_anchor,
        prefill,
    })
}

/// Consumes the window's [`AddressBarViewState`] from Freya context.
pub fn use_address_bar_state() -> AddressBarViewState {
    use_consume::<AddressBarViewState>()
}

/// Consumes [`AddressBarViewState`] if a provider is installed in this window.
pub fn try_use_address_bar_state() -> Option<AddressBarViewState> {
    try_consume_context::<AddressBarViewState>()
}

/// Steps a highlight cursor across `count` rows by `delta` (`+1` or `-1`), wrapping around.
pub fn step_highlight(current: Option<usize>, count: usize, delta: isize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    match current {
        Some(idx) => Some(((idx as isize + delta).rem_euclid(count as isize)) as usize),
        None => {
            if delta >= 0 {
                Some(0)
            } else {
                Some(count - 1)
            }
        }
    }
}

/// Computes the horizontal width and vertical top offset of the floating address bar matching Zen's formulas.
pub fn compute_floating_bar_geometry(window_width: f32, window_height: f32) -> (f32, f32) {
    const FLOATING_URLBAR_MIN_HEIGHT: f32 = 333.0;
    const FLOATING_URLBAR_MAX_WIDTH: f32 = 750.0;
    let width = (window_width / 1.5)
        .min(FLOATING_URLBAR_MAX_WIDTH)
        .min((window_width - 32.0).max(280.0))
        .max(280.0);
    let top = ((window_height - FLOATING_URLBAR_MIN_HEIGHT) * 0.5).clamp(48.0, (window_height - 120.0).max(48.0));
    (width, top)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_highlight_with_wrap_around() {
        assert_eq!(step_highlight(None, 0, 1), None);
        assert_eq!(step_highlight(None, 4, 1), Some(0));
        assert_eq!(step_highlight(None, 4, -1), Some(3));
        assert_eq!(step_highlight(Some(0), 4, -1), Some(3));
        assert_eq!(step_highlight(Some(3), 4, 1), Some(0));
        assert_eq!(step_highlight(Some(1), 4, 1), Some(2));
    }

    #[test]
    fn computes_zen_floating_bar_geometry() {
        let (w, top) = compute_floating_bar_geometry(1600.0, 1000.0);
        assert_eq!(w, 750.0);
        assert!((top - 333.5).abs() < 1.0);

        let (narrow_w, _) = compute_floating_bar_geometry(600.0, 800.0);
        assert_eq!(narrow_w, 400.0);
    }
}
