/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Toolbar focus cursor ring over toolbar buttons and the address bar in TOOLBAR yellow (`ZendiaToolbar`).

use crate::ui::theme::{ResolvedTheme, ThemeRole};
use freya::prelude::*;
use zendia_browser_core::contributions::ContributionId;
use zendia_browser_core::focus::FocusTarget;
use zendia_browser_core::model::{BrowserModel, InputMode};

/// Width of the toolbar focus ring in logical pixels.
const TOOLBAR_RING_WIDTH: f32 = 2.0;

/// Whether toolbar focus mode is active and its cursor rests on `item_id`.
pub fn is_toolbar_cursor(model: &BrowserModel, item_id: &str) -> bool {
    model.vim_mode_enabled
        && model.input_mode == InputMode::Toolbar
        && model
            .focus
            .is_cursor(&FocusTarget::ToolbarItem(ContributionId::new(item_id)))
}

/// Applies the TOOLBAR yellow cursor ring over `base` when `item_id` holds the toolbar cursor.
pub fn apply_toolbar_cursor_border(base: Border, model: &BrowserModel, item_id: &str, theme: &ResolvedTheme) -> Border {
    if is_toolbar_cursor(model, item_id) {
        Border::new()
            .width(TOOLBAR_RING_WIDTH)
            .alignment(BorderAlignment::Inner)
            .fill(theme.color(ThemeRole::ModeToolbar))
    } else {
        base
    }
}

/// Returns `base` unchanged: fork toolbar focus draws only the crisp `2px` yellow outline with no extra fill (`VIM-TB-04`).
pub fn apply_toolbar_cursor_bg(base: Color, _model: &BrowserModel, _item_id: &str, _theme: &ResolvedTheme) -> Color {
    base
}
