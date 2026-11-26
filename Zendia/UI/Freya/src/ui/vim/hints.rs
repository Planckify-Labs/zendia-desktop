/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Jump hint labels drawn over sidebar rows, toolbar items, and split pane chips (`ZendiaHints`).

use crate::ui::theme::{ResolvedTheme, ThemeRole};
use freya::prelude::*;
use zendia_browser_core::focus::FocusTarget;
use zendia_browser_core::model::BrowserModel;

/// Height of a standard row hint badge in logical pixels.
pub const HINT_BADGE_HEIGHT: f32 = 16.0;

/// Returns a positioned hint label badge for `target` inside a row of `row_height` logical pixels, or `None` when
/// vim mode is off or no matching hint is active for `target`.
pub fn hint_badge_for_row(
    model: &BrowserModel,
    target: &FocusTarget,
    row_height: f32,
    theme: &ResolvedTheme,
) -> Option<Element> {
    if !model.vim_mode_enabled {
        return None;
    }
    let hints = model.focus.hints.as_ref()?;
    let (prefix, suffix) = hints.label_parts(target)?;
    let top = ((row_height - HINT_BADGE_HEIGHT) * 0.5).max(2.0);
    Some(
        rect()
            .layer(Layer::Relative(8))
            .position(Position::new_absolute().right(4.0).top(top))
            .interactive(false)
            .child(hint_pill(prefix, suffix, HINT_BADGE_HEIGHT, 11.0, 4.0, 4.5, theme))
            .into_element(),
    )
}

/// Returns a hint badge pinned to the top-right corner (`top: 2px, right: 2px`) for compact tiles and footer buttons
/// so it clears centered favicons (`VIM-HT-03`).
pub fn hint_badge_for_corner(model: &BrowserModel, target: &FocusTarget, theme: &ResolvedTheme) -> Option<Element> {
    if !model.vim_mode_enabled {
        return None;
    }
    let hints = model.focus.hints.as_ref()?;
    let (prefix, suffix) = hints.label_parts(target)?;
    Some(
        rect()
            .layer(Layer::Relative(8))
            .position(Position::new_absolute().right(2.0).top(2.0))
            .interactive(false)
            .child(hint_pill(prefix, suffix, HINT_BADGE_HEIGHT, 11.0, 4.0, 4.0, theme))
            .into_element(),
    )
}

/// Renders an inline hint badge (used on window-mode pane chips, `13px`, `min-width: 20px`, `padding: 0 4px`, `VIM-HT-04`).
pub fn inline_hint_badge(prefix: &str, suffix: &str, theme: &ResolvedTheme) -> Element {
    hint_pill(prefix, suffix, 20.0, 13.0, 5.0, 4.0, theme)
}

fn hint_pill(
    prefix: &str,
    suffix: &str,
    height: f32,
    font_size: f32,
    radius: f32,
    pad_x: f32,
    theme: &ResolvedTheme,
) -> Element {
    let bg = theme.color(ThemeRole::Hint);
    let fg = theme.color(ThemeRole::HintText);
    let matched_fg = theme.rgba(ThemeRole::HintText).with_alpha(0.50).to_color();

    let mut pill = rect()
        .min_width(Size::px(height))
        .height(Size::px(height))
        .padding(Gaps::new(0.0, pad_x, 0.0, pad_x))
        .corner_radius(CornerRadius::new_all(radius))
        .background(bg)
        .direction(Direction::Horizontal)
        .main_align(Alignment::center())
        .cross_align(Alignment::center())
        .overflow(Overflow::Clip);

    if !prefix.is_empty() {
        pill = pill.child(
            label()
                .text(prefix.to_string())
                .font_size(font_size)
                .font_weight(FontWeight::BOLD)
                .font_family("monospace")
                .color(matched_fg)
                .max_lines(1),
        );
    }
    if !suffix.is_empty() {
        pill = pill.child(
            label()
                .text(suffix.to_string())
                .font_size(font_size)
                .font_weight(FontWeight::BOLD)
                .font_family("monospace")
                .color(fg)
                .max_lines(1),
        );
    }

    pill.into_element()
}
