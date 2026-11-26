/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Vim mode overlays: statusline and mascot, which-key popup, hint labels, picker, and focus rings.

pub mod explorer;
pub mod hints;
pub mod mascot;
pub mod mascot_data;
pub mod page_hints;
pub mod picker;
pub mod region_picker;
pub mod statusline;
pub mod toolbar_focus;
pub mod which_key;
pub mod window_mode;

pub use picker::{
    VimPickerCard, VimPickerOverlay, VimPickerViewState, try_use_vim_picker_state, use_provide_vim_picker_state,
    use_vim_picker_state,
};
pub use region_picker::{
    RegionPickerOverlay, RegionPickerSurface, RegionPickerViewState, try_use_region_picker_state,
    use_provide_region_picker_state, use_region_picker_state,
};

pub use mascot::{MASCOT_FULL_SIZE, MASCOT_MORPH_DURATION, MASCOT_REST_SCALE, VimMascot, mascot_mode_color};
pub use statusline::{
    STATUSLINE_CLEARANCE, STATUSLINE_CORNER_GAP, StatuslinePresence, VimStatusline, VimStatuslineState,
    try_use_vim_statusline_state, use_provide_vim_statusline_state, vim_mode_backdrop_endpoints,
};
pub use which_key::WhichKeyPopup;

use crate::ui::core_view::CoreView;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{BrowserModel, InputMode};

/// Combined vim statusline (mascot, typed keys, announcements) and which-key popup floated over the page.
///
/// Replaces phase 0's `DebugInputOverlay`. Renders nothing when `model.vim_mode_enabled` is false.
#[derive(Clone, PartialEq)]
pub struct VimOverlayLayer {
    pub force_shrunk: bool,
    pub force_ducked: bool,
    pub extra_panel_clearance: f32,
    pub static_preview: bool,
    pub key: DiffKey,
}

impl Default for VimOverlayLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyExt for VimOverlayLayer {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl VimOverlayLayer {
    pub fn new() -> Self {
        Self {
            force_shrunk: false,
            force_ducked: false,
            extra_panel_clearance: 0.0,
            static_preview: false,
            key: DiffKey::None,
        }
    }

    pub fn force_shrunk(mut self, shrunk: bool) -> Self {
        self.force_shrunk = shrunk;
        self
    }

    pub fn force_ducked(mut self, ducked: bool) -> Self {
        self.force_ducked = ducked;
        self
    }

    pub fn extra_panel_clearance(mut self, clearance: f32) -> Self {
        self.extra_panel_clearance = clearance;
        self
    }

    pub fn static_preview(mut self, static_preview: bool) -> Self {
        self.static_preview = static_preview;
        self
    }
}

impl Component for VimOverlayLayer {
    fn render(&self) -> impl IntoElement {
        let vim_state = try_use_vim_statusline_state();
        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .interactive(Interactive::No)
            .on_sized(move |e: Event<SizedEventData>| {
                if let Some(state) = vim_state {
                    state.set_stage_size(e.area.width(), e.area.height());
                }
            })
            .child(VimStatusline {
                force_shrunk: self.force_shrunk,
                force_ducked: self.force_ducked,
                extra_panel_clearance: self.extra_panel_clearance,
                static_preview: self.static_preview,
                key: DiffKey::None,
            })
            .child(WhichKeyPopup::new().static_preview(self.static_preview))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Whether an active vim focus surface (explorer, toolbar, or window mode) should capture plain keystrokes instead of
/// forwarding them to the focused web page.
pub fn focus_mode_captures_keys(model: &BrowserModel) -> bool {
    model.vim_mode_enabled
        && matches!(
            model.input_mode,
            InputMode::Explorer | InputMode::Toolbar | InputMode::Window
        )
}

/// Whether the browser keeps a key from the focused page whatever the key engine makes of it: hint labels take every
/// key, and the explorer, the toolbar and the library are the browser's own. In window mode a key the mode has no use
/// for ends it and reaches the page, as in the old fork.
pub fn surface_keeps_keys(model: &BrowserModel) -> bool {
    model.vim_mode_enabled
        && (model.focus.hints.is_some()
            || model.page_hints.is_some()
            || matches!(
                model.input_mode,
                InputMode::Explorer | InputMode::Toolbar | InputMode::Library
            ))
}

/// Gives the active hint labels of a vim focus surface or native page-hint overlay their keys. Every other key of the
/// explorer, the toolbar and window mode goes to the vim key engine.
///
/// Returns `true` when the key was handled and should not fall through to the key engine or the page.
pub fn handle_focus_key_down(e: &KeyboardEventData, core: &CoreView) -> bool {
    let chord = e
        .modifiers
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META);

    let (vim_enabled, has_page_hints, has_focus_hints) = {
        let model = core.model().peek();
        (
            model.vim_mode_enabled,
            model.page_hints.is_some(),
            focus_mode_captures_keys(&model) && model.focus.hints.is_some(),
        )
    };
    if !vim_enabled {
        return false;
    }

    if has_page_hints {
        use zendia_browser_core::page_actions::{HintKey, PageAction, hint_key_from_notation};
        // A bare modifier is no key.
        let Some(stroke) = crate::input::key_stroke_from_event(&e.key, e.code, e.modifiers) else {
            return true;
        };
        let key = if chord {
            HintKey::Chord
        } else {
            hint_key_from_notation(&stroke.to_string())
        };
        // Labels are letters: a key with Ctrl, Alt or Super ends the jump and goes on to the keymap.
        let passes = key == HintKey::Chord;
        core.invoke_or_log(Command::Page(PageAction::PageHintInput { key }));
        return !passes;
    }

    if chord {
        return false;
    }

    if !has_focus_hints {
        return false;
    }

    match &e.key {
        Key::Named(NamedKey::Escape) => {
            core.invoke_or_log(Command::FocusHintCancel);
        }
        Key::Character(text) if text.chars().count() == 1 => {
            core.invoke_or_log(Command::FocusHintInput {
                key: text.to_ascii_lowercase(),
            });
        }
        _ => {
            core.invoke_or_log(Command::FocusHintCancel);
        }
    }
    true
}

/// A key that window mode had no use for ends it, and the key does its usual job. A bare modifier is no key.
pub fn leave_window_mode_on_unused_key(e: &KeyboardEventData, core: &CoreView) {
    if crate::input::key_stroke_from_event(&e.key, e.code, e.modifiers).is_none() {
        return;
    }
    let in_window_mode = {
        let model = core.model().peek();
        model.vim_mode_enabled && model.input_mode == InputMode::Window
    };
    if in_window_mode {
        core.invoke_or_log(Command::WindowModeClose);
    }
}

/// How long after the page scrolls the labels and the visual mode cursor are placed again, for a smooth scroll to
/// get there.
const FOLLOW_SCROLL_DELAY: std::time::Duration = std::time::Duration::from_millis(180);

/// The page is scrolling: a jump's labels and the visual mode cursor follow it once it settles.
pub fn follow_page_scroll(core: &CoreView) {
    let follows = {
        let model = core.model().peek();
        model.vim_mode_enabled && (model.page_hints.is_some() || model.input_mode == InputMode::Visual)
    };
    if !follows {
        return;
    }
    let core = core.clone();
    spawn(async move {
        timer(FOLLOW_SCROLL_DELAY).await;
        core.invoke_or_log(Command::Page(
            zendia_browser_core::page_actions::PageAction::PageHintRefresh,
        ));
    });
}
