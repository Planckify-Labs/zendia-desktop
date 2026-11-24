/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Reactive theme provider and consumer hooks backed by [`BrowserCore`]'s profile theme settings.

#![allow(dead_code, unused_imports)]

pub mod data;
pub mod motion;

pub use data::{
    ColorPair, FontSizes, GlassSpec, Palette, Radii, ResolvedScheme, ResolvedTheme, Rgba, SemanticColors, ShadowSpec,
    Shadows, Spacing, ThemeRole,
};

use crate::ui::core_view::CoreView;
use freya::prelude::*;

/// Freya context wrapper carrying the reactive resolved theme.
#[derive(Clone, PartialEq)]
pub struct ThemeContext {
    theme: State<ResolvedTheme>,
}

impl ThemeContext {
    pub fn state(&self) -> State<ResolvedTheme> {
        self.theme
    }
}

/// Resolves the theme from `core`'s profile settings and the platform's light/dark preference, keeps it reactive, and
/// installs [`ThemeContext`] into Freya context.
pub fn use_provide_theme(core: &CoreView) -> State<ResolvedTheme> {
    let model = *core.model();
    let preferred = Platform::get().current_window().preferred_theme;

    let mut theme = use_state(|| {
        let system_is_dark = matches!(*preferred.peek(), PreferredTheme::Dark);
        ResolvedTheme::resolve(&model.peek().theme, system_is_dark)
    });

    // Keep resolved theme synchronized whenever the core's ThemeSettings or OS dark-mode preference changes.
    let system_is_dark = matches!(*preferred.read(), PreferredTheme::Dark);
    let next = ResolvedTheme::resolve(&model.read().theme, system_is_dark);
    if *theme.peek() != next {
        theme.set(next);
    }

    use_provide_context(|| ThemeContext { theme });
    theme
}

/// Provides a caller-managed [`State<ResolvedTheme>`] into Freya context (useful for standalone views and galleries).
#[allow(dead_code)]
pub fn use_provide_theme_state(theme: State<ResolvedTheme>) {
    use_provide_context(|| ThemeContext { theme });
}

/// Reads the current [`ResolvedTheme`] from Freya context, falling back to the default dark Tokyonight theme when no
/// provider is installed.
pub fn use_theme() -> ResolvedTheme {
    match try_consume_context::<ThemeContext>() {
        Some(ctx) => ctx.state().read().clone(),
        None => ResolvedTheme::default(),
    }
}
