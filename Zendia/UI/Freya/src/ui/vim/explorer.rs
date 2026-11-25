/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Explorer focus visuals over sidebar rows: EXPLORER blue cursor ring and VISUAL purple selection tint and ring.

use crate::ui::theme::{ResolvedTheme, ThemeRole};
use freya::prelude::*;
use zendia_browser_core::focus::FocusTarget;
use zendia_browser_core::model::{BrowserModel, InputMode};

/// Width of the explorer cursor and selection outline in logical pixels.
const EXPLORER_RING_WIDTH: f32 = 2.0;

/// Whether the explorer surface is currently active and visible.
pub fn is_explorer_active(model: &BrowserModel) -> bool {
    model.vim_mode_enabled && model.input_mode == InputMode::Explorer
}

/// Applies the explorer cursor (blue) or selection (purple) ring over `base` when `target` is focused or selected.
pub fn apply_explorer_border(
    base: Border,
    model: &BrowserModel,
    target: &FocusTarget,
    theme: &ResolvedTheme,
) -> Border {
    if !is_explorer_active(model) {
        return base;
    }
    if model.focus.is_cursor(target) {
        Border::new()
            .width(EXPLORER_RING_WIDTH)
            .alignment(BorderAlignment::Inner)
            .fill(theme.color(ThemeRole::ModeExplorer))
    } else if model.focus.is_selected(target) {
        Border::new()
            .width(EXPLORER_RING_WIDTH)
            .alignment(BorderAlignment::Inner)
            .fill(theme.color(ThemeRole::ModeVisual))
    } else {
        base
    }
}

/// Applies the explorer selection tint over `base` when `target` is selected in explorer mode.
pub fn apply_explorer_background(
    base: Color,
    model: &BrowserModel,
    target: &FocusTarget,
    theme: &ResolvedTheme,
) -> Color {
    if !is_explorer_active(model) {
        return base;
    }
    if model.focus.is_selected(target) {
        theme.rgba(ThemeRole::ModeVisual).with_alpha(0.25).to_color()
    } else {
        base
    }
}
