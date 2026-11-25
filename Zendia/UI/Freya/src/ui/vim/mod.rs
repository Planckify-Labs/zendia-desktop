/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Vim mode overlays: statusline and mascot, which-key popup, hint labels, picker, and focus rings.

pub mod mascot;
pub mod mascot_data;
pub mod statusline;
pub mod which_key;

pub use mascot::{MASCOT_FULL_SIZE, MASCOT_MORPH_DURATION, MASCOT_REST_SCALE, VimMascot, mascot_mode_color};
pub use statusline::{
    STATUSLINE_CLEARANCE, STATUSLINE_CORNER_GAP, StatuslinePresence, VimStatusline, VimStatuslineState,
    try_use_vim_statusline_state, use_provide_vim_statusline_state, vim_mode_backdrop_endpoints,
};
pub use which_key::WhichKeyPopup;

use freya::prelude::*;

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
        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .interactive(Interactive::No)
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
