/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The library's Media section.

use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;

/// Width of this section's body next to the rail.
pub const WIDTH: f32 = 420.0;

#[derive(Clone, PartialEq)]
pub struct MediaSection;

impl Component for MediaSection {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .padding(Gaps::new_all(16.0))
            .child(label().text("Media").color(theme.color(ThemeRole::TextMuted)))
    }
}
