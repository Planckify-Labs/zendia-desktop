/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! View-local state for window chrome surfaces (app menu open flag, about dialog flag, and menu anchor area).
//!
//! Provided once at the window root (and in the UI gallery) so the toolbar and menus share view state without storing
//! ephemeral UI flags inside `BrowserCore`.

use freya::prelude::*;

/// Reactive view-local chrome state shared across window shell components and menus.
#[derive(Clone, Copy, PartialEq)]
pub struct ChromeViewState {
    pub app_menu_open: State<bool>,
    pub about_open: State<bool>,
    pub app_menu_anchor: State<Option<Area>>,
}

impl ChromeViewState {
    /// Toggles whether the application menu popup is open.
    pub fn toggle_app_menu(&self) {
        let mut open = self.app_menu_open;
        let next = !*open.peek();
        open.set(next);
    }

    /// Closes the application menu popup if it is open.
    #[allow(dead_code)]
    pub fn close_app_menu(&self) {
        let mut open = self.app_menu_open;
        open.set_if_modified(false);
    }

    /// Records the on-screen bounding rectangle of the app menu trigger button for popup anchoring.
    pub fn set_app_menu_anchor(&self, area: Area) {
        let mut anchor = self.app_menu_anchor;
        anchor.set_if_modified(Some(area));
    }

    /// Opens the About Zendia dialog.
    #[allow(dead_code)]
    pub fn open_about(&self) {
        let mut about = self.about_open;
        about.set_if_modified(true);
    }

    /// Closes the About Zendia dialog.
    #[allow(dead_code)]
    pub fn close_about(&self) {
        let mut about = self.about_open;
        about.set_if_modified(false);
    }
}

/// Creates and provides [`ChromeViewState`] into Freya context for the current window or gallery preview.
pub fn use_provide_chrome_view_state() -> ChromeViewState {
    let app_menu_open = use_state(|| false);
    let about_open = use_state(|| false);
    let app_menu_anchor = use_state(|| None::<Area>);
    use_provide_context(|| ChromeViewState {
        app_menu_open,
        about_open,
        app_menu_anchor,
    })
}

/// Consumes [`ChromeViewState`] from Freya context.
#[allow(dead_code)]
pub fn use_chrome_view_state() -> ChromeViewState {
    use_consume::<ChromeViewState>()
}

/// Consumes [`ChromeViewState`] if a provider is present in the component tree.
pub fn try_use_chrome_view_state() -> Option<ChromeViewState> {
    try_consume_context::<ChromeViewState>()
}
